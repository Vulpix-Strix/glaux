//! The SELECT PALETTE dialog chooses the object palette bank one tile is
//! drawn with. It shows every bank's colors and the tile drawn through each.

use glaux_input::{InputEvent, Point, PointerButton};
use glaux_machine::font::BitmapFont;
use glaux_machine::framebuffer::Framebuffer;
use glaux_machine::obj_palette::{OBJ_PALETTE_BANKS, OBJ_PALETTE_COLORS, ObjBank};
use glaux_machine::sprite::OBJ_TILE_WIDTH;
use glaux_video::draw::{ObjPaletteChoice, draw_obj_tile_scaled, draw_text};

use super::SpriteSource;
use super::layout::Metrics;
use super::render::{bank_name, checker, outline};
use crate::system::PALE;
use crate::system::workspace::Rect;
use crate::system::workspace::dialog::{
    BODY, INK, PAD, SHADOW_OFFSET, draw_button, draw_window, fill, text_cells as cells,
};

const TITLE: &str = "SELECT PALETTE";
const CANCEL: &str = "CANCEL";
const APPLY: &str = "APPLY";
/// Marks the bank the tile uses now.
const CURRENT_TAG: &str = "current";
/// Marks the pending row.
const MARKER: &str = ">";
/// Screen pixels per tile pixel in the previews.
const PREVIEW_SCALE: i32 = 2;
/// Side of a preview.
const PREVIEW: i32 = OBJ_TILE_WIDTH as i32 * PREVIEW_SCALE;
/// Height of a bank row: a preview with its one-pixel frame.
const ROW_HEIGHT: i32 = PREVIEW + 2;
/// A color swatch and the space before the next one.
const SWATCH_WIDTH: i32 = 8;
const SWATCH_PITCH: i32 = SWATCH_WIDTH + 1;
/// Swatch height. The row keeps three pixels above and below it.
const SWATCH_HEIGHT: i32 = ROW_HEIGHT - 6;
/// Visible colors per bank: every local but the transparent 0.
const SWATCHES: i32 = OBJ_PALETTE_COLORS as i32 - 1;
/// Space between the parts of a row, and at its ends.
const ROW_GAP: i32 = 6;
const ROW_PAD: i32 = 2;
/// Space between the header and the rows, and the rows and the buttons.
const SECTION_GAP: i32 = 5;
/// Space between the two buttons.
const BUTTON_GAP: i32 = 4;
/// Square size of the checkerboard behind a preview's transparent pixels.
const CHECKER: i32 = 2;

/// What closing the picker asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickerOutcome {
    /// Draw the tile with this bank from now on.
    Apply(ObjBank),
    /// Leave the tile as it is.
    Cancel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Button {
    Cancel,
    Apply,
}

/// The open SELECT PALETTE dialog for one tile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PalettePicker {
    tile: usize,
    /// The bank Apply assigns, chosen but not yet applied.
    pending: ObjBank,
    /// The button a primary press started on. Releasing on the same button
    /// chooses it.
    armed: Option<Button>,
}

impl PalettePicker {
    /// A picker for `tile` with `pending` chosen.
    pub fn new(tile: usize, pending: ObjBank) -> Self {
        Self {
            tile,
            pending,
            armed: None,
        }
    }

    pub fn tile(&self) -> usize {
        self.tile
    }

    #[cfg(test)]
    pub fn pending(&self) -> ObjBank {
        self.pending
    }

    /// Applies `event`, returning how the picker closes, if it does. Every
    /// event is taken, so nothing reaches the editor under it.
    ///
    /// Up and Down move the pending choice, wrapping around, and `0`–`7`
    /// choose a bank directly. A primary press on a row chooses it. Enter
    /// applies and Escape cancels. A press and release on the same button
    /// does what it says.
    pub fn handle_input(
        &mut self,
        event: InputEvent,
        layout: &PickerLayout,
    ) -> Option<PickerOutcome> {
        let banks = OBJ_PALETTE_BANKS as u8;
        let step = |bank: ObjBank, by: u8| ObjBank::new((bank.get() + by) % banks);
        match event {
            InputEvent::Up { .. } => self.pending = step(self.pending, banks - 1)?,
            InputEvent::Down { .. } => self.pending = step(self.pending, 1)?,
            InputEvent::Text(key) => {
                let bank = key.to_digit(10).and_then(|n| ObjBank::new(n as u8));
                if let Some(bank) = bank {
                    self.pending = bank;
                }
            }
            InputEvent::Enter => return Some(PickerOutcome::Apply(self.pending)),
            InputEvent::Break => return Some(PickerOutcome::Cancel),
            InputEvent::PointerDown {
                button: PointerButton::Primary,
                at,
                ..
            } => {
                self.armed = layout.button_at(at);
                if let Some(bank) = layout.row_at(at) {
                    self.pending = bank;
                }
            }
            InputEvent::PointerUp {
                button: PointerButton::Primary,
                at,
            } => {
                let armed = self.armed.take();
                if armed.is_some() && armed == layout.button_at(at) {
                    return armed.map(|button| match button {
                        Button::Cancel => PickerOutcome::Cancel,
                        Button::Apply => PickerOutcome::Apply(self.pending),
                    });
                }
            }
            InputEvent::PointerCancel => self.armed = None,
            _ => {}
        }
        None
    }

