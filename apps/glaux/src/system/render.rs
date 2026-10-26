//! Drawing the system.

use crate::system::running::RunningCartridge;
use crate::system::screen::{BACKGROUND, text_columns, text_rows};
use crate::system::{PausedView, System, SystemState};
use glaux_input::Point;
use glaux_machine::framebuffer::Framebuffer;
use glaux_machine::palette::Palette;
use glaux_machine::system_rom::{SystemCursor, SystemRom};
use glaux_video::draw::{clear, draw_text, fill_rect};

impl System {
    /// Draws the system's mouse pointer with its hotspot on `pointer`, the
    /// logical pixel under the host's pointer, in the shape the workspace
    /// picks for what is under it. Only the workspace, the one
    /// pointer-driven state, shows it.
    ///
    /// Call on a copy of the frame drawn by [`render`](Self::render), once
    /// per presented frame. The pointer then follows the host at its
    /// refresh rate rather than the tick rate, and never ends up in the
    /// machine's framebuffer.
    pub fn draw_cursor(
        &self,
        framebuffer: &mut Framebuffer,
        rom: &SystemRom,
        pointer: Option<Point>,
    ) {
        if !matches!(
            self.state,
            SystemState::Workspace { .. } | SystemState::Docs { .. }
        ) {
            return;
        }
        let (cursor, frame) = match (pointer, &self.unsaved) {
            (Some(at), Some(flow)) => {
                let layout = self.workspace.dialog_layout(flow.dialog(), false);
                match layout.button_at(at) {
                    Some(_) => (SystemCursor::Hand, 0),
                    None => (SystemCursor::Arrow, 0),
                }
            }
            (Some(at), None) if matches!(self.state, SystemState::Docs { .. }) => {
                (self.workspace.docs_apart_cursor(at), 0)
            }
            (Some(at), None) => self.workspace.cursor(at, self.code.as_ref()),
            (None, _) => (SystemCursor::Arrow, 0),
        };
        crate::system::workspace::draw_cursor(framebuffer, rom.ui().cursor(cursor), frame, pointer);
    }

    /// The master palette the frame drawn by [`render`](Self::render)
    /// resolves to: the cartridge's own while its frame is on screen, the
    /// System ROM palette with the cartridge's object colors after the UI
    /// colors in the workspace, and the System ROM palette otherwise.
    pub fn frame_palette<'a>(&'a self, rom: &'a SystemRom) -> &'a Palette {
        match &self.state {
            SystemState::Workspace { video, .. } => video.frame_palette().unwrap_or(rom.palette()),
            SystemState::Running { running, .. }
            | SystemState::Paused {
                running,
                view: PausedView::Game(_),
                ..
            } => running.palette(),
            SystemState::Boot { .. }
            | SystemState::Shell { .. }
            | SystemState::Credits { .. }
            | SystemState::Docs { .. }
            | SystemState::Paused {
                view: PausedView::Shell,
                ..
            } => rom.palette(),
        }
    }

    /// Redraws the whole framebuffer for the current state using the ROM's
    /// font and UI images, with the question about unsaved edits on top
    /// while it is open. In the workspace, `pointer` is the logical pixel
    /// under the host's pointer, used to highlight what it hovers.
    pub fn render(&self, framebuffer: &mut Framebuffer, rom: &SystemRom, pointer: Option<Point>) {
        let Some(flow) = &self.unsaved else {
            self.render_state(framebuffer, rom, pointer);
            return;
        };
        // Nothing under the dialog reacts to the pointer.
        self.render_state(framebuffer, rom, None);
        let whole_screen = !matches!(self.state, SystemState::Workspace { .. });
        let layout = self.workspace.dialog_layout(flow.dialog(), whole_screen);
        flow.dialog().render(framebuffer, rom.font(), &layout);
    }

    /// Draws the log panel over the frame, if it is open. Over a running
    /// cartridge the panel's colors are resolved to the cartridge's palette,
    /// which the frame is presented with.
    fn render_log_panel(
        &self,
        framebuffer: &mut Framebuffer,
        rom: &SystemRom,
        running: Option<&RunningCartridge>,
    ) {
        if !self.log_panel.is_open() {
            return;
        }
        let colors: [u8; 32] = std::array::from_fn(|n| match running {
            Some(running) => {
                let color = rom.palette().get(n as u8);
                glaux_video::shade::nearest_index(running.palette(), color)
            }
            None => n as u8,
        });
        self.log_panel.render(
            framebuffer,
            rom.font(),
            &|color| colors[usize::from(color)],
            // Only the editor can show a script, and only a run that has
            // ended leaves a located record.
            running.is_none(),
        );
    }

    fn render_state(&self, framebuffer: &mut Framebuffer, rom: &SystemRom, pointer: Option<Point>) {
        let font = rom.font();
        let metrics = font.metrics();
        let (cell_w, cell_h) = (
            i32::from(metrics.cell_width()),
            i32::from(metrics.cell_height()),
        );

        let lines = match &self.state {
            SystemState::Boot { boot, .. }
                if boot.stage() == crate::system::boot::BootStage::Intro =>
            {
                crate::system::boot::render_intro(framebuffer, font, rom.version(), boot.frame());
                return;
            }
            SystemState::Boot { boot, .. } => crate::system::boot::lines(boot.stage(), rom),
            SystemState::Shell { shell, .. }
            | SystemState::Paused {
                shell,
                view: PausedView::Shell,
                ..
            } => shell.lines(font, text_columns(cell_w), text_rows(cell_h)),
            SystemState::Workspace {
                cartridge, video, ..
            } => {
                self.workspace.render(
                    framebuffer,
                    font,
                    rom.ui(),
                    cartridge,
                    video
                        .source(self.sprites.as_ref())
                        .as_ref()
                        .map(|source| *source)
                        .map_err(String::as_str),
                    self.code.as_ref(),
                    pointer,
                );
                self.render_log_panel(framebuffer, rom, None);
                return;
            }
            SystemState::Credits { view, .. } => {
                view.render(framebuffer, rom);
                return;
            }
            SystemState::Docs { .. } => {
                self.workspace.render_docs_apart(framebuffer, pointer);
                return;
            }
            SystemState::Running { running, .. } => {
                running.render(framebuffer, font);
                self.render_log_panel(framebuffer, rom, Some(running));
                return;
            }
            SystemState::Paused {
                running,
                view: PausedView::Game(overlay),
                ..
            } => {
                running.render(framebuffer, font);
                overlay.render(framebuffer, running.palette(), rom.palette());
                overlay.render_controls(framebuffer, font, running.palette(), rom.palette());
                return;
            }
        };

        clear(framebuffer, BACKGROUND);
        for line in &lines {
            let (x, y) = line.origin(cell_w, cell_h);
            if let Some(background) = line.background {
                let cells = i32::try_from(font.text_cells(&line.text)).unwrap_or(i32::MAX);
                fill_rect(
                    framebuffer,
                    x,
                    y,
                    cells.saturating_mul(cell_w),
                    cell_h,
                    background,
                );
            }
            draw_text(framebuffer, font, &line.text, x, y, line.color);
        }
    }
}
