//! Breakpoints, stepping and inspection of a VM running on its own thread.

use std::cell::Cell;
use std::sync::mpsc::{self, Receiver};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use glaux_script::debug::{
    Breakpoint, DebugEvent, DebugLink, DebugReply, DebugRequest, Resume, Step, Stop, StopReason,
    Variable, session,
};

use super::*;

const LIMIT: Duration = Duration::from_secs(10);

/// A debugged VM that starts its script and ticks it once, off this thread.
struct Debugged {
    link: DebugLink,
    finished: Receiver<Result<(), RuntimeError>>,
    thread: Option<JoinHandle<()>>,
    /// Whether the VM waits for a resume.
    stopped: Cell<bool>,
}

fn at(path: &str, line: u32) -> Breakpoint {
    Breakpoint {
        path: path.into(),
        line,
    }
}

fn main_at(lines: &[u32]) -> Vec<Breakpoint> {
    lines.iter().map(|&line| at("main.luau", line)).collect()
}

fn debugged(source: &'static str, breakpoints: Vec<Breakpoint>) -> Debugged {
    debugged_with(source, &[], breakpoints, SMALL)
}

fn debugged_with(
    source: &'static str,
    modules: &'static [(&'static str, &'static str)],
    breakpoints: Vec<Breakpoint>,
    budget: Budget,
) -> Debugged {
    let (link, port) = session();
    let (done, finished) = mpsc::channel();
    let thread = thread::spawn(move || {
        let result = match Runtime::start_debug(
            "main.luau",
            source.as_bytes(),
            Files::new(modules),
            budget,
            port,
            breakpoints,
        ) {
            Ok(mut runtime) => runtime.tick(NO_INPUT),
            Err(failure) => Err(failure.error),
        };
        let _ = done.send(result);
    });
    Debugged {
        link,
        finished,
        thread: Some(thread),
        stopped: Cell::new(false),
    }
}

impl Debugged {
    fn event(&self) -> DebugEvent {
        self.link
            .wait_event(LIMIT)
            .expect("the VM is alive")
            .expect("an event before the timeout")
    }

    fn stop(&self) -> Stop {
        match self.event() {
            DebugEvent::Stopped(stop) => {
                self.stopped.set(true);
                stop
            }
            other => panic!("expected a stop, got {other:?}"),
        }
    }

    fn resume(&self, resume: Resume) {
        self.stopped.set(false);
        assert!(self.link.send(DebugRequest::Resume(resume)));
    }

    fn ask(&self, request: DebugRequest) -> DebugReply {
        assert!(self.link.send(request));
        match self.event() {
            DebugEvent::Reply(reply) => reply,
            other => panic!("expected a reply, got {other:?}"),
        }
    }

    fn variables(&self, frame: usize) -> Vec<Variable> {
        match self.ask(DebugRequest::Variables { frame }) {
            DebugReply::Variables(variables) => variables,
            other => panic!("variables of frame {frame}: {other:?}"),
        }
    }

    fn expand(&self, frame: usize, path: &[usize]) -> Vec<Variable> {
        match self.ask(DebugRequest::Expand {
            frame,
            path: path.to_vec(),
        }) {
            DebugReply::Variables(variables) => variables,
            other => panic!("cannot expand {path:?}: {other:?}"),
        }
    }

    /// Continues, at this stop and every later one, until the tick ends, and
    /// returns how it ended.
    fn finish(&self) -> Result<(), RuntimeError> {
        if self.stopped.get() {
            self.resume(Resume::Continue);
        }
        loop {
            if let Ok(result) = self.finished.try_recv() {
                return result;
            }
            match self.link.wait_event(Duration::from_millis(5)) {
                Ok(Some(DebugEvent::Stopped(_))) => {
                    self.stopped.set(true);
                    self.resume(Resume::Continue);
                }
                Ok(_) => {}
                Err(_) => return self.finished.recv_timeout(LIMIT).expect("the VM ended"),
            }
        }
    }

    /// Resumes with `resume` at every stop and lists the lines it stopped on.
    fn lines_with(&self, first: &Stop, resume: Resume) -> Vec<u32> {
        let mut lines = vec![first.frames[0].line.unwrap()];
        self.resume(resume);
        loop {
            if let Ok(result) = self.finished.try_recv() {
                result.unwrap();
                return lines;
            }
            if let Ok(Some(DebugEvent::Stopped(stop))) =
                self.link.wait_event(Duration::from_millis(5))
            {
                self.stopped.set(true);
                lines.push(stop.frames[0].line.unwrap());
                self.resume(resume);
            }
        }
    }
}

