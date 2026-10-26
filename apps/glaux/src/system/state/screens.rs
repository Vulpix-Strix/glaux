//! The credits and the documentation browser, shown over the shell.

use crate::cartridge::LoadedCartridge;
use crate::system::clipboard::Clipboard;
use crate::system::credits::CreditsView;
use crate::system::shell::Shell;
use crate::system::workspace::{DocsApart, Workspace, WorkspaceRequest};
use crate::system::{HostRequest, SystemState};
use glaux_input::InputEvent;
use std::collections::VecDeque;

/// Feeds `input` to the credits view, which closes on its own Escape and
/// leaves the shell as it was. Events after the close are dropped, as after
/// any change of screen.
pub(in crate::system) fn credits_tick(
    shell: Box<Shell>,
    loaded: Option<LoadedCartridge>,
    mut view: Box<CreditsView>,
    input: &[InputEvent],
) -> SystemState {
    view.tick();
    if input.iter().any(|&event| view.handle_input(event)) {
        return SystemState::Shell { shell, loaded };
    }
    SystemState::Credits {
        shell,
        loaded,
        view,
    }
}

/// Feeds `input` to the documentation browser the workspace keeps, which
/// closes on its own Escape and leaves the shell as it was. Nothing reaches
/// the shell or the workspace, whatever keys `InputMode::Workspace` lets
/// through, and events after the close are dropped.
pub(in crate::system) fn docs_tick(
    shell: Box<Shell>,
    loaded: Option<LoadedCartridge>,
    workspace: &mut Workspace,
    input: &[InputEvent],
    clipboard: &mut dyn Clipboard,
    host_requests: &mut VecDeque<HostRequest>,
) -> SystemState {
    workspace.tick(None);
    for &event in input {
        match workspace.docs_apart_input(event) {
            DocsApart::Stay => {}
            DocsApart::Close => return SystemState::Shell { shell, loaded },
            DocsApart::Request(WorkspaceRequest::OpenExternal(url)) => {
                host_requests.push_back(HostRequest::OpenExternal(url));
            }
            DocsApart::Request(WorkspaceRequest::Copy(text)) => clipboard.set_text(&text),
            DocsApart::Request(WorkspaceRequest::Paste) => {
                workspace.docs_apart_paste(clipboard.text());
            }
            DocsApart::Request(_) => {}
        }
    }
    SystemState::Docs { shell, loaded }
}
