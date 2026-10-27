//! How the system moves from one [`SystemState`] to the
//! next, one module per state.
//!
//! Each transition takes the state it leaves by value and returns the one
//! that follows, borrowing what the states share through a [`Context`].

pub(super) mod confirm;
pub(super) mod debugging;
pub(super) mod logs;
pub(super) mod paused;
pub(super) mod running;
pub(super) mod screens;
pub(super) mod shell;
pub(super) mod workspace;

use crate::cartridge::LoadedCartridge;
use crate::system::SystemState;
use crate::system::editing::session::CodeSession;
use crate::system::logs::{Category, LogPanel, LogStore};
use crate::system::sprite_document::SpriteDocument;
use crate::system::unsaved::UnsavedFlow;
use crate::system::video::CartridgeVideoDefaults;
use crate::system::workspace::Workspace;
use crate::ui_text;
use glaux_i18n::Arg;

/// What the states lend the shell and the question about unsaved edits.
pub(super) struct Context<'a> {
    pub(super) video_defaults: &'a CartridgeVideoDefaults,
    pub(super) workspace: &'a mut Workspace,
    pub(super) sprites: &'a mut Option<SpriteDocument>,
    pub(super) code: &'a mut Option<CodeSession>,
    pub(super) logs: &'a mut LogStore,
    pub(super) log_panel: &'a mut LogPanel,
    pub(super) unsaved: &'a mut Option<UnsavedFlow>,
}

impl Context<'_> {
    /// Whether the editors hold edits not written to the cartridge.
    fn has_unsaved(&self) -> bool {
        self.sprites
            .as_ref()
            .is_some_and(SpriteDocument::is_modified)
            || self.code.as_ref().is_some_and(CodeSession::is_modified)
    }
}

/// The cartridge `state` holds, if any.
fn cartridge_of(state: &SystemState) -> Option<&LoadedCartridge> {
    match state {
        SystemState::Boot { .. } => None,
        SystemState::Shell { loaded, .. }
        | SystemState::Credits { loaded, .. }
        | SystemState::Docs { loaded, .. } => loaded.as_ref(),
        SystemState::Workspace { cartridge, .. } => Some(cartridge),
        SystemState::Running { running, .. }
        | SystemState::Paused { running, .. }
        | SystemState::Debugging { running, .. } => Some(running.cartridge()),
    }
}

/// Shows `message` where `state` shows notices: the shell's scrollback or
/// the workspace's status bar.
fn report(state: &mut SystemState, workspace: &mut Workspace, message: String) {
    match state {
        SystemState::Shell { shell, .. } | SystemState::Paused { shell, .. } => {
            shell.report(message);
        }
        SystemState::Workspace { .. } | SystemState::Debugging { .. } => workspace.report(message),
        SystemState::Boot { .. }
        | SystemState::Running { .. }
        | SystemState::Credits { .. }
        | SystemState::Docs { .. } => {}
    }
}

/// The documents of the loaded cartridge, lent together.
pub(super) struct Documents<'a> {
    pub(super) sprites: &'a mut Option<SpriteDocument>,
    pub(super) code: &'a mut Option<CodeSession>,
}

impl Documents<'_> {
    /// Writes every modified document of the loaded cartridge: the open
    /// scripts first, then the sprite document's changed assets. Returns
    /// whether all of them were written. At the first failure it stops and
    /// shows the failure through `workspace`. What was written before it
    /// stays written, and nothing is rolled back.
    ///
    /// Every editable document of the cartridge belongs in here, so a run
    /// from Code never starts from files that miss an edit.
    pub(super) fn save_all(
        self,
        cartridge: &LoadedCartridge,
        workspace: &mut Workspace,
        logs: &mut LogStore,
    ) -> bool {
        let Self { sprites, code } = self;
        if let Some(code) = code.as_mut() {
            let modified: Vec<_> = code
                .modified()
                .iter()
                .map(|document| document.id())
                .collect();
            for id in modified {
                if let Err(err) = code.save(cartridge, id) {
                    tracing::info!("run cancelled: a script could not be saved");
                    let path = code
                        .document(id)
                        .map(|document| document.path().to_string())
                        .unwrap_or_default();
                    logs.push_terminal(
                        Category::Save,
                        ui_text::get_with(
                            "logs-run-cancelled-script-not-saved",
                            &[("path", Arg::Text(&path))],
                        ),
                        None,
                    );
                    workspace.script_saved(id, Err(err));
                    return false;
                }
            }
        }
        if let Some(document) = sprites.as_mut().filter(|document| document.is_modified())
            && let Err(err) = document.save(cartridge)
        {
            tracing::info!("run cancelled: the graphics could not be saved");
            logs.push_terminal(
                Category::Save,
                ui_text::get("logs-run-cancelled-graphics-not-saved"),
                None,
            );
            workspace.saved(Err(err));
            return false;
        }
        true
    }
}

/// Drops the documents of a cartridge that was replaced or unloaded, and
/// everything the workspace showed of them.
fn discard_documents(
    sprites: &mut Option<SpriteDocument>,
    code: &mut Option<CodeSession>,
    workspace: &mut Workspace,
) {
    if sprites
        .take()
        .is_some_and(|document| document.is_modified())
    {
        tracing::info!("unsaved graphics changes discarded");
    }
    if let Some(session) = code.take() {
        if session.is_modified() {
            tracing::info!("unsaved code changes discarded");
        }
        tracing::debug!("code documents closed");
    }
    workspace.forget_code();
}
