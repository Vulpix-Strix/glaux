//! Routing of input events to the modal dialog, the tab bar, the
//! documentation browser or the open view's editor.

use super::code::CodeRequest;
use super::docs::{DocTarget, DocsRequest};
use super::modal::{Choice, Modal, Target, close_dialog};
use super::sprites::SpriteRequest;
use super::tab::{Rect, Tab};
use super::{
    AssetRequest, HostWork, NavigationRequest, ScriptRequest, Workspace, WorkspaceRequest,
    docs_source,
};
use crate::system::editing::session::CodeSession;
use crate::system::screen::PALE;
use crate::system::sprite_document::SpriteDocument;
use crate::ui_text;
use glaux_i18n::Arg;
use glaux_input::{InputEvent, PointerButton};

/// Who receives an input event that is not a tab or docs shortcut: the
/// first of these that applies, in the order [`Workspace::handle_input`]
/// tries them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Route {
    /// The workspace's own dialog.
    Modal,
    /// One of the sprite editor's dialogs, or its wait for the host's file
    /// dialog.
    SpriteDialog,
    /// One of the Code view's overlays.
    CodeOverlay,
    /// The documentation browser over the selected view.
    Docs,
    Sprites,
    Code,
    /// A view with no editor of its own.
    Inert,
}

impl Workspace {
    pub(super) fn route(&self) -> Route {
        if self.modal.is_some() {
            Route::Modal
        } else if self.sprites.has_dialog() {
            Route::SpriteDialog
        } else if self.code.has_overlay() {
            Route::CodeOverlay
        } else if self.docs_open {
            Route::Docs
        } else {
            match self.tab {
                Tab::Sprites => Route::Sprites,
                Tab::Code => Route::Code,
                Tab::Map | Tab::Sfx | Tab::Music => Route::Inert,
            }
        }
    }

    /// Applies `event`, letting the sprite editor change `sprites` and the
    /// Code view change `code`, and returns the file work it calls for, if
    /// any.
    ///
    /// While a dialog is open it takes every event, Escape included. So
    /// do the sprite editor's own dialogs and the Code view's overlays.
    /// Otherwise Ctrl+Tab and Ctrl+Shift+Tab cycle the tabs, a primary
    /// press on a tab selects it, and everything else goes to the open
    /// view's editor, if it has one.
    pub fn handle_input(
        &mut self,
        event: InputEvent,
        sprites: Option<&mut SpriteDocument>,
        code: Option<&mut CodeSession>,
    ) -> Option<WorkspaceRequest> {
        if let Some(modal) = &mut self.modal {
            let layout = modal.dialog.layout(
                &self.font,
                self.layout.view_area(),
                self.layout.cell_w,
                self.layout.cell_h,
            );
            let choice = modal.dialog.handle_input(event, &layout)?;
            let modal = self.modal.take()?;
            return match (choice, modal.target) {
                (Choice::Overwrite, Target::Assets(seen)) => {
                    Some(WorkspaceRequest::Assets(AssetRequest::Overwrite(seen)))
                }
                (Choice::Overwrite, Target::Script { id, seen }) => {
                    Some(WorkspaceRequest::Scripts(ScriptRequest::Overwrite {
                        id,
                        seen,
                    }))
                }
                (Choice::Reload, Target::Assets(seen)) => Some(WorkspaceRequest::Assets(
                    AssetRequest::Reload(seen.iter().map(|disk| disk.asset).collect()),
                )),
                (Choice::Delete, Target::DeleteScript { id, seen }) => {
                    Some(WorkspaceRequest::Scripts(ScriptRequest::Delete {
                        id,
                        seen,
                    }))
                }
                (Choice::Save, Target::CloseScript(id)) => {
                    Some(WorkspaceRequest::Scripts(ScriptRequest::SaveAndClose(id)))
                }
                (Choice::Discard, Target::CloseScript(id)) => {
                    if let Some(code) = code {
                        code.close(id);
                        self.script_closed(code);
                    }
                    None
                }
                // Script conflicts offer no reload, and messages neither.
                _ => None,
            };
        }
        let route = self.route();
        if route == Route::SpriteDialog {
            return self.sprite_input(event, sprites);
        }
        if route == Route::CodeOverlay {
            return self.code_input(event, code);
        }
        match event {
            InputEvent::NextView => {
                self.docs_open = false;
                self.switch_to(self.tab.next(), sprites);
            }
            InputEvent::PreviousView => {
                self.docs_open = false;
                self.switch_to(self.tab.previous(), sprites);
            }
            InputEvent::PointerDown {
                button: PointerButton::Primary,
                at,
                ..
            } if self.layout.tab_at(at).is_some() => {
                if let Some(tab) = self.layout.tab_at(at) {
                    self.docs_open = false;
                    self.switch_to(tab, sprites);
                }
            }
            InputEvent::PointerDown {
                button: PointerButton::Primary,
                at,
                ..
            } if self.docs_button().contains(at) => self.toggle_docs(sprites),
            // With a script open, the Code view decides what F1 shows.
            InputEvent::Docs if !self.docs_open && !(self.tab == Tab::Code && code.is_some()) => {
                self.toggle_docs(sprites)
            }
            InputEvent::Run if matches!(route, Route::Code | Route::Sprites) => {
                return Some(WorkspaceRequest::Run);
            }
            InputEvent::Debug if matches!(route, Route::Code | Route::Sprites) => {
                return Some(WorkspaceRequest::Debug);
            }
            _ if route == Route::Docs => return self.docs_input(event),
            _ if route == Route::Sprites => return self.sprite_input(event, sprites),
            _ if route == Route::Code => return self.code_input(event, code),
            _ => {}
        }
        None
    }

