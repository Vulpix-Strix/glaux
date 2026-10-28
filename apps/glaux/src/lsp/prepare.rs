//! Everything that has to touch the disk before a session can start.
//!
//! [`prepare`] is blocking, so it only runs through
//! [`LspHandle::spawn_blocking`](glaux_lsp::LspHandle::spawn_blocking). It
//! checks a flag between steps and stops early, removing what it made, when
//! the cartridge it works for is no longer wanted.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use glaux_docs::{Kind, Library, ScriptLanguage};
use glaux_lsp::uri::path_to_uri;

use super::workspace::{Shadow, module_name};
use super::{definitions, locate};
use crate::cartridge::LoadedCartridge;
use crate::cartridge::source::SourcePath;

pub(super) struct Input {
    pub cartridge: LoadedCartridge,
    pub cache_dir: PathBuf,
    pub config_dir: Option<PathBuf>,
    /// Where the workspace goes.
    pub workspace: PathBuf,
}

pub(super) struct PreparedModule {
    pub name: String,
    pub path: SourcePath,
    pub uri: String,
    pub text: String,
}

pub(super) struct Prepared {
    /// Handed back so the session can read the files that are not in tabs.
    pub cartridge: LoadedCartridge,
    pub binary: PathBuf,
    pub definitions: PathBuf,
    pub shadow: Shadow,
    pub modules: Vec<PreparedModule>,
    /// Names the host calls itself, such as `tick`.
    pub callbacks: Vec<String>,
}

pub(super) enum PrepareError {
    /// There is nothing to run, or nowhere to keep its files.
    Unavailable(String),
    /// The workspace could not be made.
    Failed(String),
    /// The cartridge was unloaded or replaced meanwhile.
    Cancelled,
}

/// Microseconds each module takes to prepare, to make a slow disk for tests
/// of what happens when a cartridge is replaced mid-way.
#[cfg(test)]
pub(crate) static SLOW_MODULES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn check(cancel: &AtomicBool) -> Result<(), PrepareError> {
    if cancel.load(Ordering::Relaxed) {
        Err(PrepareError::Cancelled)
    } else {
        Ok(())
    }
}

pub(super) fn prepare(input: Input, cancel: &AtomicBool) -> Result<Prepared, PrepareError> {
    let began = Instant::now();
    let binary = locate::find(input.config_dir.as_deref())
        .map_err(|error| PrepareError::Unavailable(error.to_string()))?;
    check(cancel)?;
    let definitions = definitions::extract(&input.cache_dir).map_err(|error| {
        PrepareError::Unavailable(format!("could not write the Luau definitions: {error}"))
    })?;
    check(cancel)?;
    let callbacks = callback_names();
    let found = began.elapsed();

    let shadow = Shadow::create(input.workspace).map_err(|error| {
        PrepareError::Failed(format!("could not prepare the workspace: {error}"))
    })?;
    // The placeholders all exist before the server is started, and the
    // modules reach it together. See `LspHandle::open_session_with`.
    let mut modules = Vec::new();
    if let Ok(listing) = input.cartridge.script_files() {
        for path in listing.paths {
            // Dropping `shadow` on the way out removes the workspace.
            check(cancel)?;
            #[cfg(test)]
            std::thread::sleep(std::time::Duration::from_micros(
                SLOW_MODULES.load(Ordering::Relaxed),
            ));
            let Some(name) = module_name(&path) else {
                continue;
            };
            let Some(text) = read_text(&input.cartridge, &path) else {
                continue;
            };
            let file = match shadow.add(&name) {
                Ok(file) => file,
                Err(error) => {
                    tracing::debug!(%error, module = name.as_str(), "no placeholder for a module");
                    continue;
                }
            };
            modules.push(PreparedModule {
                name: name.as_str().to_owned(),
                path,
                uri: path_to_uri(&file),
                text,
            });
        }
    }
    tracing::info!(
        modules = modules.len(),
        find = ?found,
        workspace = ?began.elapsed() - found,
        "the Luau analyzer's workspace is prepared"
    );
    Ok(Prepared {
        cartridge: input.cartridge,
        binary,
        definitions,
        shadow,
        modules,
        callbacks,
    })
}

fn callback_names() -> Vec<String> {
    let library = Library::embedded();
    library
        .api()
        .entries_for(ScriptLanguage::Luau)
        .filter(|entry| entry.kind() == Kind::Callback)
        .filter_map(|entry| entry.binding(ScriptLanguage::Luau))
        .map(|binding| binding.name.clone())
        .collect()
}

pub(super) fn read_text(cartridge: &LoadedCartridge, path: &SourcePath) -> Option<String> {
    String::from_utf8(cartridge.read_source(path).ok()?).ok()
}