impl Drop for Debugged {
    fn drop(&mut self) {
        self.link.stop();
        if let Some(thread) = self.thread.take() {
            thread.join().expect("the VM thread did not panic");
        }
    }
}

fn names(variables: &[Variable]) -> Vec<(&str, &str)> {
    variables
        .iter()
        .map(|v| (v.name.as_str(), v.value.as_str()))
        .collect()
}

const LINES: &str = "local function add(a, b)
  local sum = a + b
  return sum
end
function tick()
  local x = 10
  local y = add(x, 5)
  local z = y * 2
  log.info(z)
end";

#[test]
fn a_breakpoint_stops_with_the_line_the_function_and_its_locals() {
    let vm = debugged(LINES, main_at(&[7]));
    let stop = vm.stop();
    assert_eq!(stop.reason, StopReason::Breakpoint);
    assert_eq!(stop.frames[0].function, "tick");
    assert_eq!(stop.frames[0].path.as_deref(), Some("main.luau"));
    assert_eq!(stop.frames[0].line, Some(7));
    assert_eq!(names(&stop.variables), [("x", "10"), ("add", "function")]);
    vm.resume(Resume::Continue);
    assert_eq!(vm.finish(), Ok(()));
}

#[test]
fn the_stack_lists_callers_and_shows_the_engine_frame_without_a_path() {
    let vm = debugged(
        "local function add(a, b)\n  return a + b\nend\nfunction tick()\n  pcall(add, 1, 2)\nend",
        main_at(&[2]),
    );
    let stop = vm.stop();
    let shape: Vec<_> = stop
        .frames
        .iter()
        .map(|f| (f.function.as_str(), f.path.is_some(), f.line))
        .collect();
    assert_eq!(
        shape,
        [
            ("add", true, Some(2)),
            ("pcall", false, None),
            ("tick", true, Some(5)),
        ]
    );
    assert!(stop.frames[0].has_variables);
    assert!(!stop.frames[1].has_variables);
    assert_eq!(vm.finish(), Ok(()));
}

#[test]
fn the_variables_of_an_outer_frame_can_be_asked_for() {
    let vm = debugged(LINES, main_at(&[3]));
    vm.stop();
    assert_eq!(
        names(&vm.variables(0)),
        [("a", "10"), ("b", "5"), ("sum", "15")]
    );
    // `y` is declared by the statement that is still calling `add`.
    assert_eq!(names(&vm.variables(1)), [("x", "10"), ("add", "function")]);
    assert_eq!(
        vm.ask(DebugRequest::Variables { frame: 9 }),
        DebugReply::Unavailable
    );
    assert_eq!(vm.finish(), Ok(()));
}

#[test]
fn upvalues_are_listed_after_the_locals() {
    let vm = debugged(
        "local score = 41\nlocal function bump()\n  local step = 1\n  score += step\nend\nfunction tick() bump() end",
        main_at(&[4]),
    );
    let stop = vm.stop();
    assert_eq!(names(&stop.variables), [("step", "1"), ("score", "41")]);
    assert_eq!(vm.finish(), Ok(()));
}

#[test]
fn a_breakpoint_on_a_line_without_code_lands_on_the_next_one() {
    let vm = debugged(
        "function tick()\n\n  -- comment\n  local a = 1\nend",
        main_at(&[2]),
    );
    assert_eq!(vm.stop().frames[0].line, Some(4));
    assert_eq!(vm.finish(), Ok(()));
}

#[test]
fn a_breakpoint_in_the_entry_chunk_and_in_boot_stops_the_start() {
    let vm = debugged(
        "local ready = true\nfunction boot()\n  local n = 1\nend",
        main_at(&[1, 3]),
    );
    assert_eq!(vm.stop().frames[0].line, Some(1));
    vm.resume(Resume::Continue);
    let stop = vm.stop();
    assert_eq!(stop.frames[0].function, "boot");
    assert_eq!(vm.finish(), Ok(()));
}

#[test]
fn a_breakpoint_in_a_required_module_stops_there() {
    const MODULES: &[(&str, &str)] = &[(
        "code/enemy.luau",
        "local M = {}\nfunction M.hit(n)\n  local left = n - 1\n  return left\nend\nreturn M",
    )];
    let vm = debugged_with(
        "local enemy = require(\"enemy\")\nfunction tick()\n  enemy.hit(3)\nend",
        MODULES,
        vec![at("code/enemy.luau", 3)],
        SMALL,
    );
    let stop = vm.stop();
    assert_eq!(stop.frames[0].path.as_deref(), Some("code/enemy.luau"));
    assert_eq!(stop.frames[1].path.as_deref(), Some("main.luau"));
    assert_eq!(names(&stop.variables), [("n", "3")]);
    assert_eq!(vm.finish(), Ok(()));
}

