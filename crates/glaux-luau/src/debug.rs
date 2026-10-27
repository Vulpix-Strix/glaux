//! Breakpoints, stepping and inspection for a Luau VM that is being
//! debugged.
//!
//! A debugged VM runs on its own thread and blocks inside Luau's `debugbreak`
//! hook while it is stopped, as [`glaux_script::debug`] describes.
//!
//! Breakpoints are Luau's own `LOP_BREAK` patches, so they cost nothing until
//! reached. Stepping cannot use `lua_singlestep`: Luau picks its single-step
//! interpreter when `luau_execute` is entered, so turning it on from a hook
//! does not reach the Lua calls already running. Instead a step puts a
//! breakpoint on every line of every loaded chunk and the hook decides, from
//! the call depth and line, whether the step is over.

mod inspect;

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::ffi::{CStr, CString, c_int};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;

use glaux_script::debug::{
    Breakpoint, DebugPort, DebugReply, Inspect, Resume, Step, Stop, StopReason,
};
use glaux_script::{LogEntry, LogSink};
use mlua::chunk::Compiler;
use mlua::ffi::{self, lua_Debug, lua_State};
use mlua::{Function, Lua};

/// Registry key of the table holding the main function of every loaded
/// chunk, by path. Scripts cannot reach the registry.
const CHUNKS: &CStr = c"glaux.debug.chunks";

/// What the hook does at a line that is not a user breakpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Run,
    Into { depth: c_int, line: c_int },
    Over { depth: c_int, line: c_int },
    Out { depth: c_int },
}

struct Chunk {
    path: String,
    lines: u32,
}

/// The debugger state of one VM.
pub(crate) struct Debugger {
    port: DebugPort,
    output: Rc<LogSink>,
    main: *mut lua_State,
    mode: Cell<Mode>,
    breakpoints: RefCell<Vec<Breakpoint>>,
    /// Where the breakpoints actually are: a request on a line without code
    /// lands on the next line that has some.
    landed: RefCell<Vec<(String, c_int)>>,
    chunks: RefCell<Vec<Chunk>>,
}

thread_local! {
    /// The debuggers of this thread's VMs, by main state. Luau hooks only
    /// receive a state, and mlua keeps its own data where a hook could
    /// otherwise store a pointer.
    static ATTACHED: RefCell<HashMap<usize, Rc<Debugger>>> = RefCell::default();
}

fn attached(main: *mut lua_State) -> Option<Rc<Debugger>> {
    ATTACHED.with(|map| map.borrow().get(&(main as usize)).cloned())
}

/// Keeps a VM attached to its debugger, and detaches it when dropped.
pub(crate) struct Attachment {
    debugger: Rc<Debugger>,
}

impl Attachment {
    /// Makes `lua` stop at `breakpoints` and wherever `port`'s debugger
    /// steps. Scripts loaded afterwards are compiled with the debug
    /// information and without the optimizations that would hide lines.
    pub(crate) fn new(
        lua: &Lua,
        port: DebugPort,
        output: Rc<LogSink>,
        breakpoints: Vec<Breakpoint>,
    ) -> mlua::Result<Self> {
        lua.set_compiler(Compiler::new().set_optimization_level(0).set_debug_level(2));
        let main = lua.exec_raw_lua(|raw| {
            // SAFETY: `raw.state()` is the live state mlua is running.
            unsafe { ffi::lua_mainthread(raw.state()) }
        });
        lua.set_named_registry_value(CHUNKS.to_str().unwrap_or_default(), lua.create_table()?)?;
        let debugger = Rc::new(Debugger {
            port,
            output,
            main,
            mode: Cell::new(Mode::Run),
            breakpoints: RefCell::new(breakpoints),
            landed: RefCell::default(),
            chunks: RefCell::default(),
        });
        // SAFETY: `main` is the live main state, and the callbacks table is
        // shared by every thread of the VM. mlua leaves `debugbreak` alone.
        unsafe { (*ffi::lua_callbacks(main)).debugbreak = Some(on_break) };
        ATTACHED.with(|map| map.borrow_mut().insert(main as usize, Rc::clone(&debugger)));
        Ok(Self { debugger })
    }

    /// Whether the debugger asked to end the run.
    pub(crate) fn stop_requested(&self) -> bool {
        self.debugger.port.stop_requested()
    }

    /// A check for the interrupt, which must keep failing once the debugger
    /// asked to stop, so a script that catches the error cannot go on.
    pub(crate) fn stopper(&self) -> impl Fn() -> bool + use<> {
        let debugger = Rc::clone(&self.debugger);
        move || debugger.port.stop_requested()
    }

