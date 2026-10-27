//! The workspace while a debugged cartridge is stopped.

use glaux_script::debug::Stop;

use super::{Tab, Workspace};
use crate::cartridge::LoadedCartridge;
use crate::cartridge::source::SourcePath;
use crate::system::editing::session::{CodeSession, Destination, Spot};
use crate::system::workspace::code::DebugView;

impl Workspace {
    /// Shows where the cartridge stopped in the Code view, which then cannot
    /// edit the scripts.
    pub fn show_stop(&mut self, code: &mut CodeSession, cartridge: &LoadedCartridge, stop: Stop) {
        code.set_read_only(true);
        self.code.show_stop(stop);
        self.switch_to(Tab::Code, None);
        self.show_frame(code, cartridge);
    }

    /// Gives the Code view back its editing, once the cartridge goes on.
    pub fn end_stop(&mut self, code: Option<&mut CodeSession>) {
        if let Some(code) = code {
            code.set_read_only(false);
        }
        self.code.end_stop();
    }

    #[cfg(test)]
    pub fn stop_view(&self) -> Option<&DebugView> {
        self.code.debug_view()
    }

    /// Where the panel of a stopped cartridge sits.
    #[cfg(test)]
    pub fn stop_panel(&self) -> Option<crate::system::workspace::code::DebugLayout> {
        self.code.debug_layout()
    }

    pub fn stop_view_mut(&mut self) -> Option<&mut DebugView> {
        self.code.debug_view_mut()
    }

    /// Moves the Code view to the line of the selected frame. The move is
    /// not remembered for Back, since the user did not make it.
    pub fn show_frame(&mut self, code: &mut CodeSession, cartridge: &LoadedCartridge) {
        let Some((path, line)) = self.code.debug_view().and_then(DebugView::place) else {
            return;
        };
        let Ok(path) = SourcePath::parse(path) else {
            return;
        };
        let target = Destination {
            path,
            at: Spot::Line(line.saturating_sub(1) as usize),
        };
        if let Ok(id) = code.reveal(cartridge, &target) {
            self.code.show_arrival(code, id, None);
        }
    }
}