#[test]
fn step_over_walks_the_lines_of_a_function_without_entering_calls() {
    let vm = debugged(LINES, main_at(&[6]));
    let first = vm.stop();
    assert_eq!(
        vm.lines_with(&first, Resume::Step(Step::Over)),
        [6, 7, 8, 9, 10]
    );
}

#[test]
fn step_into_enters_the_callee_and_comes_back() {
    let vm = debugged(LINES, main_at(&[7]));
    let first = vm.stop();
    let lines = vm.lines_with(&first, Resume::Step(Step::Into));
    assert_eq!(&lines[..4], [7, 2, 3, 8]);
}

#[test]
fn step_out_finishes_the_function_and_stops_in_its_caller() {
    let vm = debugged(LINES, main_at(&[2]));
    let inside = vm.stop();
    assert_eq!(inside.frames.len(), 2);
    vm.resume(Resume::Step(Step::Out));
    let back = vm.stop();
    assert_eq!(back.reason, StopReason::Step);
    assert_eq!(back.frames[0].function, "tick");
    assert_eq!(back.frames.len(), 1);
    assert_eq!(vm.finish(), Ok(()));
}

#[test]
fn continuing_after_a_step_stops_only_at_the_users_breakpoints() {
    let vm = debugged(LINES, main_at(&[6]));
    vm.stop();
    vm.resume(Resume::Step(Step::Over));
    assert_eq!(vm.stop().frames[0].line, Some(7));
    vm.resume(Resume::Continue);
    // Another stop would leave the call unfinished until the timeout.
    assert_eq!(vm.finished.recv_timeout(LIMIT), Ok(Ok(())));
}

