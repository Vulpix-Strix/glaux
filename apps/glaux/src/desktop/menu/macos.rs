use std::collections::HashMap;

use anyhow::Context;
use muda::accelerator::Accelerator;
use muda::{Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem, Submenu};

use super::MenuCommand;
use crate::system::MenuState;

/// The menu bar, which replaces the one GLFW installs.
///
/// Dropping it tears the menu down, so it must be dropped before the
/// window.
pub struct NativeMenu {
    _menu: Menu,
    items: Vec<(MenuCommand, MenuItem)>,
    commands: HashMap<MenuId, MenuCommand>,
    /// What the items show, so only the ones that change are touched.
    shown: MenuState,
}

impl NativeMenu {
    /// Replaces the main menu. Must run on the main thread after the window
    /// exists, since `muda` panics elsewhere.
    ///
    /// Everything is built before `init_for_nsapp`, so a build error leaves
    /// GLFW's menu in place. Nothing can undo `init_for_nsapp` afterwards.
    ///
    /// The editing items are plain items of Glaux's own. AppKit's
    /// predefined ones send selectors down the responder chain, and the
    /// window's view answers none of them.
    pub fn install() -> anyhow::Result<Self> {
        let menu = Menu::new();
        let mut items = Vec::with_capacity(MenuCommand::ALL.len());
        let mut item = |command: MenuCommand| {
            let (mods, key) = command.shortcut();
            let item = MenuItem::with_id(
                command.id(),
                command.label(),
                false,
                Some(Accelerator::new(mods, key)),
            );
            items.push((command, item.clone()));
            item
        };

        let app = Submenu::new("Glaux", true);
        app.append_items(&[
            &PredefinedMenuItem::about(None, None),
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::services(None),
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::hide(None),
            &PredefinedMenuItem::hide_others(None),
            &PredefinedMenuItem::show_all(None),
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::quit(None),
        ])
        .context("failed to fill the application menu")?;

        let file = Submenu::new("File", true);
        file.append_items(&[
            &item(MenuCommand::NewFile),
            &item(MenuCommand::OpenFile),
            &item(MenuCommand::CloseDocument),
            &PredefinedMenuItem::separator(),
            &item(MenuCommand::Save),
        ])
        .context("failed to fill the File menu")?;

        let edit = Submenu::new("Edit", true);
        edit.append_items(&[
            &item(MenuCommand::Undo),
            &item(MenuCommand::Redo),
            &PredefinedMenuItem::separator(),
            &item(MenuCommand::Cut),
            &item(MenuCommand::Copy),
            &item(MenuCommand::Paste),
            &item(MenuCommand::SelectAll),
            &PredefinedMenuItem::separator(),
            &item(MenuCommand::Find),
            &item(MenuCommand::Replace),
            &item(MenuCommand::GoToLine),
        ])
        .context("failed to fill the Edit menu")?;

        let view = Submenu::new("View", true);
        view.append_items(&[&item(MenuCommand::ToggleSidebar)])
            .context("failed to fill the View menu")?;

        let window = Submenu::new("Window", true);
        window
            .append_items(&[
                &PredefinedMenuItem::minimize(None),
                &PredefinedMenuItem::maximize(None),
                &PredefinedMenuItem::separator(),
                &PredefinedMenuItem::bring_all_to_front(None),
            ])
            .context("failed to fill the Window menu")?;

        let help = Submenu::new("Help", true);
        help.append_items(&[&item(MenuCommand::Documentation)])
            .context("failed to fill the Help menu")?;

        menu.append_items(&[&app, &file, &edit, &view, &window, &help])
            .context("failed to fill the menu bar")?;

        menu.init_for_nsapp();
        // These two hand the submenus to AppKit, which adds the window list
        // and the menu search, so they follow `init_for_nsapp`.
        window.set_as_windows_menu_for_nsapp();
        help.set_as_help_menu_for_nsapp();

        tracing::info!(bundled = is_bundled(), "native menu bar installed");
        let commands = items
            .iter()
            .map(|(command, item)| (item.id().clone(), *command))
            .collect();
        Ok(Self {
            _menu: menu,
            items,
            commands,
            shown: MenuState::default(),
        })
    }

    /// The commands chosen since the previous call, without blocking. Call
    /// it once per frame after the host polled the platform, which is when
    /// AppKit delivers them.
    pub fn drain(&mut self) -> Vec<MenuCommand> {
        let receiver = MenuEvent::receiver();
        let mut chosen = Vec::new();
        while let Ok(event) = receiver.try_recv() {
            tracing::trace!(id = ?event.id(), "menu event received");
            match self.commands.get(event.id()) {
                Some(&command) => chosen.push(command),
                None => tracing::warn!(id = ?event.id(), "menu event with an unknown id"),
            }
        }
        chosen
    }

    /// Enables the items `state` offers and disables the others. A
    /// disabled item does not take its shortcut, so the key reaches the
    /// window as it would without the menu.
    pub fn sync(&mut self, state: MenuState) {
        if state == self.shown {
            return;
        }
        for (command, item) in &self.items {
            let enabled = command.is_enabled(state);
            if enabled != command.is_enabled(self.shown) {
                item.set_enabled(enabled);
            }
        }
        tracing::debug!(?state, "menu availability changed");
        self.shown = state;
    }
}

impl Drop for NativeMenu {
    fn drop(&mut self) {
        tracing::debug!("native menu bar removed");
    }
}

fn is_bundled() -> bool {
    std::env::current_exe()
        .map(|path| {
            path.components()
                .any(|part| part.as_os_str().to_string_lossy().ends_with(".app"))
        })
        .unwrap_or(false)
}
