//! The IMPORT SPRITES dialog. Shows a PNG spritesheet converted to G1 as it
//! will land in the object-tile grid, tells what replacing the sprites with
//! it changes, and imports it when confirmed.
//!
//! The conversion is the one `glaux import sprites` runs; this dialog only
//! shows its result and hands it to the document as one edit.

use glaux_input::{InputEvent, Point, PointerButton};
use glaux_machine::font::BitmapFont;
use glaux_machine::framebuffer::Framebuffer;
use glaux_machine::obj_palette::OBJ_PALETTE_BANKS;
use glaux_machine::palette::Palette;
use glaux_machine::sprite::{
    OBJ_GRID_COLUMNS, OBJ_TILE_HEIGHT, OBJ_TILE_WIDTH, ObjTileAttributes, ObjTileBank, ObjVram,
};
use glaux_video::draw::{ObjPaletteChoice, draw_obj_tile_scaled, draw_text};
use glaux_video::preview::ObjPreviewPalette;

use super::color_editor::fit;
use super::layout::Metrics;
use super::render::{checker, outline};
use crate::sprite_import::{FitSummary, Fitted, SheetSource};
use crate::system::DIM;
use crate::system::sprite_document::{ReplaceImpact, SpriteDocument};
use crate::system::video::{CartridgeVideo, CartridgeVideoDefaults};
use crate::system::workspace::dialog::{
    AREA_MARGIN, BODY, INK, PAD, SHADOW_OFFSET, draw_button, draw_window, fill, text_cells as cells,
};
use crate::system::workspace::{Rect, SYSTEM_COLORS, wrap};

const TITLE: &str = "IMPORT SPRITES";
const CANCEL: &str = "CANCEL";
const IMPORT: &str = "IMPORT";
const TRANSPARENT_LABEL: &str = "Transparent:";
const ALPHA_ONLY: &str = "Alpha only";
const TOP_LEFT: &str = "Top-left color";
/// Widest the sheet is shown, the whole object-tile grid at 1x.
const PREVIEW_WIDTH: i32 = (OBJ_GRID_COLUMNS * OBJ_TILE_WIDTH) as i32;
/// Lines of the summary column above the transparency control.
const SUMMARY_LINES: i32 = 5;
/// Lines the warnings, or the reason the sheet cannot be imported, take.
pub const NOTE_LINES: usize = 7;
/// Space between the preview and the summary column.
const COLUMN_GAP: i32 = 8;
/// Space between the dialog's sections.
const SECTION_GAP: i32 = 5;
/// Space between the two buttons.
const BUTTON_GAP: i32 = 4;
/// Width of the preview's scroll indicator.
const SCROLLBAR: i32 = 2;
/// Square size of the checkerboard behind transparent pixels.
const CHECKER: i32 = 2;

/// What closing the dialog asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportOutcome {
    /// Replace every asset of the document with this.
    Import(Box<CartridgeVideo>),
    /// Leave the document as it is.
    Cancel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Button {
    Cancel,
    Import,
    Transparency,
}

/// A successful conversion and what it would change.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Converted {
    summary: FitSummary,
    video: CartridgeVideo,
    impact: ReplaceImpact,
    /// Where the frame palette holds the converted sheet's colors.
    preview: ObjPreviewPalette,
}

/// The open IMPORT SPRITES dialog for one decoded PNG.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportPreview {
    /// The PNG's file name, as the dialog shows it.
    name: String,
    source: SheetSource,
    defaults: CartridgeVideoDefaults,
    /// The palette the workspace's UI is drawn with, whose leading entries
    /// the preview's frame palette keeps.
    system_palette: Palette,
    /// Whether opaque pixels of the top-left pixel's color are transparent.
    keyed: bool,
    converted: Result<Box<Converted>, String>,
    /// The first sheet row shown.
    scroll: usize,
    /// The button a primary press started on. Releasing on the same button
    /// chooses it.
    armed: Option<Button>,
}

impl ImportPreview {
    /// Converts `source`, read from a file called `name`, for `document`
    /// with transparency from alpha alone. `defaults` holds the System ROM
    /// palettes the conversion starts from, as `glaux import sprites` does.
    pub fn new(
        name: String,
        source: SheetSource,
        document: &SpriteDocument,
        defaults: CartridgeVideoDefaults,
        system_palette: Palette,
    ) -> Self {
        let mut preview = Self {
            name,
            source,
            defaults,
            system_palette,
            keyed: false,
            converted: Err(String::new()),
            scroll: 0,
            armed: None,
        };
        preview.convert(document);
        preview
    }