#[test]
fn tables_expand_one_level_at_a_time() {
    const SOURCE: &str = "function tick()
  local player = {name = \"ana\", pos = {x = 1, y = 2}, empty = {}}
  local here = 1
end";
    let vm = debugged(SOURCE, main_at(&[3]));
    let stop = vm.stop();
    assert_eq!(names(&stop.variables)[0], ("player", "{..}"));
    assert!(stop.variables[0].expandable);

    let entries = vm.expand(0, &[0]);
    let mut keys: Vec<_> = entries.iter().map(|e| e.name.as_str()).collect();
    keys.sort_unstable();
    assert_eq!(keys, ["empty", "name", "pos"]);
    let pos = entries.iter().position(|e| e.name == "pos").unwrap();
    assert!(entries[pos].expandable);
    let empty = entries.iter().position(|e| e.name == "empty").unwrap();
    assert!(
        !entries[empty].expandable,
        "an empty table has nothing to show"
    );
    assert_eq!(entries[empty].value, "{}");

    let mut inner = vm.expand(0, &[0, pos]);
    inner.sort_by(|a, b| a.name.cmp(&b.name));
    assert_eq!(names(&inner), [("x", "1"), ("y", "2")]);
    assert_eq!(
        vm.ask(DebugRequest::Expand {
            frame: 0,
            path: vec![0, 99]
        }),
        DebugReply::Unavailable
    );
    assert_eq!(
        vm.ask(DebugRequest::Expand {
            frame: 0,
            path: vec![1]
        }),
        DebugReply::Unavailable,
        "a number has no entries"
    );
    assert_eq!(vm.finish(), Ok(()));
}

#[test]
fn a_long_table_lists_a_bounded_number_of_entries() {
    let vm = debugged(
        "function tick()\n  local big = {}\n  for i = 1, 500 do big[i] = i end\n  local here = 1\nend",
        main_at(&[4]),
    );
    vm.stop();
    let entries = vm.expand(0, &[0]);
    assert_eq!(entries.len(), 64);
    assert!(entries.iter().all(|e| e.name.starts_with('[')));
    assert_eq!(vm.finish(), Ok(()));
}

#[test]
fn values_are_shown_without_running_any_script_code() {
    let vm = debugged(
        "function tick()
  local guarded = setmetatable({a = 1}, {__tostring = function() error('tostring ran') end, __index = function() error('index ran') end, __pairs = function() error('pairs ran') end, __len = function() error('len ran') end})
  local text = 'hello'
  local nothing = nil
  local ratio = 1.5
  local flag = true
  local fn = function() end
  local here = 1
end",
        main_at(&[8]),
    );
    let stop = vm.stop();
    assert_eq!(
        names(&stop.variables),
        [
            ("guarded", "{..}"),
            ("text", "\"hello\""),
            ("nothing", "nil"),
            ("ratio", "1.5"),
            ("flag", "true"),
            ("fn", "function"),
        ]
    );
    let entries = vm.expand(0, &[0]);
    assert_eq!(names(&entries), [("a", "1")]);
    assert_eq!(vm.finish(), Ok(()));
}

#[test]
fn breakpoints_sent_before_the_tick_and_while_stopped_take_effect() {
    let vm = debugged(LINES, main_at(&[6]));
    vm.stop();
    assert!(vm.link.send(DebugRequest::Breakpoints(main_at(&[8]))));
    vm.resume(Resume::Continue);
    assert_eq!(vm.stop().frames[0].line, Some(8));
    vm.resume(Resume::Continue);
    assert_eq!(vm.finish(), Ok(()));
}

#[test]
fn time_spent_stopped_is_not_charged_to_the_budget() {
    let vm = debugged_with(
        "function tick()\n  local total = 0\n  for i = 1, 200 do\n    total += i\n  end\nend",
        &[],
        main_at(&[4]),
        Budget { interrupts: 500 },
    );
    for _ in 0..200 {
        vm.stop();
        vm.resume(Resume::Continue);
    }
    assert_eq!(vm.finish(), Ok(()));
}

#[test]
fn stopping_a_stopped_vm_ends_the_call_even_if_the_script_catches_errors() {
    let vm = debugged(
        "function tick()\n  local a = 1\n  pcall(function() while true do end end)\n  while true do end\nend",
        main_at(&[2]),
    );
    vm.stop();
    vm.link.stop();
    assert_eq!(
        vm.finished.recv_timeout(LIMIT).unwrap(),
        Err(RuntimeError {
            phase: Phase::Tick,
            error: ScriptError::Stopped
        })
    );
}

#[test]
fn stopping_a_running_vm_ends_the_call() {
    let vm = debugged("function tick() while true do end end", Vec::new());
    thread::sleep(Duration::from_millis(20));
    vm.link.stop();
    let result = vm.finished.recv_timeout(LIMIT).unwrap();
    assert!(
        matches!(
            result,
            Err(RuntimeError {
                error: ScriptError::Stopped | ScriptError::LimitExceeded,
                ..
            })
        ),
        "{result:?}"
    );
}

#[test]
fn losing_the_debugger_while_stopped_ends_the_call_instead_of_hanging() {
    let mut vm = debugged(LINES, main_at(&[7]));
    vm.stop();
    let (link, _) = session();
    drop(std::mem::replace(&mut vm.link, link));
    assert_eq!(
        vm.finished.recv_timeout(LIMIT).unwrap(),
        Err(RuntimeError {
            phase: Phase::Tick,
            error: ScriptError::Stopped
        })
    );
}

#[test]
fn a_script_error_in_a_debugged_vm_is_reported_as_usual() {
    let vm = debugged("function tick()\n  error(\"boom\")\nend", Vec::new());
    let result = vm.finish();
    assert_eq!(
        result,
        Err(error(
            Phase::Tick,
            ScriptError::Runtime("main.luau:2: boom".into())
        ))
    );
}

#[test]
fn a_coroutine_stops_with_its_own_stack() {
    let vm = debugged(
        "function tick()\n  local co = coroutine.wrap(function(z)\n    local w = z * 3\n    return w\n  end)\n  co(5)\nend",
        main_at(&[4]),
    );
    let stop = vm.stop();
    assert_eq!(names(&stop.variables), [("z", "5"), ("w", "15")]);
    assert_eq!(vm.finish(), Ok(()));
}

#[test]
fn what_the_script_logged_so_far_can_be_read_while_it_is_stopped() {
    let vm = debugged(
        "function tick()\n  log.info(\"before\")\n  local here = 1\n  log.info(\"after\")\nend",
        main_at(&[3]),
    );
    vm.stop();
    let DebugReply::Output { entries, dropped } = vm.ask(DebugRequest::Output) else {
        panic!("expected the output");
    };
    assert_eq!(
        entries.iter().map(|e| e.text.as_str()).collect::<Vec<_>>(),
        ["before"]
    );
    assert_eq!(dropped, 0);
    // Taking it empties the sink: only the rest is left at the end.
    let DebugReply::Output { entries, .. } = vm.ask(DebugRequest::Output) else {
        panic!("expected the output");
    };
    assert!(entries.is_empty());
    assert_eq!(vm.finish(), Ok(()));
}
