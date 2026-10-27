//! Breakpoints, stepping and inspection of an engine running on its own
//! thread.

use std::cell::Cell;
use std::sync::mpsc::{self, Receiver};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use glaux_script::debug::{
    Breakpoint, DebugEvent, DebugLink, DebugReply, DebugRequest, Resume, Step, Stop, StopReason,
    Variable, session,
};
use pretty_assertions::assert_eq;

use super::*;

const LIMIT: Duration = Duration::from_secs(10);

/// A debugged engine that starts its script and ticks it once, off this
/// thread.
struct Debugged {
    link: DebugLink,
    finished: Receiver<Result<(), RuntimeError>>,
    thread: Option<JoinHandle<()>>,
    /// Whether the engine waits for a resume.
    stopped: Cell<bool>,
}

fn at(path: &str, line: u32) -> Breakpoint {
    Breakpoint {
        path: path.into(),
        line,
    }
}

fn main_at(lines: &[u32]) -> Vec<Breakpoint> {
    lines.iter().map(|&line| at("main.rhai", line)).collect()
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
            "main.rhai",
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
            .expect("the engine is alive")
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

    fn expand(&self, path: &[usize]) -> Vec<Variable> {
        match self.ask(DebugRequest::Expand {
            frame: 0,
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
                Err(_) => return self.finished.recv_timeout(LIMIT).expect("the engine ended"),
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
            thread.join().expect("the engine thread did not panic");
        }
    }
}

fn names(variables: &[Variable]) -> Vec<(&str, &str)> {
    variables
        .iter()
        .map(|v| (v.name.as_str(), v.value.as_str()))
        .collect()
}

const LINES: &str = "fn add(a, b) {
  let sum = a + b;
  sum
}
fn tick() {
  let x = 10;
  let y = add(x, 5);
  let z = y * 2;
  log::info(z);
}";

#[test]
fn a_breakpoint_stops_with_the_line_the_function_and_its_locals() {
    let vm = debugged(LINES, main_at(&[7]));
    let stop = vm.stop();
    assert_eq!(stop.reason, StopReason::Breakpoint);
    assert_eq!(stop.frames[0].function, "tick");
    assert_eq!(stop.frames[0].path.as_deref(), Some("main.rhai"));
    assert_eq!(stop.frames[0].line, Some(7));
    assert_eq!(names(&stop.variables), [("this", "#{}"), ("x", "10")]);
    assert_eq!(vm.finish(), Ok(()));
}

#[test]
fn one_visit_of_a_line_is_one_stop_however_many_expressions_it_holds() {
    let vm = debugged(
        "fn g(n) { n + 1 }\nfn tick() {\n  let a = g(1) + g(2);\n  let b = a;\n}",
        main_at(&[3]),
    );
    vm.stop();
    vm.resume(Resume::Continue);
    // Another stop would leave the call unfinished until the timeout.
    assert_eq!(vm.finished.recv_timeout(LIMIT), Ok(Ok(())));
}

#[test]
fn a_line_stops_again_on_each_new_visit() {
    let vm = debugged(
        "fn tick() {\n  for i in 0..3 {\n    let n = i;\n  }\n}",
        main_at(&[3]),
    );
    for expected in ["0", "1", "2"] {
        let stop = vm.stop();
        assert_eq!(stop.frames[0].line, Some(3));
        assert_eq!(names(&stop.variables)[1], ("i", expected));
        vm.resume(Resume::Continue);
    }
    assert_eq!(vm.finished.recv_timeout(LIMIT), Ok(Ok(())));
}

#[test]
fn the_stack_lists_callers_innermost_first_and_only_the_innermost_has_variables() {
    let vm = debugged(LINES, main_at(&[3]));
    let stop = vm.stop();
    let shape: Vec<_> = stop
        .frames
        .iter()
        .map(|f| (f.function.as_str(), f.line, f.has_variables))
        .collect();
    assert_eq!(shape, [("add", Some(3), true), ("tick", Some(7), false)]);
    assert_eq!(
        vm.ask(DebugRequest::Variables { frame: 1 }),
        DebugReply::Unavailable
    );
    assert_eq!(
        vm.ask(DebugRequest::Variables { frame: 0 }),
        DebugReply::Variables(stop.variables.clone())
    );
    assert_eq!(vm.finish(), Ok(()));
}