    /// Whether the sheet converted, so IMPORT can be chosen.
    pub fn can_import(&self) -> bool {
        self.converted.is_ok()
    }

    /// Why the sheet cannot be imported as it is, if it cannot.
    #[cfg(test)]
    pub fn error(&self) -> Option<&str> {
        self.converted.as_ref().err().map(String::as_str)
    }

    #[cfg(test)]
    pub fn summary(&self) -> Option<&FitSummary> {
        self.converted.as_ref().ok().map(|c| &c.summary)
    }

    #[cfg(test)]
    pub fn is_keyed(&self) -> bool {
        self.keyed
    }

    /// The palette to present frames with while the dialog is open: the
    /// System ROM's UI colors followed by the converted sheet's. `None`
    /// when there is no conversion to show.
    pub fn frame_palette(&self) -> Option<&Palette> {
        self.converted.as_ref().ok().map(|c| c.preview.palette())
    }

    /// The color key the conversion uses: the top-left pixel's color when
    /// keyed and that pixel is opaque.
    fn key(&self) -> Option<glaux_machine::palette::Rgb> {
        self.keyed.then(|| self.source.image.top_left()).flatten()
    }

    /// Whether the transparency setting can be changed: only a sheet whose
    /// top-left pixel is opaque has a color to key out.
    fn can_key(&self) -> bool {
        self.source.image.top_left().is_some()
    }

    /// Runs the conversion again with the current settings, against
    /// `document` as it is now.
    fn convert(&mut self, document: &SpriteDocument) {
        self.scroll = 0;
        self.converted = self
            .source
            .fit(
                self.key(),
                &document.video().palette,
                self.defaults.palette(),
                self.defaults.obj_palettes(),
            )
            .map_err(|err| err.to_string())
            .and_then(|fitted| self.prepare(fitted, document))
            .map(Box::new);
    }

    fn prepare(&self, fitted: Fitted, document: &SpriteDocument) -> Result<Converted, String> {
        // The fit verified its own output through these decoders, so a
        // failure here is a broken invariant reported, not a panic.
        let internal = |err: &dyn std::fmt::Display| format!("internal error: {err}");
        let tiles = ObjTileBank::from_bytes(&fitted.tiles).map_err(|e| internal(&e))?;
        let attributes =
            ObjTileAttributes::from_bytes(&fitted.attributes).map_err(|e| internal(&e))?;
        let video = CartridgeVideo {
            palette: fitted.palette,
            obj: ObjVram::new(tiles, attributes, fitted.obj_palettes),
        };
        let preview = ObjPreviewPalette::new(
            &self.system_palette,
            SYSTEM_COLORS,
            &video.palette,
            video.obj.palettes(),
        )
        .map_err(|e| internal(&e))?;
        Ok(Converted {
            summary: fitted.summary,
            impact: document.replacement_impact(&video),
            video,
            preview,
        })
    }

    /// Applies `event`, returning how the dialog closes, if it does. Every
    /// event is taken, so nothing reaches the editor under it.
    ///
    /// Up, Down and the wheel scroll a sheet taller than the preview. `t`
    /// switches the transparency setting and converts again. Enter imports
    /// and Escape cancels; a press and release on the same button does
    /// what it says.
    pub fn handle_input(
        &mut self,
        event: InputEvent,
        layout: &ImportLayout,
        document: &SpriteDocument,
    ) -> Option<ImportOutcome> {
        match event {
            InputEvent::Up => self.scroll_by(-1, layout),
            InputEvent::Down => self.scroll_by(1, layout),
            InputEvent::Wheel { delta } => self.scroll_by(-i32::from(delta), layout),
            InputEvent::Text('t' | 'T') => self.toggle(document),
            InputEvent::Enter => return self.import(),
            InputEvent::Break => return Some(ImportOutcome::Cancel),
            InputEvent::PointerDown {
                button: PointerButton::Primary,
                at,
            } => self.armed = layout.button_at(at),
            InputEvent::PointerUp {
                button: PointerButton::Primary,
                at,
            } => {
                let armed = self.armed.take();
                if armed.is_some() && armed == layout.button_at(at) {
                    match armed? {
                        Button::Cancel => return Some(ImportOutcome::Cancel),
                        Button::Import => return self.import(),
                        Button::Transparency => self.toggle(document),
                    }
                }
            }
            InputEvent::PointerCancel => self.armed = None,
            _ => {}
        }
        None
    }

