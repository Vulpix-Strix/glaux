//! Breakpoints, stepping and inspection for a Rhai engine that is being
//! debugged.
//!
//! A debugged engine runs on its own thread and blocks inside Rhai's
//! debugger callback while it is stopped, as [`glaux_script::debug`]
//! describes. Rhai reports every node of the syntax tree, so this module
//! decides which of those events are stops a person would recognize.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use glaux_script::debug::{
    Breakpoint, DebugPort, DebugReply, Frame, Inspect, Resume, Step, Stop, StopReason, Variable,
};
use glaux_script::{LogEntry, LogSink};
use rhai::debugger::{BreakPoint, DebuggerCommand, DebuggerEvent};
use rhai::{
    ASTNode, Array, Dynamic, Engine, EvalAltResult, EvalContext, Map, OptimizationLevel, Position,
    Stmt,
};

/// Functions of this prefix are Glaux's own trampolines. They are compiled
/// apart from the script but report its file name, so their lines collide
/// with the script's.
pub(crate) const TRAMPOLINE_PREFIX: &str = "__glaux_";

/// Entries listed for one map or array.
const MAX_ENTRIES: usize = 64;

/// Longest string shown, in bytes.
const MAX_TEXT: usize = 40;

/// Where one visit of a line is told apart from the next: source, line and
/// call level.
type Key = (String, u32, usize);

#[derive(Debug, Clone, PartialEq, Eq)]
enum Mode {
    Run,
    Over(Key),
    Into(Key),
    /// Waiting for the current function to return.
    Out,
}

/// The debugger state of one engine.
pub(crate) struct Debugger {
    port: DebugPort,
    output: Rc<LogSink>,
    breakpoints: RefCell<Vec<Breakpoint>>,
    mode: RefCell<Mode>,
    /// The columns already reported in the current visit of each line.
    ///
    /// A breakpoint on a line fires for the statement and again for each
    /// expression in it, so the first event of a visit is the stop and the
    /// rest are the same visit. A column seen twice starts the next visit.
    visits: RefCell<HashMap<Key, HashSet<usize>>>,
}

impl Debugger {
    pub(crate) fn new(port: DebugPort, output: Rc<LogSink>, breakpoints: Vec<Breakpoint>) -> Self {
        Self {
            port,
            output,
            breakpoints: RefCell::new(breakpoints),
            mode: RefCell::new(Mode::Run),
            visits: RefCell::default(),
        }
    }

    /// Whether the debugger asked to end the run.
    pub(crate) fn stop_requested(&self) -> bool {
        self.port.stop_requested()
    }

    /// Takes the breakpoints the debugger sent while the engine was running.
    /// They apply from the next call into the script.
    pub(crate) fn sync(&self) {
        if let Some(breakpoints) = self.port.take_breakpoints() {
            *self.breakpoints.borrow_mut() = breakpoints;
        }
    }

    /// Registers the debugger on `engine`. Scripts compiled afterwards keep
    /// every statement the optimizer would remove, so each line can stop.
    pub(crate) fn attach(self: &Rc<Self>, engine: &mut Engine) {
        engine.set_optimization_level(OptimizationLevel::None);
        let init = Rc::clone(self);
        let callback = Rc::clone(self);
        #[allow(deprecated, reason = "volatile but supported API of Rhai's debugger")]
        engine.register_debugger(
            move |_, mut debugger| {
                debugger
                    .break_points_mut()
                    .extend(init.breakpoints.borrow().iter().filter_map(position));
                debugger
            },
            move |ctx, event, node, source, pos| callback.on_event(ctx, event, node, source, pos),
        );
    }

