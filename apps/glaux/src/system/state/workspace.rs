//! Opening the editor workspace and serving the requests it makes while it
//! is the active UI.

use super::running::{save_before_running, start};
use super::{Context, Documents};
use crate::cartridge::LoadedCartridge;
use crate::cartridge::source::{SourceEditError, SourcePath};
use crate::system::clipboard::Clipboard;
use crate::system::definition::Definition;
use crate::system::editing::session::CodeSession;
use crate::system::running::RunOrigin;
use crate::system::shell::{Shell, cartridge_identity};
use crate::system::sprite_document::SpriteDocument;
use crate::system::video::CartridgeVideoDefaults;
use crate::system::workspace::{
    AssetRequest, HostWork, NavigationRequest, ScriptRequest, Tab, Workspace, WorkspaceRequest,
    WorkspaceVideo,
};
use crate::system::{HostRequest, SystemState};
use glaux_input::InputEvent;

/// Makes the workspace the active UI on `cartridge`.
///
/// A document kept from an earlier visit is brought up to date with the
/// files without losing unsaved changes or, for assets whose files are
/// unchanged, their undo history. See [`SpriteDocument::reopen`]. Without
/// one, the video assets are read afresh.
///
/// The Code view's session is kept from earlier visits, or started, and the
/// entry script opens if the workspace opens on the Code view for the
/// first time.
pub(super) fn open_workspace(
    shell: Box<Shell>,
    cartridge: LoadedCartridge,
    workspace: &mut Workspace,
    documents: Documents<'_>,
    video_defaults: &CartridgeVideoDefaults,
) -> SystemState {
    let Documents { sprites, code } = documents;
    let identity = cartridge_identity(cartridge.manifest());
    tracing::info!(cartridge = %identity, tab = ?workspace.tab(), "workspace opened");
    workspace.end_stroke(sprites.as_mut());
    let document = match sprites.take() {
        Some(mut document) => {
            workspace.reopened(document.reopen(&cartridge, video_defaults));
            Ok(document)
        }
        None => SpriteDocument::load(&cartridge, video_defaults)
            .map_err(|err| crate::system::ui_errors::video::video_load(&err)),
    };
    let video = match document {
        Ok(document) => workspace.prepare_video(sprites.insert(document)),
        Err(message) => WorkspaceVideo::unavailable(message),
    };
    open_entry_once(workspace, code.get_or_insert_default(), &cartridge);
    SystemState::Workspace {
        shell,
        cartridge,
        video,
    }
}

/// Feeds `input` to the workspace and carries out the file work it asks
/// for, returning the work it asks of the host, if any. Break returns to
/// the shell with the cartridge still loaded, unless a dialog is open,
/// which takes it as Escape. Events after a Break that leaves are dropped,
/// so nothing meant for the workspace reaches the shell. Copies and pastes
/// read and write `clipboard` as they come.
pub(in crate::system) fn workspace_tick(
    shell: Box<Shell>,
    cartridge: LoadedCartridge,
    mut video: WorkspaceVideo,
    ctx: &mut Context<'_>,
    input: &[InputEvent],
    clipboard: &mut dyn Clipboard,
) -> (SystemState, Vec<HostRequest>, bool) {
    let workspace = &mut *ctx.workspace;
    let sprites = &mut *ctx.sprites;
    let code = ctx.code.get_or_insert_default();
    workspace.set_entry_script(&cartridge.manifest().runtime.entry);
    workspace.tick(Some(code));
    let mut host_requests = Vec::new();
    let mut run_requested = false;
    for &event in input {
        if event == InputEvent::Break && !workspace.claims_break() {
            workspace.end_stroke(sprites.as_mut());
            tracing::info!("workspace closed");
            let state = SystemState::Shell {
                shell,
                loaded: Some(cartridge),
            };
            return (state, host_requests, false);
        }
        // The editors only change what they can show.
        let document = sprites.as_mut().filter(|_| video.is_ready());
        let request = workspace.handle_input(event, document, Some(code));
        open_entry_once(workspace, code, &cartridge);
        let Some(request) = request else {
            continue;
        };
        match request {
            WorkspaceRequest::Run => {
                // Events after a run is asked for are dropped, as after any
                // change of screen.
                run_requested = true;
                break;
            }
            WorkspaceRequest::SaveEditorSettings(settings) => {
                host_requests.push(HostRequest::SaveEditorSettings(settings));
            }
            WorkspaceRequest::Scripts(request) => {
                serve_scripts(request, workspace, code, &cartridge);
            }
            WorkspaceRequest::Navigate(request) => {
                serve_navigation(request, workspace, code, &cartridge);
            }
            WorkspaceRequest::Host(request) => {
                serve_host(request, workspace, code, clipboard, &mut host_requests);
            }
            WorkspaceRequest::Assets(request) => {
                if let Some(document) = sprites.as_mut().filter(|_| video.is_ready()) {
                    serve_assets(
                        request,
                        workspace,
                        document,
                        &cartridge,
                        ctx.video_defaults,
                        &mut host_requests,
                    );
                }
            }
        }
    }
    if let Some(document) = sprites.as_ref() {
        workspace.refresh_video(&mut video, document);
    }
    let state = SystemState::Workspace {
        shell,
        cartridge,
        video,
    };
    (state, host_requests, run_requested)
}

