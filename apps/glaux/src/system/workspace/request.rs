//! What the workspace asks of the system, grouped by what carries it out.
//!
//! The workspace never touches the cartridge's files or the host. It asks
//! with a [`WorkspaceRequest`], and the system answers through the methods
//! its docs name, so a request is complete once the answer has reached the
//! workspace.

use crate::cartridge::VideoAsset;
use crate::cartridge::source::SourcePath;
use crate::system::code_document::DocumentId;
use crate::system::code_session::Destination;
use crate::system::editor_settings::EditorSettings;
use crate::system::external_url::ExternalUrl;
use crate::system::sprite_document::DiskAsset;

/// Work the workspace asks the system to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspaceRequest {
    /// Run the cartridge from its entry point. The system saves what the
    /// editors hold first when told to, and cancels the run if that fails.
    Run,
    /// Keep the Code view's settings for the next launch.
    SaveEditorSettings(EditorSettings),
    /// File work on the sprite document.
    Assets(AssetRequest),
    /// File work on the cartridge's scripts.
    Scripts(ScriptRequest),
    /// Moving between places in the scripts.
    Navigate(NavigationRequest),
    /// Links and the clipboard.
    Host(HostWork),
}

/// File work on the sprite document, which only runs while the cartridge's
/// video state is ready.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssetRequest {
    /// Save every modified asset of the sprite document.
    Save,
    /// Save the sprite document, writing over these files, which changed
    /// outside Glaux, provided each still holds what the user was shown.
    Overwrite(Vec<DiskAsset>),
    /// Discard the changes to these assets, and the history steps involving
    /// them, and read their files again.
    Reload(Vec<VideoAsset>),
    /// Ask the host's file dialog for a PNG spritesheet to import. The
    /// workspace takes no other input until [`Workspace::sheet_read`] or
    /// [`Workspace::file_dialog_closed`] answers it, or Escape stops the
    /// wait.
    ///
    /// [`Workspace::sheet_read`]: super::Workspace::sheet_read
    /// [`Workspace::file_dialog_closed`]: super::Workspace::file_dialog_closed
    PickSpritesheet,
}

/// File work on the cartridge's scripts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScriptRequest {
    /// List the cartridge's scripts for OPEN FILE. Answer with
    /// [`Workspace::scripts_listed`](super::Workspace::scripts_listed).
    List,
    /// Open this script in a tab, or show its tab if it has one. Answer
    /// with [`Workspace::script_opened`](super::Workspace::script_opened).
    Open(SourcePath),
    /// Write this script document to its file. Answer with
    /// [`Workspace::script_saved`](super::Workspace::script_saved).
    Save(DocumentId),
    /// Write this script document over its file, which changed outside
    /// Glaux, provided it still holds `seen` (`None` means it is still
    /// absent). Answer with
    /// [`Workspace::script_saved`](super::Workspace::script_saved).
    Overwrite {
        id: DocumentId,
        seen: Option<Vec<u8>>,
    },
    /// Write this script document to its file and, once it is written,
    /// close its tab. Answer with
    /// [`Workspace::script_saved`](super::Workspace::script_saved) and, if
    /// it closed, [`Workspace::script_closed`](super::Workspace::script_closed).
    SaveAndClose(DocumentId),
    /// Ask for a new script's path. Answer with
    /// [`Workspace::open_new_script`](super::Workspace::open_new_script)
    /// and the cartridge's language.
    New,
    /// Create this empty script and open it in a tab. Answer with
    /// [`Workspace::script_created`](super::Workspace::script_created).
    Create(SourcePath),
    /// Read the state of this script's file for DELETE SCRIPT. Answer with
    /// [`Workspace::delete_prompt`](super::Workspace::delete_prompt).
    PrepareDelete(DocumentId),
    /// Delete this script's file and close its tab, provided the file still
    /// holds `seen` (`None` means it is still absent). Answer with
    /// [`Workspace::script_deleted`](super::Workspace::script_deleted).
    Delete {
        id: DocumentId,
        seen: Option<Vec<u8>>,
    },
    /// Move this script's file to this path. Answer with
    /// [`Workspace::script_renamed`](super::Workspace::script_renamed).
    Rename { id: DocumentId, to: SourcePath },
}

/// Moving between places in the scripts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NavigationRequest {
    /// Go to the definition of the name at byte `caret` of this script.
    /// Answer with [`Workspace::jump`](super::Workspace::jump), or
    /// [`Workspace::definition_missed`](super::Workspace::definition_missed)
    /// when there is none.
    GoToDefinition { id: DocumentId, caret: usize },
    /// Move to this place and remember where the jump started. Answer with
    /// [`Workspace::jump`](super::Workspace::jump).
    Jump(Destination),
    /// Return to the place before the last jump. Answer with
    /// [`Workspace::travel`](super::Workspace::travel).
    Back,
    /// Go again to the place the last Back left. Answer with
    /// [`Workspace::travel`](super::Workspace::travel).
    Forward,
}

/// Work that needs the host's links or clipboard.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostWork {
    /// Open this link in the program the host uses for it.
    OpenExternal(ExternalUrl),
    /// Put this text on the host's clipboard.
    Copy(String),
    /// Read the host's clipboard. Answer with
    /// [`Workspace::paste`](super::Workspace::paste).
    Paste,
}
