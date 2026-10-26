//! Desktop-session behavior around the Glaux window: restoring its state
//! between launches, keeping Glaux to one running instance, and showing the
//! host's file dialogs, and opening links in other programs.

#![warn(unreachable_pub)]

pub(crate) mod external;
pub(crate) mod file_picker;
#[cfg(target_os = "macos")]
pub(crate) mod menu;
pub(crate) mod single_instance;
pub(crate) mod window_state;