    /// Draws the dialog over the editor from `source`, with the row under
    /// `pointer` highlighted.
    pub fn render(
        &self,
        framebuffer: &mut Framebuffer,
        font: &BitmapFont,
        layout: &PickerLayout,
        source: &SpriteSource<'_>,
        pointer: Option<Point>,
    ) {
        let (cell_w, cell_h) = (layout.cell_w, layout.cell_h);
        let text = |fb: &mut Framebuffer, text: &str, x: i32, y: i32, color: u8| {
            draw_text(fb, font, text, x, y, color);
        };
        draw_window(
            framebuffer,
            font,
            layout.window,
            layout.title_bar,
            TITLE,
            cell_w,
        );

        let current = source.obj.attributes().bank(self.tile);
        text(
            framebuffer,
            &format!("Tile {:04}", self.tile),
            layout.text_x,
            layout.header_y,
            INK,
        );
        let current_text = format!("Current: {}", bank_name(current, layout.advanced));
        let current_x = layout.text_right - cells(&current_text) * cell_w;
        text(framebuffer, &current_text, current_x, layout.header_y, INK);

        let hovered = pointer.and_then(|at| layout.row_at(at));
        let slots = source.preview.slots();
        let row_text_y = (ROW_HEIGHT - cell_h) / 2 + 1;
        for (number, &row) in layout.rows.iter().enumerate() {
            let Some(bank) = ObjBank::new(number as u8) else {
                continue;
            };
            let pending = bank == self.pending;
            let (band, ink) = if pending {
                (INK, BODY)
            } else if hovered == Some(bank) {
                (PALE, INK)
            } else {
                (BODY, INK)
            };
            fill(framebuffer, row, band);
            let y = row.y + row_text_y;
            if pending {
                text(framebuffer, MARKER, layout.marker_x, y, ink);
            }
            text(
                framebuffer,
                &bank_name(bank, layout.advanced),
                layout.label_x,
                y,
                ink,
            );

            outline(framebuffer, layout.swatch_strip(row).inset(-1), ink);
            for local in 1..OBJ_PALETTE_COLORS as u8 {
                let swatch = layout.swatch(row, local);
                match source.obj.palettes().resolve(bank, local) {
                    Some(master) => fill(framebuffer, swatch, slots[usize::from(master)]),
                    None => checker(framebuffer, swatch, CHECKER),
                }
            }

            let preview = layout.preview(row);
            outline(framebuffer, preview.inset(-1), ink);
            checker(framebuffer, preview, CHECKER);
            draw_obj_tile_scaled(
                framebuffer,
                source.obj,
                self.tile,
                ObjPaletteChoice::Bank(bank),
                (preview.x, preview.y),
                PREVIEW_SCALE,
                slots,
            );

            if bank == current {
                text(framebuffer, CURRENT_TAG, layout.tag_x, y, ink);
            }
        }

        draw_button(
            framebuffer,
            font,
            layout.cancel,
            CANCEL,
            false,
            cell_w,
            cell_h,
        );
        draw_button(framebuffer, font, layout.apply, APPLY, true, cell_w, cell_h);
    }
}

/// Where the picker's parts sit for one area, font and mode. Rendering and
/// hit-testing both read it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PickerLayout {
    pub window: Rect,
    pub title_bar: Rect,
    /// Top of the header line.
    pub header_y: i32,
    /// Where text starts and must end inside the window.
    pub text_x: i32,
    pub text_right: i32,
    /// One row per bank, in bank order.
    pub rows: [Rect; OBJ_PALETTE_BANKS],
    pub marker_x: i32,
    pub label_x: i32,
    pub swatch_x: i32,
    pub preview_x: i32,
    pub tag_x: i32,
    pub cancel: Rect,
    pub apply: Rect,
    advanced: bool,
    cell_w: i32,
    cell_h: i32,
}