/// Carries out file work on the cartridge's scripts.
fn serve_scripts(
    request: ScriptRequest,
    workspace: &mut Workspace,
    code: &mut CodeSession,
    cartridge: &LoadedCartridge,
) {
    match request {
        ScriptRequest::List => workspace.scripts_listed(cartridge.script_files()),
        ScriptRequest::Open(path) => {
            let result = code.open(cartridge, &path).map(drop);
            workspace.script_opened(code, &path, result);
        }
        ScriptRequest::Save(id) => workspace.script_saved(id, code.save(cartridge, id)),
        ScriptRequest::Overwrite { id, seen } => {
            let result = code.overwrite(cartridge, id, seen.as_deref());
            workspace.script_saved(id, result);
        }
        ScriptRequest::SaveAndClose(id) => {
            let result = code.save(cartridge, id);
            let written = result.is_ok();
            workspace.script_saved(id, result);
            if written {
                code.close(id);
                workspace.script_closed(code);
            }
        }
        ScriptRequest::New => workspace.open_new_script(cartridge.manifest().runtime.language),
        ScriptRequest::Create(path) => {
            let result = cartridge
                .create_source(&path)
                .map_err(|err| crate::system::ui_errors::fs(&err))
                .and_then(|()| {
                    code.open(cartridge, &path)
                        .map(drop)
                        .map_err(|err| crate::system::ui_errors::code::document(&err))
                });
            workspace.script_created(code, &path, result);
        }
        ScriptRequest::PrepareDelete(id) => {
            let Some(path) = code.document(id).map(|document| document.path().clone()) else {
                return;
            };
            if cartridge.is_entry(&path) {
                workspace.script_deleted(code, &path, Err(SourceEditError::Entry(path.clone())));
            } else if let Some(state) = code.disk_state(cartridge, id) {
                workspace.delete_prompt(code, id, state);
            }
        }
        ScriptRequest::Delete { id, seen } => {
            let Some(path) = code.document(id).map(|document| document.path().clone()) else {
                return;
            };
            let result = cartridge.delete_source(&path, seen.as_deref());
            if result.is_ok() {
                code.close(id);
                code.forget_breakpoints(&path);
            }
            workspace.script_deleted(code, &path, result);
        }
        ScriptRequest::Rename { id, to } => {
            let result = code.move_script(cartridge, id, &to);
            workspace.script_renamed(code, &to, result);
        }
    }
}

/// Carries out a move between places in the scripts.
fn serve_navigation(
    request: NavigationRequest,
    workspace: &mut Workspace,
    code: &mut CodeSession,
    cartridge: &LoadedCartridge,
) {
    match request {
        NavigationRequest::GoToDefinition { id, caret } => {
            match crate::system::definition::locate(code, cartridge, id, caret) {
                Definition::Found(target) => workspace.jump(code, cartridge, &target),
                missed => workspace.definition_missed(&missed),
            }
        }
        NavigationRequest::Jump(target) => workspace.jump(code, cartridge, &target),
        NavigationRequest::Back => workspace.travel(code, cartridge, true),
        NavigationRequest::Forward => workspace.travel(code, cartridge, false),
    }
}

/// Carries out work that needs the host's links or clipboard. Links go to
/// the host through `host_requests`.
fn serve_host(
    request: HostWork,
    workspace: &mut Workspace,
    code: &mut CodeSession,
    clipboard: &mut dyn Clipboard,
    host_requests: &mut Vec<HostRequest>,
) {
    match request {
        HostWork::OpenExternal(url) => host_requests.push(HostRequest::OpenExternal(url)),
        HostWork::Copy(text) => clipboard.set_text(&text),
        HostWork::Paste => workspace.paste(code, clipboard.text()),
    }
}

/// Carries out file work on the sprite document `document`.
fn serve_assets(
    request: AssetRequest,
    workspace: &mut Workspace,
    document: &mut SpriteDocument,
    cartridge: &LoadedCartridge,
    video_defaults: &CartridgeVideoDefaults,
    host_requests: &mut Vec<HostRequest>,
) {
    match request {
        AssetRequest::Save => workspace.saved(document.save(cartridge)),
        AssetRequest::Overwrite(seen) => {
            workspace.saved(document.overwrite(cartridge, &seen));
        }
        AssetRequest::Reload(assets) => {
            let result = document
                .reload(cartridge, &assets, video_defaults)
                .map_err(|err| crate::system::ui_errors::video::video_load(&err));
            workspace.reloaded(&assets, result);
        }
        AssetRequest::PickSpritesheet => {
            host_requests.push(HostRequest::PickPng {
                directory: cartridge.host_dir().to_path_buf(),
            });
        }
    }
}

/// Opens the cartridge's entry script in `code` the first time the Code
/// view shows, and never again. A session whose tabs were all closed stays
/// empty.
fn open_entry_once(workspace: &mut Workspace, code: &mut CodeSession, cartridge: &LoadedCartridge) {
    if workspace.tab() != Tab::Code || !code.take_entry_request() {
        return;
    }
    let entry = &cartridge.manifest().runtime.entry;
    let result = SourcePath::parse(entry)
        .map_err(|err| crate::system::ui_errors::code::source_path(&err))
        .and_then(|path| {
            code.open(cartridge, &path)
                .map(drop)
                .map_err(|err| crate::system::ui_errors::code::document(&err))
        });
    workspace.entry_opened(code, result);
}

/// Runs the workspace's cartridge from its entry point, as the Code view
/// asks. With Save Before Running on, every modified document is written
/// first and the run is cancelled, with the failure shown, if one cannot
/// be.
pub(in crate::system) fn run_from_workspace(
    state: SystemState,
    ctx: &mut Context<'_>,
) -> SystemState {
    let SystemState::Workspace {
        shell,
        cartridge,
        video,
    } = state
    else {
        return state;
    };
    ctx.workspace.end_stroke(ctx.sprites.as_mut());
    if !save_before_running(&cartridge, ctx) {
        return SystemState::Workspace {
            shell,
            cartridge,
            video,
        };
    }
    start(shell, cartridge, RunOrigin::Code, ctx)
}