    /// Applies the breakpoints the debugger sent while the VM was running.
    pub(crate) fn sync(&self, lua: &Lua) {
        let Some(breakpoints) = self.debugger.port.take_breakpoints() else {
            return;
        };
        *self.debugger.breakpoints.borrow_mut() = breakpoints;
        lua.exec_raw_lua(|raw| {
            // SAFETY: `raw.state()` is the live state mlua is running.
            unsafe { self.debugger.apply_breakpoints(raw.state()) }
        });
    }
}

impl Drop for Attachment {
    fn drop(&mut self) {
        ATTACHED.with(|map| map.borrow_mut().remove(&(self.debugger.main as usize)));
    }
}

/// Tells the debugger of `lua`, if it has one, about a chunk that is about
/// to run, so breakpoints can be placed in it. Call it between compiling the
/// chunk and running it.
pub(crate) fn chunk_loaded(lua: &Lua, path: &str, source: &[u8], function: &Function) {
    if ATTACHED.with(|map| map.borrow().is_empty()) {
        return;
    }
    lua.exec_raw_lua(|raw| {
        // SAFETY: `raw.state()` is the live state mlua is running.
        let main = unsafe { ffi::lua_mainthread(raw.state()) };
        let Some(debugger) = attached(main) else {
            return;
        };
        let registered = lua
            .named_registry_value::<mlua::Table>(CHUNKS.to_str().unwrap_or_default())
            .and_then(|chunks| chunks.raw_set(path, function.clone()));
        if registered.is_err() {
            return;
        }
        let lines = source.iter().filter(|&&byte| byte == b'\n').count() as u32 + 1;
        debugger.chunks.borrow_mut().push(Chunk {
            path: path.to_owned(),
            lines,
        });
        // SAFETY: as above.
        unsafe { debugger.apply_breakpoints(raw.state()) };
    });
}

impl Debugger {
    /// Puts a breakpoint on `line` of the chunk at `path`, or removes it.
    /// Returns the line that holds it, which is the next one with code when
    /// `line` has none.
    unsafe fn patch(&self, l: *mut lua_State, path: &str, line: u32, on: bool) -> Option<c_int> {
        let Ok(key) = CString::new(path) else {
            return None;
        };
        // SAFETY: the registry table and the function are pushed and popped
        // here, and `lua_breakpoint` takes the function from the stack.
        unsafe {
            ffi::lua_getfield(l, ffi::LUA_REGISTRYINDEX, CHUNKS.as_ptr());
            ffi::lua_getfield(l, -1, key.as_ptr());
            let landed = (ffi::lua_type(l, -1) == ffi::LUA_TFUNCTION)
                .then(|| ffi::lua_breakpoint(l, -1, line as c_int, c_int::from(on)))
                .filter(|&landed| landed >= 0);
            ffi::lua_settop(l, -3);
            landed
        }
    }

    /// Places the user's breakpoints in every loaded chunk and records where
    /// they landed.
    unsafe fn apply_breakpoints(&self, l: *mut lua_State) {
        let mut landed = self.landed.borrow_mut();
        landed.clear();
        for Breakpoint { path, line } in self.breakpoints.borrow().iter() {
            // SAFETY: forwarded from the caller.
            if let Some(at) = unsafe { self.patch(l, path, *line, true) } {
                landed.push((path.clone(), at));
            }
        }
    }

    /// Breakpoints on every line, for a step to notice where it ends.
    unsafe fn arm_all(&self, l: *mut lua_State) {
        for Chunk { path, lines } in self.chunks.borrow().iter() {
            for line in 1..=*lines {
                // SAFETY: forwarded from the caller.
                unsafe { self.patch(l, path, line, true) };
            }
        }
    }

    /// Takes the stepping breakpoints out and puts the user's back.
    unsafe fn disarm(&self, l: *mut lua_State) {
        for Chunk { path, lines } in self.chunks.borrow().iter() {
            for line in 1..=*lines {
                // SAFETY: forwarded from the caller.
                unsafe { self.patch(l, path, line, false) };
            }
        }
        // SAFETY: forwarded from the caller.
        unsafe { self.apply_breakpoints(l) };
    }

    fn is_script(&self, path: &str) -> bool {
        self.chunks.borrow().iter().any(|chunk| chunk.path == path)
    }