    fn import(&self) -> Option<ImportOutcome> {
        let converted = self.converted.as_ref().ok()?;
        Some(ImportOutcome::Import(Box::new(converted.video.clone())))
    }

    fn toggle(&mut self, document: &SpriteDocument) {
        if self.can_key() {
            self.keyed = !self.keyed;
            self.convert(document);
        }
    }

    /// Sheet rows the preview shows at once, and how large each tile is.
    fn view(&self, layout: &ImportLayout) -> (usize, i32) {
        let scale = match &self.converted {
            Ok(c) if c.summary.tiles_wide * OBJ_TILE_WIDTH <= PREVIEW_WIDTH as usize / 2 => 2,
            _ => 1,
        };
        let tile = OBJ_TILE_HEIGHT as i32 * scale;
        (
            usize::try_from(layout.preview.height / tile).unwrap_or(0),
            scale,
        )
    }

    fn rows(&self) -> usize {
        self.converted.as_ref().map_or(0, |c| c.summary.tiles_high)
    }

    fn scroll_by(&mut self, rows: i32, layout: &ImportLayout) {
        let (visible, _) = self.view(layout);
        let last = self.rows().saturating_sub(visible);
        let scroll = i64::try_from(self.scroll).unwrap_or(i64::MAX) + i64::from(rows);
        self.scroll = usize::try_from(scroll.max(0)).unwrap_or(0).min(last);
    }

    /// The summary column's lines.
    fn summary_lines(&self) -> Vec<String> {
        let image = &self.source.image;
        let mut lines = vec![
            self.name.clone(),
            format!("{}x{} px", image.width, image.height),
        ];
        if let Ok(converted) = &self.converted {
            let summary = &converted.summary;
            let new = summary.allocated.as_ref().map_or(0, |r| r.clone().count());
            lines[1] += &format!(", {} tiles", summary.source_tiles);
            lines.push(format!("{} colors, {new} new", summary.colors));
            lines.push(format!(
                "{} of {OBJ_PALETTE_BANKS} palettes",
                summary.banks_used
            ));
            lines.push(match self.source.groups.len() {
                0 => "No sprite groups".to_owned(),
                1 => "1 sprite group".to_owned(),
                n => format!("{n} sprite groups"),
            });
        }
        lines
    }

    /// The paragraphs under the preview: what importing changes, or why
    /// the sheet cannot be imported.
    pub fn notes(&self) -> Vec<String> {
        match &self.converted {
            Ok(converted) => impact_notes(&converted.impact),
            Err(message) => vec![format!("Cannot import: {message}")],
        }
    }