    fn on_event(
        &self,
        mut ctx: EvalContext,
        event: DebuggerEvent,
        node: ASTNode,
        source: Option<&str>,
        pos: Position,
    ) -> Result<DebuggerCommand, Box<EvalAltResult>> {
        let mode = self.mode.borrow().clone();
        let reason = match event {
            DebuggerEvent::Start => {
                self.visits.borrow_mut().clear();
                // A step that ran off the end of one callback goes on into
                // the next.
                return Ok(if mode == Mode::Run {
                    DebuggerCommand::Continue
                } else {
                    DebuggerCommand::StepInto
                });
            }
            DebuggerEvent::End => return Ok(DebuggerCommand::Continue),
            DebuggerEvent::BreakPoint(_) => StopReason::Breakpoint,
            _ => StopReason::Step,
        };

        let in_trampoline = ctx
            .debugger()
            .and_then(|debugger| debugger.call_stack().last())
            .is_some_and(|call| call.fn_name.starts_with(TRAMPOLINE_PREFIX));
        // A function body opens with a no-op statement on its signature
        // line. A step should land on its first real statement.
        let opening =
            matches!(node, ASTNode::Stmt(Stmt::Noop(..))) && matches!(event, DebuggerEvent::Step);
        if in_trampoline || opening {
            return Ok(resume_command(&mode, in_trampoline));
        }

        let level = ctx.call_level();
        let line = pos.line().unwrap_or(0) as u32;
        let column = pos.position().unwrap_or(0);
        let key: Key = (source.unwrap_or_default().to_owned(), line, level);

        let stops = match (&mode, &event) {
            (
                Mode::Out,
                DebuggerEvent::FunctionExitWithValue(_) | DebuggerEvent::FunctionExitWithError(_),
            ) => {
                // Out of the function: stop at whatever the caller does next.
                *self.mode.borrow_mut() = Mode::Into(key);
                return Ok(DebuggerCommand::StepInto);
            }
            (_, DebuggerEvent::BreakPoint(_)) => self.new_visit(&key, column),
            (Mode::Into(from), DebuggerEvent::Step) => *from != key,
            (Mode::Over(from), DebuggerEvent::Step) => {
                key.2 < from.2 || (key.2 == from.2 && (&key.0, key.1) != (&from.0, from.1))
            }
            _ => false,
        };
        if !stops {
            return Ok(resume_command(&mode, false));
        }
        self.visits
            .borrow_mut()
            .entry(key.clone())
            .or_default()
            .insert(column);

        let stop = Stop {
            reason,
            frames: frames(&ctx, source, pos),
            variables: variables(&ctx),
        };
        let resume = self.port.pause(
            stop,
            &mut Inspector {
                debugger: self,
                ctx: &mut ctx,
            },
        );
        let next = match resume {
            Resume::Continue | Resume::Stop => Mode::Run,
            Resume::Step(Step::Over) => Mode::Over(key),
            Resume::Step(Step::Into) => Mode::Into(key),
            Resume::Step(Step::Out) => Mode::Out,
        };
        let command = resume_command(&next, false);
        *self.mode.borrow_mut() = next;
        if resume == Resume::Stop {
            return Err(EvalAltResult::ErrorTerminated(Dynamic::UNIT, pos).into());
        }
        Ok(command)
    }

    /// Whether an event of a breakpoint at `column` of `key`'s line begins
    /// a new visit of it.
    fn new_visit(&self, key: &Key, column: usize) -> bool {
        let mut visits = self.visits.borrow_mut();
        let seen = visits.entry(key.clone()).or_default();
        if seen.contains(&column) {
            seen.clear();
            seen.insert(column);
            return true;
        }
        seen.insert(column);
        seen.len() == 1
    }
}

/// The command that keeps `mode` going. Inside a trampoline an engine that
/// is stepping must step on, to reach the script's own code.
fn resume_command(mode: &Mode, in_trampoline: bool) -> DebuggerCommand {
    match mode {
        Mode::Run => DebuggerCommand::Continue,
        _ if in_trampoline => DebuggerCommand::StepInto,
        Mode::Over(_) => DebuggerCommand::StepOver,
        Mode::Into(_) => DebuggerCommand::StepInto,
        Mode::Out => DebuggerCommand::FunctionExit,
    }
}

/// A line breakpoint, which Rhai matches by column 0. Lines past what its
/// positions can hold are left out.
fn position(breakpoint: &Breakpoint) -> Option<BreakPoint> {
    let line = u16::try_from(breakpoint.line).ok()?;
    Some(BreakPoint::AtPosition {
        source: Some(breakpoint.path.as_str().into()),
        pos: Position::new(line, 0),
        enabled: true,
    })
}

/// The stack, innermost first.
///
/// Rhai records, for each call, the function's own source and the position
/// it was called from. A frame takes its name and source from its own entry
/// and its line from the next inner call. The top-level code is not an entry
/// and is named `main`, shown only when it is running a call, since a
/// callback is called by the host and leaves it no line.
fn frames(ctx: &EvalContext, source: Option<&str>, pos: Position) -> Vec<Frame> {
    let calls = ctx
        .debugger()
        .map(|debugger| debugger.call_stack().to_vec())
        .unwrap_or_default();
    let mut out = Vec::new();
    let mut line = pos.line();
    let mut path = source.unwrap_or_default().to_owned();
    for call in calls.iter().rev() {
        if let Some(own) = &call.source {
            path = own.to_string();
        }
        out.push((call.fn_name.to_string(), path.clone(), line));
        line = call.pos.line();
    }
    if line.is_some() {
        out.push(("main".to_owned(), path, line));
    }
    out.retain(|(function, ..)| !function.starts_with(TRAMPOLINE_PREFIX));
    out.into_iter()
        .enumerate()
        .map(|(index, (function, path, line))| Frame {
            function,
            has_variables: index == 0,
            line: line.map(|line| line as u32),
            path: (!path.is_empty()).then_some(path),
        })
        .collect()
}