impl PickerLayout {
    /// Lays the picker out centred in `area`, with bank labels for Advanced
    /// mode when `advanced`.
    pub fn new(area: Rect, metrics: Metrics, advanced: bool) -> Self {
        let Metrics { cell_w, cell_h, .. } = metrics;
        let label_cells = cells(&bank_name(ObjBank::ZERO, advanced));
        let swatches_width = SWATCHES * SWATCH_PITCH - 1;
        let row_width = ROW_PAD
            + cells(MARKER) * cell_w
            + ROW_GAP / 2
            + label_cells * cell_w
            + ROW_GAP
            + swatches_width
            + ROW_GAP
            + PREVIEW
            + 2
            + ROW_GAP
            + cells(CURRENT_TAG) * cell_w
            + ROW_PAD;
        let button_width = (cells(CANCEL) + 4) * cell_w;
        let button_height = cell_h + 4;

        let title_height = cell_h + 2;
        let rows_height = OBJ_PALETTE_BANKS as i32 * ROW_HEIGHT;
        let body_height =
            PAD + cell_h + SECTION_GAP + rows_height + SECTION_GAP + button_height + PAD;
        let width = row_width + 2 * PAD + 2;
        let height = 1 + title_height + body_height + 1;
        // The system font fits the area, as the layout tests check. A taller
        // one would only push the window's bottom past it, never its title
        // above it.
        let window = Rect {
            x: (area.x + (area.width - width - SHADOW_OFFSET) / 2).max(area.x),
            y: (area.y + (area.height - height - SHADOW_OFFSET) / 2).max(area.y),
            width,
            height,
        };
        let title_bar = Rect {
            x: window.x + 1,
            y: window.y + 1,
            width: width - 2,
            height: title_height,
        };
        let text_x = window.x + 1 + PAD;
        let text_right = text_x + row_width;
        let header_y = title_bar.bottom() + PAD;
        let rows_top = header_y + cell_h + SECTION_GAP;
        let rows = std::array::from_fn(|n| Rect {
            x: text_x,
            y: rows_top + n as i32 * ROW_HEIGHT,
            width: row_width,
            height: ROW_HEIGHT,
        });

        let marker_x = text_x + ROW_PAD;
        let label_x = marker_x + cells(MARKER) * cell_w + ROW_GAP / 2;
        let swatch_x = label_x + label_cells * cell_w + ROW_GAP;
        let preview_x = swatch_x + swatches_width + ROW_GAP + 1;
        let tag_x = preview_x + PREVIEW + 1 + ROW_GAP;

        let buttons_y = rows_top + rows_height + SECTION_GAP;
        let apply = Rect {
            x: text_right - button_width,
            y: buttons_y,
            width: button_width,
            height: button_height,
        };
        let cancel = Rect {
            x: apply.x - BUTTON_GAP - button_width,
            ..apply
        };
        Self {
            window,
            title_bar,
            header_y,
            text_x,
            text_right,
            rows,
            marker_x,
            label_x,
            swatch_x,
            preview_x,
            tag_x,
            cancel,
            apply,
            advanced,
            cell_w,
            cell_h,
        }
    }

    /// The swatch of local color `local` (1–15) in `row`.
    pub fn swatch(&self, row: Rect, local: u8) -> Rect {
        Rect {
            x: self.swatch_x + (i32::from(local) - 1) * SWATCH_PITCH,
            y: row.y + (ROW_HEIGHT - SWATCH_HEIGHT) / 2,
            width: SWATCH_WIDTH,
            height: SWATCH_HEIGHT,
        }
    }

    /// All of `row`'s swatches together.
    fn swatch_strip(&self, row: Rect) -> Rect {
        Rect {
            width: SWATCHES * SWATCH_PITCH - 1,
            ..self.swatch(row, 1)
        }
    }

    /// The tile preview of `row`, inside its one-pixel frame.
    pub fn preview(&self, row: Rect) -> Rect {
        Rect {
            x: self.preview_x,
            y: row.y + 1,
            width: PREVIEW,
            height: PREVIEW,
        }
    }

    /// The bank whose row is under `point`.
    pub fn row_at(&self, point: Point) -> Option<ObjBank> {
        let row = self.rows.iter().position(|row| row.contains(point))?;
        ObjBank::new(row as u8)
    }

    fn button_at(&self, point: Point) -> Option<Button> {
        if self.cancel.contains(point) {
            Some(Button::Cancel)
        } else if self.apply.contains(point) {
            Some(Button::Apply)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests;