    /// Draws the dialog over the editor, with the converted sheet in the
    /// colors of [`frame_palette`](Self::frame_palette).
    pub fn render(&self, framebuffer: &mut Framebuffer, font: &BitmapFont, layout: &ImportLayout) {
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

        outline(framebuffer, layout.preview.inset(-1), INK);
        if let Ok(converted) = &self.converted {
            self.render_sheet(framebuffer, layout, converted);
        }

        let columns = layout.summary_columns();
        for (n, line) in self.summary_lines().iter().enumerate() {
            let y = layout.summary_y + n as i32 * cell_h;
            text(framebuffer, &fit(line, columns), layout.summary_x, y, INK);
        }
        text(
            framebuffer,
            TRANSPARENT_LABEL,
            layout.summary_x,
            layout.transparency.y - cell_h - 1,
            INK,
        );
        let mode = if self.keyed { TOP_LEFT } else { ALPHA_ONLY };
        if self.can_key() {
            draw_button(
                framebuffer,
                font,
                layout.transparency,
                mode,
                false,
                cell_w,
                cell_h,
            );
        } else {
            draw_disabled_button(framebuffer, font, layout.transparency, mode, cell_w, cell_h);
        }

        let mut y = layout.notes_y;
        let notes = self.notes();
        let lines = notes
            .iter()
            .flat_map(|note| wrap(note, layout.note_columns()))
            .take(NOTE_LINES);
        for line in lines {
            text(framebuffer, &line, layout.text_x, y, INK);
            y += cell_h;
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
        if self.can_import() {
            draw_button(
                framebuffer,
                font,
                layout.import,
                IMPORT,
                true,
                cell_w,
                cell_h,
            );
        } else {
            draw_disabled_button(framebuffer, font, layout.import, IMPORT, cell_w, cell_h);
        }
    }

    /// Draws the visible rows of the converted sheet as they land in the
    /// object-tile grid, over a checkerboard, with a scroll indicator when
    /// not every row fits.
    fn render_sheet(&self, framebuffer: &mut Framebuffer, layout: &ImportLayout, c: &Converted) {
        let (visible, scale) = self.view(layout);
        let tile = OBJ_TILE_WIDTH as i32 * scale;
        let shown = visible.min(c.summary.tiles_high.saturating_sub(self.scroll));
        let area = Rect {
            width: c.summary.tiles_wide as i32 * tile,
            height: shown as i32 * tile,
            ..layout.preview
        };
        checker(framebuffer, area, CHECKER);
        for row in 0..shown {
            for column in 0..c.summary.tiles_wide {
                draw_obj_tile_scaled(
                    framebuffer,
                    &c.video.obj,
                    (self.scroll + row) * OBJ_GRID_COLUMNS + column,
                    ObjPaletteChoice::TileDefault,
                    (
                        layout.preview.x + column as i32 * tile,
                        layout.preview.y + row as i32 * tile,
                    ),
                    scale,
                    c.preview.slots(),
                );
            }
        }
        let rows = c.summary.tiles_high;
        if rows > visible {
            let track = layout.scrollbar;
            let thumb = (track.height * visible as i32 / rows as i32).max(2);
            let top = (track.height - thumb) * self.scroll as i32 / (rows - visible).max(1) as i32;
            fill(
                framebuffer,
                Rect {
                    y: track.y + top,
                    height: thumb,
                    ..track
                },
                INK,
            );
        }
    }
}

/// What replacing the sprites changes, as the dialog tells it.
pub fn impact_notes(impact: &ReplaceImpact) -> Vec<String> {
    if !impact.changes_anything() {
        return vec!["The sheet matches the current sprites: nothing changes.".to_owned()];
    }
    let mut notes = Vec::new();
    notes.push(match impact.drawn_tiles_erased {
        0 => format!(
            "Replaces every sprite: {} tiles change.",
            impact.tiles_changed
        ),
        erased => format!(
            "Replaces every sprite: {} tiles change, {erased} drawn ones erased.",
            impact.tiles_changed
        ),
    });
    let masters = impact.master_range.map(|(low, high)| {
        if low == high {
            format!("{low}")
        } else {
            format!("{low}-{high}")
        }
    });
    match &masters {
        Some(range) => notes.push(format!(
            "{} palettes and master colors {range} change.",
            impact.banks_changed
        )),
        None if impact.banks_changed > 0 => {
            notes.push(format!("{} palettes change.", impact.banks_changed));
        }
        None => {}
    }
    notes.push(match &masters {
        Some(range) => {
            format!("Code that draws tiles, or colors {range}, by number will show the new ones.")
        }
        None => "Code that draws tiles by number will show the new ones.".to_owned(),
    });
    notes.push("Nothing is written until Save. Ctrl+Z undoes it.".to_owned());
    notes
}

/// Draws a button that cannot be chosen: a dim frame and label.
fn draw_disabled_button(
    framebuffer: &mut Framebuffer,
    font: &BitmapFont,
    rect: Rect,
    label: &str,
    cell_w: i32,
    cell_h: i32,
) {
    fill(framebuffer, rect, DIM);
    fill(framebuffer, rect.inset(1), BODY);
    let x = rect.x + (rect.width - cells(label) * cell_w) / 2;
    let y = rect.y + (rect.height - cell_h) / 2 + 1;
    draw_text(framebuffer, font, label, x, y, DIM);
}

/// Where the dialog's parts sit for one area and font. Rendering and
/// hit-testing both read it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportLayout {
    pub window: Rect,
    pub title_bar: Rect,
    /// The converted sheet, inside a one-pixel frame.
    pub preview: Rect,
    pub scrollbar: Rect,
    pub summary_x: i32,
    pub summary_y: i32,
    /// Switches what counts as transparent.
    pub transparency: Rect,
    /// Where the body text starts and must end.
    pub text_x: i32,
    pub text_right: i32,
    /// Top of the first note line.
    pub notes_y: i32,
    pub cancel: Rect,
    pub import: Rect,
    cell_w: i32,
    cell_h: i32,
}

