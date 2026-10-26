//! The native menu bar, which only macOS has. Elsewhere this does nothing.

use glaux_input::InputEvent;

use crate::system::System;

#[cfg(target_os = "macos")]
pub(super) use macos::Menu;
#[cfg(not(target_os = "macos"))]
pub(super) use none::Menu;

#[cfg(target_os = "macos")]
mod macos {
    use super::{InputEvent, System};
    use crate::desktop::menu::NativeMenu;

    pub(in crate::app) struct Menu(Option<NativeMenu>);

    impl Menu {
        /// Installs the menu bar. The window must exist already.
        pub(in crate::app) fn install() -> Self {
            Self(
                NativeMenu::install()
                    .inspect_err(|err| {
                        tracing::error!(error = %format!("{err:#}"), "native menu not installed")
                    })
                    .ok(),
            )
        }

        /// Carries out the commands chosen in the menu bar since the last
        /// frame.
        ///
        /// A command the system no longer offers is dropped: the menu shows
        /// what it offered a frame ago, and a shortcut can arrive before it
        /// catches up. The others send the event the keyboard shortcut
        /// sends, except documentation, which the system opens itself.
        pub(in crate::app) fn run_commands(
            &mut self,
            mut system: System,
            pending: &mut Vec<InputEvent>,
        ) -> System {
            let Some(menu) = &mut self.0 else {
                return system;
            };
            for command in menu.drain() {
                if !command.is_enabled(system.menu_state()) {
                    tracing::debug!(?command, "menu command dropped, not available now");
                    continue;
                }
                tracing::debug!(?command, "menu command chosen");
                match command.input_event() {
                    Some(event) => pending.push(event),
                    None => system = system.open_documentation(),
                }
            }
            system
        }

        /// Shows what `system` offers.
        pub(in crate::app) fn sync(&mut self, system: &System) {
            if let Some(menu) = &mut self.0 {
                menu.sync(system.menu_state());
            }
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod none {
    use super::{InputEvent, System};

    pub(in crate::app) struct Menu;

    impl Menu {
        pub(in crate::app) fn install() -> Self {
            Self
        }

        pub(in crate::app) fn run_commands(
            &mut self,
            system: System,
            _pending: &mut Vec<InputEvent>,
        ) -> System {
            system
        }

        pub(in crate::app) fn sync(&mut self, _system: &System) {}
    }
}
