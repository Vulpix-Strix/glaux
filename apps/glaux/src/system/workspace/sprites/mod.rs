//! The sprite editor
//!
//! The editor only views the cartridge.

mod layout;
mod render;
mod sheet;

use glaux_input::{InputEvent, PointerButton};
use glaux_machine::obj_palette::OBJ_PALETTE_COLORS;
use glaux_machine::palette::Palette;
use glaux_machine::sprite::{OBJ_GRID_COLUMNS, OBJ_GRID_ROWS, ObjVram};
use glaux_video::preview::ObjPreviewPalette;

pub use layout::Metrics;
use layout::{Hit, SpriteLayout};
use sheet::{SheetView, grid_position};

/// The cartridge state the editor shows, borrowed for one frame.
#[derive(Debug, Clone, Copy)]
pub struct SpriteSource<'a> {
    pub obj: &'a ObjVram,
    /// The cartridge's master palette, for the colors Advanced mode names.
    pub master: &'a Palette,
    /// Where the frame palette holds each master color.
    pub preview: &'a ObjPreviewPalette,
}

/// The sprite editor's UI state. It holds no cartridge data, so it stays
/// valid across tab switches and trips to the shell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpriteEditor {
    metrics: Metrics,
    selected: usize,
    sheet: SheetView,
    grid: bool,
    advanced: bool,
    /// The selected local color, 0–15.
    color: u8,
}

impl SpriteEditor {
    /// An editor on tile 0 with the pixel grid shown, laid out below a
    /// bar ending at `metrics.top`.
    pub fn new(metrics: Metrics) -> Self {
        Self {
            metrics,
            selected: 0,
            sheet: SheetView::default(),
            grid: true,
            advanced: false,
            color: 1,
        }
    }

    #[cfg(test)]
    pub fn selected(&self) -> usize {
        self.selected
    }

    #[cfg(test)]
    pub fn is_advanced(&self) -> bool {
        self.advanced
    }

    /// Where the enlarged tile is drawn.
    #[cfg(test)]
    pub fn canvas_rect(&self) -> crate::system::workspace::Rect {
        self.layout().canvas
    }

    fn layout(&self) -> SpriteLayout {
        SpriteLayout::new(self.metrics, self.advanced, self.sheet.zoom())
    }

    /// Applies `event`. Nothing here changes the cartridge.
    ///
    /// A click selects a sheet tile, a palette color or an options button.
    /// The wheel scrolls the sheet's rows, and Shift with the wheel pans its
    /// columns. Arrow keys move the selection, `g` toggles the grid, `a`
    /// Advanced mode, and `+` and `-` zoom the sheet.
    pub fn handle_input(&mut self, event: InputEvent) {
        let layout = self.layout();
        match event {
            InputEvent::PointerDown {
                button: PointerButton::Primary,
                at,
            } => match layout.hit(at, &self.sheet) {
                // A tile under the pointer is on screen, so selecting it
                // never scrolls.
                Hit::Sheet(tile) => self.selected = tile,
                Hit::Swatch(local) => self.color = local,
                Hit::GridButton => self.grid = !self.grid,
                Hit::AdvancedButton => self.toggle_advanced(),
                Hit::ZoomOut => self.zoom(-1),
                Hit::ZoomIn => self.zoom(1),
                Hit::Canvas { .. } | Hit::Tool(_) | Hit::Bank(_) | Hit::None => {}
            },
            InputEvent::Wheel { delta } => {
                let rows = -i32::from(delta) * self.sheet.zoom().wheel_rows();
                self.sheet.scroll_rows(rows, layout.visible);
            }
            InputEvent::WheelHorizontal { delta } => {
                self.sheet.scroll_columns(-i32::from(delta), layout.visible);
            }
            InputEvent::Left { .. } => self.step(-1, 0),
            InputEvent::Right { .. } => self.step(1, 0),
            InputEvent::Up => self.step(0, -1),
            InputEvent::Down => self.step(0, 1),
            InputEvent::Text('g' | 'G') => self.grid = !self.grid,
            InputEvent::Text('a' | 'A') => self.toggle_advanced(),
            InputEvent::Text('+' | '=') => self.zoom(1),
            InputEvent::Text('-' | '_') => self.zoom(-1),
            _ => {}
        }
    }

    /// Moves the selection by whole grid cells, stopping at the grid's
    /// edges, and scrolls just enough to keep it on screen.
    fn step(&mut self, columns: i32, rows: i32) {
        let (column, row) = grid_position(self.selected);
        let column = (column as i32 + columns).clamp(0, OBJ_GRID_COLUMNS as i32 - 1) as usize;
        let row = (row as i32 + rows).clamp(0, OBJ_GRID_ROWS as i32 - 1) as usize;
        self.selected = row * OBJ_GRID_COLUMNS + column;
        self.sheet.reveal(self.selected, self.layout().visible);
    }

    /// Advanced mode changes the sheet's height; the sheet keeps its
    /// scroll position as far as the new height allows.
    fn toggle_advanced(&mut self) {
        self.advanced = !self.advanced;
        self.sheet.clamp(self.layout().visible);
    }

    fn zoom(&mut self, direction: i32) {
        let current = self.sheet.zoom();
        let next = if direction > 0 {
            current.zoomed_in()
        } else {
            current.zoomed_out()
        };
        if let Some(zoom) = next {
            let visible = SpriteLayout::new(self.metrics, self.advanced, zoom).visible;
            self.sheet.set_zoom(zoom, visible, self.selected);
        }
    }