impl ImportLayout {
    /// Lays the dialog out over nearly all of `area`, which it is centred
    /// in.
    pub fn new(area: Rect, metrics: Metrics) -> Self {
        let Metrics { cell_w, cell_h, .. } = metrics;
        let width = area.width - 2 * AREA_MARGIN - SHADOW_OFFSET;
        let height = area.height - SHADOW_OFFSET - 2;
        let window = Rect {
            x: area.x + (area.width - width - SHADOW_OFFSET) / 2,
            y: area.y + (area.height - height - SHADOW_OFFSET) / 2,
            width,
            height,
        };
        let title_bar = Rect {
            x: window.x + 1,
            y: window.y + 1,
            width: width - 2,
            height: cell_h + 2,
        };
        let text_x = window.x + 1 + PAD;
        let text_right = window.right() - 1 - PAD;

        let button_width = (cells(CANCEL).max(cells(IMPORT)) + 4) * cell_w;
        let button_height = cell_h + 4;
        let buttons_y = window.bottom() - 1 - PAD - button_height;
        let import = Rect {
            x: text_right - button_width,
            y: buttons_y,
            width: button_width,
            height: button_height,
        };
        let cancel = Rect {
            x: import.x - BUTTON_GAP - button_width,
            ..import
        };

        let notes_y = buttons_y - SECTION_GAP - NOTE_LINES as i32 * cell_h;
        let main_top = title_bar.bottom() + PAD;
        let main_bottom = notes_y - SECTION_GAP;
        // The preview shows whole tile rows inside its frame.
        let tile = OBJ_TILE_HEIGHT as i32;
        let preview_height = (main_bottom - main_top - 2) / tile * tile;
        let preview = Rect {
            x: text_x + 1,
            y: main_top + 1,
            width: PREVIEW_WIDTH,
            height: preview_height,
        };
        let scrollbar = Rect {
            x: preview.right() + 2,
            y: preview.y,
            width: SCROLLBAR,
            height: preview.height,
        };

        let summary_x = scrollbar.right() + COLUMN_GAP;
        let summary_y = main_top;
        let transparency = Rect {
            x: summary_x,
            y: summary_y + (SUMMARY_LINES + 1) * cell_h + 1,
            width: (cells(TOP_LEFT) + 2) * cell_w,
            height: button_height,
        };
        debug_assert!(
            window.x >= area.x
                && window.right() + SHADOW_OFFSET <= area.right()
                && window.bottom() + SHADOW_OFFSET <= area.bottom()
                && preview_height >= tile
                && transparency.bottom() <= main_bottom
                && transparency.right() <= text_right,
            "the import dialog must fit its area"
        );
        Self {
            window,
            title_bar,
            preview,
            scrollbar,
            summary_x,
            summary_y,
            transparency,
            text_x,
            text_right,
            notes_y,
            cancel,
            import,
            cell_w,
            cell_h,
        }
    }

    /// Characters a summary line may take.
    fn summary_columns(&self) -> usize {
        usize::try_from((self.text_right - self.summary_x) / self.cell_w.max(1)).unwrap_or(0)
    }

    /// Characters a note line may take.
    pub fn note_columns(&self) -> usize {
        usize::try_from((self.text_right - self.text_x) / self.cell_w.max(1)).unwrap_or(0)
    }

    fn button_at(&self, point: Point) -> Option<Button> {
        [
            (self.cancel, Button::Cancel),
            (self.import, Button::Import),
            (self.transparency, Button::Transparency),
        ]
        .into_iter()
        .find(|(rect, _)| rect.contains(point))
        .map(|(_, button)| button)
    }
}

#[cfg(test)]
mod tests;
