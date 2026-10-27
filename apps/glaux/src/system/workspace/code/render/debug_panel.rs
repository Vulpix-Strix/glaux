//! Paints the panel of a stopped cartridge and the plates that replace the
//! run plates while it waits.

use glaux_input::Point;
use glaux_script::debug::Frame;
use glaux_video::draw::{fill_rect, pixel};

use super::{ACCENT, BACKGROUND, DIM, ERROR, OK, PALE, Painter, TEXT, WARN};
use crate::system::workspace::code::debugging::{DebugLayout, DebugView, Row};
use crate::system::workspace::code::symbols::{PAD, clip_name};
use crate::system::workspace::{BAR, HOVER_SHADE, IDLE_SHADE, Rect};
use crate::ui_text;

impl Painter<'_> {
    pub(super) fn debug_panel(
        &mut self,
        view: &DebugView,
        layout: &DebugLayout,
        pointer: Option<Point>,
        grip_lit: bool,
    ) {
        let area = layout.area;
        self.fill(area, BACKGROUND);
        self.fill(
            Rect { width: 1, ..area },
            if grip_lit { ACCENT } else { BAR },
        );
        self.panel_title(&ui_text::get("code-debug-locals"), layout.locals_title);
        self.variables(view, layout, pointer);
        self.panel_title(&ui_text::get("code-debug-stack"), layout.stack_title);
        self.frames(view, layout, pointer);
    }

    fn panel_title(&mut self, title: &str, rect: Rect) {
        let y = self.bar_text_y(rect.y, rect.height - 1);
        self.text(title, rect.x + PAD, y, PALE);
        self.fill(
            Rect {
                y: rect.bottom() - 1,
                height: 1,
                ..rect
            },
            BAR,
        );
    }

    fn row_ground(&mut self, row: Rect, selected: bool, lit: bool) {
        let ground = if selected {
            BAR
        } else if lit {
            IDLE_SHADE
        } else {
            return;
        };
        self.fill(
            Rect {
                x: row.x + 1,
                width: row.width - 1,
                ..row
            },
            ground,
        );
    }

    fn variables(&mut self, view: &DebugView, layout: &DebugLayout, pointer: Option<Point>) {
        let rows = view.rows();
        if rows.is_empty() {
            let empty = ui_text::get("code-debug-no-variables");
            self.text(&empty, layout.area.x + PAD, layout.first_local.y + 2, DIM);
            return;
        }
        for (n, row) in rows
            .iter()
            .skip(view.scroll())
            .take(layout.visible_locals)
            .enumerate()
        {
            let rect = layout.local_row(n);
            let lit = pointer.is_some_and(|at| rect.contains(at));
            self.row_ground(rect, false, lit);
            self.variable(row, rect);
        }
    }

    fn variable(&mut self, row: &Row, rect: Rect) {
        let cell_w = self.cell_w().max(1);
        let y = rect.y + (rect.height - self.cell_h()) / 2;
        let x = rect.x + PAD + row.depth as i32 * cell_w;
        let cells = ((rect.right() - PAD - x) / cell_w).max(0) as usize;
        let value_cells = self.layout.cells(&row.value) as usize;
        // A long name cannot push the value below a third of the row.
        let value_room = value_cells.min(cells.saturating_sub(4)).max(cells / 3);
        let value = clip_name(self.font, &row.value, value_room);
        let value_x = rect.right() - PAD - self.layout.cells(&value) * cell_w;
        let marker = match (row.expandable, row.open) {
            (false, _) => " ",
            (true, false) => "+",
            (true, true) => "-",
        };
        let name_room = ((value_x - x) / cell_w - 2).max(0) as usize;
        let name = clip_name(self.font, &row.name, name_room);
        self.text(marker, x, y, DIM);
        self.text(&name, x + cell_w, y, TEXT);
        self.text(&value, value_x, y, PALE);
    }

    fn frames(&mut self, view: &DebugView, layout: &DebugLayout, pointer: Option<Point>) {
        for (n, frame) in view.frames().iter().take(layout.visible_frames).enumerate() {
            let rect = layout.frame_row(n);
            let selected = n == view.selected();
            let lit = pointer.is_some_and(|at| rect.contains(at));
            self.row_ground(rect, selected, lit);
            self.frame(frame, selected, rect);
        }
    }

    /// Frames without a script, such as the VM's own, show as the engine.
    fn frame(&mut self, frame: &Frame, selected: bool, rect: Rect) {
        let cell_w = self.cell_w().max(1);
        let y = rect.y + (rect.height - self.cell_h()) / 2;
        let x = rect.x + PAD;
        let (name, color) = if frame.path.is_some() {
            (frame.function.as_str(), if selected { TEXT } else { PALE })
        } else {
            ("", DIM)
        };
        let engine;
        let name = if name.is_empty() {
            engine = ui_text::get("code-debug-engine");
            engine.as_str()
        } else {
            name
        };
        let line = frame
            .line
            .map(|line| format!(":{line}"))
            .unwrap_or_default();
        let line_x = rect.right() - PAD - self.layout.cells(&line) * cell_w;
        let room = ((line_x - x) / cell_w - 2).max(0) as usize;
        let name = clip_name(self.font, name, room);
        if selected {
            self.text(">", x, y, super::ACCENT);
        }
        self.text(&name, x + 2 * cell_w, y, color);
        self.text(&line, line_x, y, DIM);
    }

    /// The plates follow the order of the layout's step buttons.
    pub(super) fn step_buttons(&mut self, pointer: Option<Point>) {
        let plates = self.layout.step_buttons();
        for (n, plate) in plates.into_iter().enumerate() {
            let lit = pointer.is_some_and(|at| plate.contains(at));
            self.fill(plate, if lit { HOVER_SHADE } else { BAR });
            let (x, y) = (plate.x, plate.y);
            match n {
                0 => self.play(plate),
                1 => self.arrow(x, y, Arrow::Right),
                2 => self.arrow(x, y, Arrow::Down),
                3 => self.arrow(x, y, Arrow::Up),
                _ => fill_rect(self.framebuffer, x + 2, y + 2, 5, 5, ERROR),
            }
        }
    }

    pub(super) fn play(&mut self, plate: Rect) {
        for column in 0..4 {
            fill_rect(
                self.framebuffer,
                plate.x + 3 + column,
                plate.y + 1 + column,
                1,
                7 - 2 * column,
                OK,
            );
        }
    }

    pub(super) fn bug(&mut self, plate: Rect, lit: bool) {
        self.fill(plate, if lit { HOVER_SHADE } else { BAR });
        let (x, y) = (plate.x, plate.y);
        fill_rect(self.framebuffer, x + 3, y + 3, 3, 4, WARN);
        fill_rect(self.framebuffer, x + 4, y + 2, 1, 1, WARN);
        for (dx, dy) in [(2, 3), (6, 3), (2, 5), (6, 5), (2, 7), (6, 7)] {
            pixel(self.framebuffer, x + dx, y + dy, WARN);
        }
    }

    fn arrow(&mut self, x: i32, y: i32, direction: Arrow) {
        let fb = &mut *self.framebuffer;
        match direction {
            Arrow::Right => {
                fill_rect(fb, x + 1, y + 4, 6, 1, PALE);
                fill_rect(fb, x + 5, y + 2, 1, 5, PALE);
                fill_rect(fb, x + 6, y + 3, 1, 3, PALE);
            }
            Arrow::Down => {
                fill_rect(fb, x + 4, y + 1, 1, 6, PALE);
                fill_rect(fb, x + 2, y + 5, 5, 1, PALE);
                fill_rect(fb, x + 3, y + 6, 3, 1, PALE);
            }
            Arrow::Up => {
                fill_rect(fb, x + 4, y + 2, 1, 6, PALE);
                fill_rect(fb, x + 2, y + 3, 5, 1, PALE);
                fill_rect(fb, x + 3, y + 2, 3, 1, PALE);
            }
        }
    }
}

enum Arrow {
    Right,
    Down,
    Up,
}