/// The variables of the innermost function: `this` when the callback has
/// one, then the scope, with a shadowed name shown once.
fn named<'a>(ctx: &'a EvalContext) -> Vec<(String, &'a Dynamic)> {
    // The scope lists the newest variable first.
    let mut seen = HashSet::new();
    let mut scope: Vec<_> = ctx
        .scope()
        .iter_raw()
        .filter(|(name, ..)| seen.insert(*name))
        .map(|(name, _, value)| (name.to_owned(), value))
        .collect();
    scope.reverse();
    ctx.this_ptr()
        .map(|this| ("this".to_owned(), this))
        .into_iter()
        .chain(scope)
        .collect()
}

fn variables(ctx: &EvalContext) -> Vec<Variable> {
    named(ctx)
        .into_iter()
        .map(|(name, value)| variable(name, value))
        .collect()
}

fn variable(name: String, value: &Dynamic) -> Variable {
    Variable {
        name,
        value: render(value),
        expandable: entry_count(value) > 0,
    }
}

/// How many entries `value` lists, when it is a map or an array.
fn entry_count(value: &Dynamic) -> usize {
    if value.is_map() {
        value.read_lock::<Map>().map_or(0, |map| map.len())
    } else if value.is_array() {
        value.read_lock::<Array>().map_or(0, |array| array.len())
    } else {
        0
    }
}

/// A short rendering of `value` that runs no script code.
fn render(value: &Dynamic) -> String {
    if value.is_unit() {
        "()".to_owned()
    } else if value.is_bool() || value.is_int() || value.is_float() {
        value.to_string()
    } else if value.is_char() {
        format!("'{value}'")
    } else if value.is_string() {
        let text = value.clone().into_string().unwrap_or_default();
        format!("\"{}\"", clip(&text))
    } else if value.is_map() {
        if entry_count(value) > 0 {
            "{..}"
        } else {
            "#{}"
        }
        .to_owned()
    } else if value.is_array() {
        if entry_count(value) > 0 { "[..]" } else { "[]" }.to_owned()
    } else {
        value.type_name().to_owned()
    }
}

/// `text` cut to [`MAX_TEXT`] bytes at a character boundary.
fn clip(text: &str) -> String {
    if text.len() <= MAX_TEXT {
        return text.to_owned();
    }
    let end = (0..=MAX_TEXT)
        .rev()
        .find(|&end| text.is_char_boundary(end))
        .unwrap_or(0);
    format!("{}...", &text[..end])
}

/// The entries at the end of `path` below `value`: each index picks the
/// entry of the map or array before it, in key order or position.
fn expand(value: &Dynamic, path: &[usize]) -> Option<Vec<Variable>> {
    let Some((&first, rest)) = path.split_first() else {
        return Some(entries(value));
    };
    if let Some(map) = value.read_lock::<Map>() {
        let (_, child) = map.iter().nth(first)?;
        return expand(child, rest);
    }
    let array = value.read_lock::<Array>()?;
    expand(array.get(first)?, rest)
}

fn entries(value: &Dynamic) -> Vec<Variable> {
    if let Some(map) = value.read_lock::<Map>() {
        return map
            .iter()
            .take(MAX_ENTRIES)
            .map(|(key, child)| variable(key.to_string(), child))
            .collect();
    }
    value.read_lock::<Array>().map_or_else(Vec::new, |array| {
        array
            .iter()
            .enumerate()
            .take(MAX_ENTRIES)
            .map(|(index, child)| variable(format!("[{index}]"), child))
            .collect()
    })
}

/// Answers the debugger's questions about the state the engine stopped in.
struct Inspector<'a, 'e, 's, 'ps, 'g, 'c, 't> {
    debugger: &'a Debugger,
    ctx: &'a mut EvalContext<'e, 's, 'ps, 'g, 'c, 't>,
}

impl Inspect for Inspector<'_, '_, '_, '_, '_, '_, '_> {
    fn variables(&mut self, frame: usize) -> DebugReply {
        if frame == 0 {
            DebugReply::Variables(variables(self.ctx))
        } else {
            DebugReply::Unavailable
        }
    }

    fn expand(&mut self, frame: usize, path: &[usize]) -> DebugReply {
        let Some((&first, rest)) = path.split_first().filter(|_| frame == 0) else {
            return DebugReply::Unavailable;
        };
        named(self.ctx)
            .get(first)
            .and_then(|(_, value)| expand(value, rest))
            .map_or(DebugReply::Unavailable, DebugReply::Variables)
    }

    fn output(&mut self) -> (Vec<LogEntry>, u32) {
        (
            self.debugger.output.take(),
            self.debugger.output.take_dropped(),
        )
    }

    fn breakpoints(&mut self, breakpoints: &[Breakpoint]) {
        *self.debugger.breakpoints.borrow_mut() = breakpoints.to_vec();
        // The running call keeps the breakpoints it started with unless they
        // are replaced here.
        if let Some(live) = self.ctx.debugger_mut() {
            let points = live.break_points_mut();
            points.retain(|point| !matches!(point, BreakPoint::AtPosition { .. }));
            points.extend(breakpoints.iter().filter_map(position));
        }
    }
}