    /// The Docs button in the tab bar.
    pub(super) fn docs_button(&self) -> Rect {
        self.layout.docs_button(self.tab_labels.docs(), &self.font)
    }

    /// Shows the documentation browser over the selected view, or hides it.
    /// Its tabs and history stay for the next time. Neither the Code view's
    /// documents nor the sprite editor's are touched.
    pub(super) fn toggle_docs(&mut self, sprites: Option<&mut SpriteDocument>) {
        if self.docs_open {
            self.docs_open = false;
            return;
        }
        // A drag must not outlive the view it started in.
        self.end_stroke(sprites);
        self.dock_docs();
        self.docs.show(docs_source());
        self.docs_open = true;
    }

    /// Shows the browser on `target`, in a new tab or in the one that has
    /// the page already. Neither the Code view's documents nor the sprite
    /// editor's are touched.
    fn open_docs(&mut self, target: DocTarget) {
        let source = docs_source();
        if source.library.page(&target.id, source.chain).is_none() {
            self.notify(
                ui_text::get_with(
                    "code-no-documentation-for",
                    &[("value", Arg::Text(target.id.as_str()))],
                ),
                PALE,
            );
            return;
        }
        self.end_stroke(None);
        self.dock_docs();
        let refused = self.docs.show_target(source, target);
        self.docs_open = true;
        if let Some(DocsRequest::Notice(text)) = refused {
            self.notify(text, PALE);
        }
    }

    /// Shows the browser with its search open.
    fn search_docs(&mut self) {
        self.end_stroke(None);
        self.dock_docs();
        self.docs.show_search(docs_source());
        self.docs_open = true;
    }

    /// Puts the browser below the tab bar, where the workspace draws it.
    fn dock_docs(&mut self) {
        self.docs.set_top(self.layout.docs_metrics().top);
    }

    /// Hands `event` to the documentation browser and carries out what it
    /// asks.
    fn docs_input(&mut self, event: InputEvent) -> Option<WorkspaceRequest> {
        let request = self.docs.handle_input(event, docs_source())?;
        self.docs_request(request).map(WorkspaceRequest::Host)
    }

    /// Carries out what the documentation browser asked for.
    pub(super) fn docs_request(&mut self, request: DocsRequest) -> Option<HostWork> {
        match request {
            DocsRequest::Close => {
                self.docs_open = false;
                None
            }
            DocsRequest::Notice(text) => {
                self.notify(text, PALE);
                None
            }
            DocsRequest::OpenExternal(url) => {
                self.notify(
                    ui_text::get_with("workspace-opening", &[("value", Arg::Text(url.as_str()))]),
                    PALE,
                );
                Some(HostWork::OpenExternal(url))
            }
            DocsRequest::Copy(text) => Some(HostWork::Copy(text)),
            DocsRequest::Paste => Some(HostWork::Paste),
        }
    }