#[test]
fn glaux_trampolines_never_stop_on_the_scripts_lines() {
    // Line 2 of the script is also a line of Glaux's `__glaux_tick`.
    let vm = debugged(LINES, main_at(&[2]));
    let stop = vm.stop();
    assert_eq!(stop.frames[0].function, "add");
    assert!(
        stop.frames
            .iter()
            .all(|f| !f.function.starts_with("__glaux_"))
    );
    assert_eq!(names(&stop.variables), [("a", "10"), ("b", "5")]);
    vm.resume(Resume::Continue);
    assert_eq!(vm.finished.recv_timeout(LIMIT), Ok(Ok(())));
}

#[test]
fn a_breakpoint_in_the_entry_chunk_and_in_boot_stops_the_start() {
    let vm = debugged(
        "let ready = true;\nfn boot() {\n  let n = 1;\n}",
        main_at(&[1, 3]),
    );
    let first = vm.stop();
    assert_eq!(first.frames[0].line, Some(1));
    assert_eq!(first.frames[0].function, "main");
    vm.resume(Resume::Continue);
    assert_eq!(vm.stop().frames[0].function, "boot");
    assert_eq!(vm.finish(), Ok(()));
}

#[test]
fn a_breakpoint_in_an_imported_module_stops_there() {
    const MODULES: &[(&str, &str)] = &[(
        "code/enemy.rhai",
        "fn hit(n) {\n  let left = n - 1;\n  left\n}",
    )];
    let vm = debugged_with(
        "import \"enemy\" as enemy;\nfn tick() {\n  enemy::hit(3);\n}",
        MODULES,
        vec![at("code/enemy.rhai", 2)],
        SMALL,
    );
    let stop = vm.stop();
    assert_eq!(stop.frames[0].path.as_deref(), Some("code/enemy.rhai"));
    assert_eq!(stop.frames[0].function, "hit");
    assert_eq!(
        names(&stop.variables),
        [("n", "3")],
        "module functions have no `this`"
    );
    assert_eq!(vm.finish(), Ok(()));
}

#[test]
fn step_over_walks_the_lines_of_a_function_without_entering_calls() {
    let vm = debugged(LINES, main_at(&[6]));
    let first = vm.stop();
    assert_eq!(
        vm.lines_with(&first, Resume::Step(Step::Over)),
        [6, 7, 8, 9]
    );
}

#[test]
fn step_into_enters_the_callee_and_comes_back() {
    let vm = debugged(LINES, main_at(&[7]));
    let first = vm.stop();
    let lines = vm.lines_with(&first, Resume::Step(Step::Into));
    assert_eq!(lines, [7, 2, 3, 8, 9]);
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
    assert_eq!(back.frames[0].line, Some(8));
    assert_eq!(vm.finish(), Ok(()));
}

#[test]
fn continuing_after_a_step_stops_only_at_the_users_breakpoints() {
    let vm = debugged(LINES, main_at(&[6]));
    vm.stop();
    vm.resume(Resume::Step(Step::Over));
    assert_eq!(vm.stop().frames[0].line, Some(7));
    vm.resume(Resume::Continue);
    assert_eq!(vm.finished.recv_timeout(LIMIT), Ok(Ok(())));
}

#[test]
fn this_and_nested_maps_and_arrays_expand_one_level_at_a_time() {
    const SOURCE: &str = "fn boot() {
  this.hp = 3;
  this.pos = #{x: 1, y: 2};
  this.items = [10, 20];
  this.none = #{};
}
fn tick() {
  let here = 1;
}";
    let vm = debugged(SOURCE, main_at(&[8]));
    let stop = vm.stop();
    assert_eq!(names(&stop.variables)[0], ("this", "{..}"));
    assert!(stop.variables[0].expandable);

    let entries = vm.expand(&[0]);
    assert_eq!(
        names(&entries),
        [
            ("hp", "3"),
            ("items", "[..]"),
            ("none", "#{}"),
            ("pos", "{..}")
        ]
    );
    assert!(!entries[2].expandable, "an empty map has nothing to show");
    assert_eq!(names(&vm.expand(&[0, 3])), [("x", "1"), ("y", "2")]);
    assert_eq!(names(&vm.expand(&[0, 1])), [("[0]", "10"), ("[1]", "20")]);
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
            path: vec![0, 0]
        }),
        DebugReply::Variables(Vec::new()),
        "an integer has no entries"
    );
    assert_eq!(
        vm.ask(DebugRequest::Expand {
            frame: 1,
            path: vec![0]
        }),
        DebugReply::Unavailable
    );
    assert_eq!(vm.finish(), Ok(()));
}