    /// The selected local color, always below [`OBJ_PALETTE_COLORS`].
    fn color(&self) -> u8 {
        debug_assert!(usize::from(self.color) < OBJ_PALETTE_COLORS);
        self.color
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glaux_input::Point;
    use pretty_assertions::assert_eq;
    use sheet::Zoom;

    const METRICS: Metrics = Metrics {
        cell_w: 6,
        cell_h: 10,
        top: 16,
    };

    fn editor() -> SpriteEditor {
        SpriteEditor::new(METRICS)
    }

    fn click(editor: &mut SpriteEditor, rect: crate::system::workspace::Rect) {
        editor.handle_input(InputEvent::PointerDown {
            button: PointerButton::Primary,
            at: Point::new(
                (rect.x + rect.width / 2) as u16,
                (rect.y + rect.height / 2) as u16,
            ),
        });
    }

    #[test]
    fn clicking_a_visible_tile_selects_it_without_scrolling() {
        let mut editor = editor();
        editor.handle_input(InputEvent::Wheel { delta: -5 });
        let before = editor.sheet;
        let layout = editor.layout();
        click(&mut editor, layout.sheet_cell(4, 21, Zoom::X1));
        assert_eq!(editor.selected(), (10 + 21) * 16 + 4);
        assert_eq!(editor.sheet, before);
    }

    #[test]
    fn the_wheel_scrolls_rows_and_shift_wheel_pans_at_two_x() {
        let mut editor = editor();
        editor.handle_input(InputEvent::Wheel { delta: -3 });
        assert_eq!(editor.sheet.top_row(), 6);
        editor.handle_input(InputEvent::Wheel { delta: 1 });
        assert_eq!(editor.sheet.top_row(), 4);
        editor.handle_input(InputEvent::WheelHorizontal { delta: -2 });
        assert_eq!(editor.sheet.left_column(), 0, "1x shows every column");

        editor.handle_input(InputEvent::Text('+'));
        assert_eq!(editor.sheet.zoom(), Zoom::X2);
        editor.handle_input(InputEvent::WheelHorizontal { delta: -2 });
        assert_eq!(editor.sheet.left_column(), 2);
        editor.handle_input(InputEvent::WheelHorizontal { delta: -100 });
        assert_eq!(editor.sheet.left_column(), 5);
        editor.handle_input(InputEvent::Wheel { delta: i8::MIN });
        assert_eq!(editor.sheet.top_row(), 64 - 11);
    }

    #[test]
    fn arrows_move_the_selection_and_reveal_it_minimally() {
        let mut editor = editor();
        editor.handle_input(InputEvent::Left { select: false });
        editor.handle_input(InputEvent::Up);
        assert_eq!(
            editor.selected(),
            0,
            "the grid's corner stops the selection"
        );

        for _ in 0..22 {
            editor.handle_input(InputEvent::Down);
        }
        assert_eq!(editor.selected(), 22 * 16);
        assert_eq!(editor.sheet.top_row(), 1);

        for _ in 0..20 {
            editor.handle_input(InputEvent::Right { select: false });
        }
        assert_eq!(editor.selected(), 22 * 16 + 15);
        assert_eq!(editor.sheet.top_row(), 1);
    }

    #[test]
    fn zooming_keeps_the_selection_on_screen() {
        let mut editor = editor();
        let layout = editor.layout();
        click(&mut editor, layout.sheet_cell(15, 20, Zoom::X1));
        editor.handle_input(InputEvent::Text('+'));
        let visible = editor.layout().visible;
        assert!(editor.sheet.is_visible(editor.selected(), visible));
        editor.handle_input(InputEvent::Text('+'));
        assert_eq!(editor.sheet.zoom(), Zoom::X2, "2x is the largest zoom");
        editor.handle_input(InputEvent::Text('-'));
        editor.handle_input(InputEvent::Text('-'));
        assert_eq!(editor.sheet.zoom(), Zoom::X1);
    }

    #[test]
    fn advanced_mode_keeps_zoom_and_scroll() {
        let mut editor = editor();
        editor.handle_input(InputEvent::Text('+'));
        editor.handle_input(InputEvent::Wheel { delta: -10 });
        editor.handle_input(InputEvent::WheelHorizontal { delta: -3 });
        let before = editor.sheet;
        editor.handle_input(InputEvent::Text('a'));
        assert!(editor.is_advanced());
        assert_eq!(editor.sheet, before);
        editor.handle_input(InputEvent::Text('a'));
        assert_eq!(editor.sheet, before);
    }

    #[test]
    fn options_buttons_and_swatches_respond_to_clicks() {
        let mut editor = editor();
        let layout = editor.layout();
        click(&mut editor, layout.grid_button);
        assert!(!editor.grid);
        click(&mut editor, layout.zoom_in);
        assert_eq!(editor.sheet.zoom(), Zoom::X2);
        click(&mut editor, layout.swatches[9]);
        assert_eq!(editor.color(), 9);
        click(&mut editor, layout.advanced_button);
        assert!(editor.is_advanced());
    }

    #[test]
    fn canvas_and_tool_clicks_change_nothing() {
        let mut editor = editor();
        let before = editor.clone();
        let layout = editor.layout();
        click(&mut editor, layout.canvas);
        for (_, rect) in layout.tools {
            click(&mut editor, rect);
        }
        assert_eq!(editor, before);
    }
}
