//! Tests of the runtime's observable behavior, grouped by subsystem.

mod audio;
mod compare;
mod draw;
mod input;
mod isolation;
mod limits;
mod log;
mod modules;
mod runtime;
mod time;

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use glaux_input::{Button, ButtonSet, ButtonState, InputState, Key, KeySet, KeyState};
use glaux_modules::{ModuleLoadError, ModulePath, ModuleSource};
use glaux_script::{LogEntry, Phase, RuntimeError, ScriptError, StartFailure};

use crate::*;

pub(super) const SMALL: Budget = Budget { operations: 10_000 };
pub(super) const NO_INPUT: InputState = InputState {
    buttons: ButtonState {
        held: ButtonSet::EMPTY,
        pressed: ButtonSet::EMPTY,
    },
    keys: KeyState {
        held: KeySet::EMPTY,
        pressed: KeySet::EMPTY,
    },
};

/// In-memory modules, keyed by cartridge-relative path, that record
/// every path requested from them.
#[derive(Default)]
struct Files {
    files: HashMap<&'static str, Vec<u8>>,
    unreadable: Vec<&'static str>,
    requested: Rc<RefCell<Vec<String>>>,
}

impl Files {
    fn new(files: &[(&'static str, &str)]) -> Self {
        Self {
            files: files
                .iter()
                .map(|(path, source)| (*path, source.as_bytes().to_vec()))
                .collect(),
            ..Self::default()
        }
    }
}

impl ModuleSource for Files {
    fn load(&self, path: &ModulePath) -> Result<Vec<u8>, ModuleLoadError> {
        self.requested.borrow_mut().push(path.to_string());
        if self.unreadable.contains(&path.as_str()) {
            return Err(ModuleLoadError::Unreadable);
        }
        self.files
            .get(path.as_str())
            .cloned()
            .ok_or(ModuleLoadError::NotFound)
    }
}

fn start(source: &str) -> Result<Runtime, StartFailure> {
    start_with(source, Files::default())
}

fn start_with(source: &str, files: Files) -> Result<Runtime, StartFailure> {
    Runtime::start("main.rhai", source.as_bytes(), files, SMALL)
}

fn started_with(source: &str, files: Files) -> Runtime {
    match start_with(source, files) {
        Ok(runtime) => runtime,
        Err(failure) => panic!("expected the script to start, got {failure:?}"),
    }
}

fn start_failure_with(source: &str, files: Files) -> StartFailure {
    match start_with(source, files) {
        Ok(_) => panic!("expected the script to fail to start"),
        Err(failure) => failure,
    }
}

fn started(source: &str) -> Runtime {
    started_with(source, Files::default())
}

fn start_failure(source: &str) -> StartFailure {
    start_failure_with(source, Files::default())
}

/// The text of each entry, dropping levels.
fn texts(entries: Vec<LogEntry>) -> Vec<String> {
    entries.into_iter().map(|entry| entry.text).collect()
}

/// Starts `source` and returns what it logged while starting.
fn start_output(source: &str) -> Vec<String> {
    texts(started(source).take_output())
}

fn error(phase: Phase, error: ScriptError) -> RuntimeError {
    RuntimeError { phase, error }
}

fn runtime_error(phase: Phase, message: &str) -> RuntimeError {
    error(phase, ScriptError::Runtime(message.to_owned()))
}

fn limit(phase: Phase) -> RuntimeError {
    error(phase, ScriptError::LimitExceeded)
}

/// The runtime error message of a script that fails while starting.
fn start_message(source: &str) -> String {
    match start_failure(source).error.error {
        ScriptError::Runtime(message) => message,
        other => panic!("expected a runtime error, got {other:?}"),
    }
}

/// The runtime error message of the first tick of `source`.
fn tick_message(source: &str) -> String {
    match started(source).tick(NO_INPUT) {
        Err(RuntimeError {
            error: ScriptError::Runtime(message),
            ..
        }) => message,
        other => panic!("expected a runtime error, got {other:?}"),
    }
}

/// Input with only these buttons held and pressed.
fn buttons(held: &[Button], pressed: &[Button]) -> InputState {
    InputState {
        buttons: ButtonState {
            held: held.iter().copied().collect(),
            pressed: pressed.iter().copied().collect(),
        },
        ..NO_INPUT
    }
}

/// Input with only these keys held and pressed.
fn keys(held: &[Key], pressed: &[Key]) -> InputState {
    InputState {
        keys: KeyState {
            held: held.iter().copied().collect(),
            pressed: pressed.iter().copied().collect(),
        },
        ..NO_INPUT
    }
}
