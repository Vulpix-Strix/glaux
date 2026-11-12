//! Tests of the runtime's observable behavior, grouped by subsystem.

mod audio;
mod catalog_consistency;
mod debug;
mod definitions_consistency;
mod draw;
mod hardware;
mod input;
mod limits;
mod log;
mod modules;
mod runtime;
mod time;

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use glaux_input::{Button, ButtonSet, ButtonState, InputState, Key, KeySet, KeyState};
use glaux_machine::bg::BgVram;
use glaux_machine::font::{BitmapFont, FontMetrics};
use glaux_machine::hardware::CartridgeHardware;
use glaux_machine::obj_palette::{ObjBank, ObjPalettes};
use glaux_machine::palette::Palette;
use glaux_machine::sprite::{ObjTileAttributes, ObjTileBank, ObjVram};
use glaux_modules::{ModuleLoadError, ModulePath, ModuleSource};

use crate::*;

pub(super) const SMALL: Budget = Budget { interrupts: 10_000 };
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

/// A font with a glyph for every printable ASCII character, each one made
/// of rows that differ from the next character's.
pub(crate) fn font() -> BitmapFont {
    let mut font = BitmapFont::new(FontMetrics::new(4, 5).unwrap());
    for ch in ' '..='~' {
        let rows: Vec<u16> = (0..5)
            .map(|row| ((u32::from(ch) * 7 + row * 3) % 15 + 1) as u16)
            .collect();
        font.insert(ch, 1, &rows).unwrap();
    }
    font
}

/// Hardware whose object tiles and palette banks make every sprite
/// distinguishable: each tile has its own pixel pattern and its own default
/// bank, and every bank maps to different master indices.
pub(crate) fn hardware() -> Box<CartridgeHardware> {
    let mut obj = ObjVram::new(
        ObjTileBank::new(),
        ObjTileAttributes::new(),
        ObjPalettes::linear(),
    );
    for tile in 0..1024 {
        for y in 0..8 {
            for x in 0..8 {
                let local = ((tile * 5 + x * 3 + y * 7) % 16) as u8;
                obj.tiles_mut().set_pixel(tile, x, y, local);
            }
        }
        obj.attributes_mut()
            .set_bank(tile, ObjBank::new((tile % 8) as u8).unwrap());
    }
    Box::new(CartridgeHardware::new(
        Palette::default(),
        obj,
        BgVram::default(),
    ))
}

fn start(source: &str) -> Result<Runtime, StartFailure> {
    start_with(source, Files::default())
}

fn start_with(source: &str, files: Files) -> Result<Runtime, StartFailure> {
    Runtime::start_on_scratch("main.luau", source.as_bytes(), files, SMALL, font())
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
    match start(source) {
        Ok(runtime) => runtime,
        Err(failure) => panic!("expected the script to start, got {failure:?}"),
    }
}

fn start_failure(source: &str) -> StartFailure {
    match start(source) {
        Ok(_) => panic!("expected the script to fail to start"),
        Err(failure) => failure,
    }
}

/// The text of each entry, dropping levels.
fn texts(entries: Vec<LogEntry>) -> Vec<String> {
    entries.into_iter().map(|entry| entry.text).collect()
}

/// Starts `source` and returns what it logged while starting.
fn start_output(source: &str) -> Vec<String> {
    texts(started(source).take_output())
}

fn runtime_message(source: &str) -> String {
    match start_failure(source).error.error {
        ScriptError::Runtime(message) => message,
        other => panic!("expected a runtime error, got {other:?}"),
    }
}

fn error(phase: Phase, error: ScriptError) -> RuntimeError {
    RuntimeError { phase, error }
}

const RUNAWAY: &str = "while true do end";
const CAUGHT_RUNAWAY: &str = "pcall(function() while true do end end) log.info(\"after\")";

fn limit(phase: Phase) -> RuntimeError {
    error(phase, ScriptError::LimitExceeded)
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