    /// Hands `event` to the Code view and passes on the file work it asks
    /// for.
    fn code_input(
        &mut self,
        event: InputEvent,
        mut code: Option<&mut CodeSession>,
    ) -> Option<WorkspaceRequest> {
        match self.code.handle_input(event, code.as_deref_mut())? {
            CodeRequest::ListScripts => Some(WorkspaceRequest::Scripts(ScriptRequest::List)),
            CodeRequest::Open(path) => Some(WorkspaceRequest::Scripts(ScriptRequest::Open(path))),
            CodeRequest::SaveSettings(settings) => {
                Some(WorkspaceRequest::SaveEditorSettings(settings))
            }
            CodeRequest::Save(id) => Some(WorkspaceRequest::Scripts(ScriptRequest::Save(id))),
            CodeRequest::Run => Some(WorkspaceRequest::Run),
            CodeRequest::StartDebug => Some(WorkspaceRequest::Debug),
            CodeRequest::Debug(action) => Some(WorkspaceRequest::Stopped(action)),
            CodeRequest::GoToDefinition { id, caret } => Some(WorkspaceRequest::Navigate(
                NavigationRequest::GoToDefinition { id, caret },
            )),
            CodeRequest::Jump(target) => {
                Some(WorkspaceRequest::Navigate(NavigationRequest::Jump(target)))
            }
            CodeRequest::Back => Some(WorkspaceRequest::Navigate(NavigationRequest::Back)),
            CodeRequest::Forward => Some(WorkspaceRequest::Navigate(NavigationRequest::Forward)),
            CodeRequest::Copy(text) => Some(WorkspaceRequest::Host(HostWork::Copy(text))),
            CodeRequest::Paste => Some(WorkspaceRequest::Host(HostWork::Paste)),
            CodeRequest::NewFile => Some(WorkspaceRequest::Scripts(ScriptRequest::New)),
            CodeRequest::Create(path) => {
                Some(WorkspaceRequest::Scripts(ScriptRequest::Create(path)))
            }
            CodeRequest::DeleteScript(id) => {
                Some(WorkspaceRequest::Scripts(ScriptRequest::PrepareDelete(id)))
            }
            CodeRequest::Rename { id, to } => {
                Some(WorkspaceRequest::Scripts(ScriptRequest::Rename { id, to }))
            }
            CodeRequest::Notice(text) => {
                self.notify(text, PALE);
                None
            }
            CodeRequest::OpenDocs(target) => {
                self.open_docs(target);
                None
            }
            CodeRequest::SearchDocs => {
                self.search_docs();
                None
            }
            CodeRequest::ConfirmClose(id) => {
                let document = code.as_deref().and_then(|code| code.document(id))?;
                self.code.end_drag();
                self.modal = Some(Modal {
                    dialog: close_dialog(document.path()),
                    target: Target::CloseScript(id),
                });
                None
            }
        }
    }

    /// Hands `event` to the sprite editor and carries out what it asks.
    fn sprite_input(
        &mut self,
        event: InputEvent,
        sprites: Option<&mut SpriteDocument>,
    ) -> Option<WorkspaceRequest> {
        match self.sprites.handle_input(event, sprites)? {
            SpriteRequest::Save => Some(WorkspaceRequest::Assets(AssetRequest::Save)),
            SpriteRequest::Import => Some(WorkspaceRequest::Assets(AssetRequest::PickSpritesheet)),
            SpriteRequest::Notice(text) => {
                self.notify(text, PALE);
                None
            }
        }
    }

    /// Whether Escape stays in the workspace, closing what is open there,
    /// rather than returning to the shell.
    pub fn claims_break(&self) -> bool {
        self.has_modal() || self.docs_open || (self.tab == Tab::Code && self.code.claims_break())
    }

    pub(super) fn switch_to(&mut self, tab: Tab, sprites: Option<&mut SpriteDocument>) {
        if tab != self.tab {
            self.end_stroke(sprites);
        }
        self.tab = tab;
    }

    /// Ends any drag in progress, closing its edit in `sprites`. A drag
    /// must not outlive the view or the visit it started in.
    pub fn end_stroke(&mut self, sprites: Option<&mut SpriteDocument>) {
        self.sprites.end_stroke(sprites);
        self.code.end_drag();
    }
}