    /// Stops the VM, waits for the debugger and applies its answer. Returns
    /// whether the run must end with an error.
    unsafe fn stop(&self, l: *mut lua_State, reason: StopReason) -> bool {
        // SAFETY: `l` is stopped inside a hook, as the helpers require.
        let (stop, depth, line) = unsafe {
            let frames = inspect::frames(l, &|path| self.is_script(path));
            let line = frames
                .first()
                .and_then(|frame| frame.line)
                .map_or(0, |line| line as c_int);
            let stop = Stop {
                reason,
                variables: inspect::variables(l, 0),
                frames,
            };
            (stop, ffi::lua_stackdepth(l), line)
        };
        let resume = self.port.pause(stop, &mut Inspector { debugger: self, l });
        self.mode.set(match resume {
            Resume::Continue | Resume::Stop => Mode::Run,
            Resume::Step(Step::Into) => Mode::Into { depth, line },
            Resume::Step(Step::Over) => Mode::Over { depth, line },
            Resume::Step(Step::Out) => Mode::Out { depth },
        });
        // SAFETY: as above.
        unsafe {
            self.disarm(l);
            if self.mode.get() != Mode::Run {
                self.arm_all(l);
            }
        }
        resume == Resume::Stop
    }
}

/// Answers the debugger's questions about the state the VM stopped in.
struct Inspector<'a> {
    debugger: &'a Debugger,
    l: *mut lua_State,
}

impl Inspect for Inspector<'_> {
    fn variables(&mut self, frame: usize) -> DebugReply {
        // SAFETY: `l` is the state stopped inside the hook that made `self`.
        let depth = unsafe { ffi::lua_stackdepth(self.l) };
        match c_int::try_from(frame) {
            Ok(level) if level < depth => {
                DebugReply::Variables(unsafe { inspect::variables(self.l, level) })
            }
            _ => DebugReply::Unavailable,
        }
    }

    fn expand(&mut self, frame: usize, path: &[usize]) -> DebugReply {
        // SAFETY: as in `variables`.
        unsafe {
            c_int::try_from(frame)
                .ok()
                .and_then(|level| inspect::expand(self.l, level, path))
                .map_or(DebugReply::Unavailable, DebugReply::Variables)
        }
    }

    fn output(&mut self) -> (Vec<LogEntry>, u32) {
        (
            self.debugger.output.take(),
            self.debugger.output.take_dropped(),
        )
    }

    fn breakpoints(&mut self, breakpoints: &[Breakpoint]) {
        *self.debugger.breakpoints.borrow_mut() = breakpoints.to_vec();
        // SAFETY: as in `variables`. Stepping breakpoints stay in place.
        unsafe {
            if self.debugger.mode.get() == Mode::Run {
                self.debugger.apply_breakpoints(self.l);
            }
        }
    }
}

/// The hook behind every `LOP_BREAK`: a user breakpoint, or a line a step
/// armed.
///
/// A Lua error is a C++ exception, and one that crosses `catch_unwind`
/// aborts the process. The guarded part therefore only decides, and the
/// error is raised after it.
unsafe extern "C-unwind" fn on_break(l: *mut lua_State, _: *mut lua_Debug) {
    // SAFETY: Luau calls the hook with a live state.
    let Some(debugger) = attached(unsafe { ffi::lua_mainthread(l) }) else {
        return;
    };
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: as above.
        unsafe { debugger.break_here(l) }
    }));
    let message = match outcome {
        Ok(false) => return,
        Ok(true) => c"stopped by the debugger",
        Err(_) => {
            debugger.port.request_stop();
            c"the debugger failed"
        }
    };
    // SAFETY: raising from a hook is how Luau expects a hook to end a call.
    unsafe {
        ffi::lua_pushstring(l, message.as_ptr());
        ffi::lua_error(l)
    }
}

impl Debugger {
    /// Decides whether the line that just hit a breakpoint is a stop.
    /// Returns whether the run must end.
    unsafe fn break_here(&self, l: *mut lua_State) -> bool {
        // SAFETY: `l` is stopped inside the hook.
        let (depth, line, path) = unsafe {
            let mut ar: lua_Debug = std::mem::zeroed();
            if ffi::lua_getinfo(l, 0, c"sl".as_ptr(), &mut ar) == 0 {
                return false;
            }
            let source = CStr::from_ptr(ar.source).to_string_lossy();
            (
                ffi::lua_stackdepth(l),
                ar.currentline,
                source.trim_start_matches('=').to_owned(),
            )
        };
        let user = self
            .landed
            .borrow()
            .iter()
            .any(|(at, landed)| *at == path && *landed == line);
        let stepped = match self.mode.get() {
            Mode::Run => false,
            Mode::Into {
                depth: from,
                line: at,
            } => depth != from || line != at,
            Mode::Over {
                depth: from,
                line: at,
            } => depth < from || (depth == from && line != at),
            Mode::Out { depth: from } => depth < from,
        };
        if user {
            // SAFETY: as above.
            unsafe { self.stop(l, StopReason::Breakpoint) }
        } else if stepped {
            // SAFETY: as above.
            unsafe { self.stop(l, StopReason::Step) }
        } else {
            false
        }
    }
}
