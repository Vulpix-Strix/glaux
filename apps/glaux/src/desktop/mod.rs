//! Desktop-session behavior around the Glaux window: restoring its state
//! between launches, keeping Glaux to one running instance, and showing the
//! host's file dialogs.

pub mod file_picker;
pub mod single_instance;
pub mod window_state;