#[test]
fn a_long_array_lists_a_bounded_number_of_entries() {
    let vm = debugged(
        "fn tick() {\n  let big = [];\n  for i in 0..500 { big.push(i); }\n  let here = 1;\n}",
        main_at(&[4]),
    );
    vm.stop();
    let entries = vm.expand(&[1]);
    assert_eq!(entries.len(), 64);
    assert_eq!(entries[63].name, "[63]");
    assert_eq!(vm.finish(), Ok(()));
}

#[test]
fn values_are_shown_in_short_form_and_a_shadowed_name_once() {
    let vm = debugged(
        "fn tick() {
  let text = \"hello\";
  let ratio = 1.5;
  let flag = true;
  let letter = 'c';
  let nothing = ();
  let n = 1;
  let n = 2;
  let here = 1;
}",
        main_at(&[9]),
    );
    let stop = vm.stop();
    assert_eq!(
        names(&stop.variables),
        [
            ("this", "#{}"),
            ("text", "\"hello\""),
            ("ratio", "1.5"),
            ("flag", "true"),
            ("letter", "'c'"),
            ("nothing", "()"),
            ("n", "2"),
        ]
    );
    assert_eq!(vm.finish(), Ok(()));
}

#[test]
fn breakpoints_sent_while_stopped_take_effect_in_the_same_call() {
    let vm = debugged(LINES, main_at(&[6]));
    vm.stop();
    assert!(vm.link.send(DebugRequest::Breakpoints(main_at(&[8]))));
    vm.resume(Resume::Continue);
    assert_eq!(vm.stop().frames[0].line, Some(8));
    assert_eq!(vm.finish(), Ok(()));
}

#[test]
fn time_spent_stopped_is_not_charged_to_the_budget() {
    let vm = debugged_with(
        "fn tick() {\n  let total = 0;\n  for i in 0..200 {\n    total += i;\n  }\n}",
        &[],
        main_at(&[4]),
        Budget { operations: 1_500 },
    );
    for _ in 0..200 {
        vm.stop();
        vm.resume(Resume::Continue);
    }
    assert_eq!(vm.finished.recv_timeout(LIMIT), Ok(Ok(())));
}

#[test]
fn stopping_a_stopped_engine_ends_the_call_even_inside_try_catch() {
    let vm = debugged(
        "fn tick() {\n  let a = 1;\n  try {\n    loop { }\n  } catch {\n    loop { }\n  }\n}",
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
fn stopping_a_running_engine_ends_the_call() {
    let vm = debugged_with(
        "fn tick() { loop { } }",
        &[],
        Vec::new(),
        Budget {
            operations: u64::MAX,
        },
    );
    thread::sleep(Duration::from_millis(20));
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
fn a_script_error_in_a_debugged_engine_is_reported_as_usual() {
    let vm = debugged("fn tick() {\n  throw \"boom\";\n}", Vec::new());
    let result = vm.finish();
    assert!(
        matches!(&result, Err(RuntimeError { phase: Phase::Tick, error: ScriptError::Runtime(m) }) if m.starts_with("main.rhai:2")),
        "{result:?}"
    );
}

#[test]
fn an_optimizable_statement_still_stops() {
    // The default optimizer removes `if true`, and the line with it.
    let vm = debugged(
        "fn tick() {\n  let a = 1;\n  if true {\n    a += 1;\n  }\n}",
        main_at(&[3]),
    );
    assert_eq!(vm.stop().frames[0].line, Some(3));
    assert_eq!(vm.finish(), Ok(()));
}

#[test]
fn what_the_script_logged_so_far_can_be_read_while_it_is_stopped() {
    let vm = debugged(
        "fn tick() {\n  log::info(\"before\");\n  let here = 1;\n  log::info(\"after\");\n}",
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
