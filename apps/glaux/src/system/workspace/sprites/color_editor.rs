//! The EDIT COLOR dialog. Changes one visible color of a tile's palette by
//! eye, with a saturation/value field and a hue strip, or exactly, as hex
//! or RGB, previewing the tile with the proposed color before anything is
//! applied.

pub mod hsv;

use glaux_input::{InputEvent, Point, PointerButton};
use glaux_machine::font::BitmapFont;
use glaux_machine::framebuffer::Framebuffer;
use glaux_machine::palette::{PALETTE_LEN, Rgb};
use glaux_machine::sprite::OBJ_TILE_WIDTH;
use glaux_video::draw::{ObjPaletteChoice, draw_obj_tile_scaled, draw_text, fill_rect};

use super::SpriteSource;
use super::layout::Metrics;
use super::render::{checker, contrasting, outline};
use crate::system::ERROR;
use crate::system::sprite_document::{
    CODE_COLORS, ColorScope, ColorTarget, ColorUses, Spare, spare_master,
};
use crate::system::workspace::Rect;
use crate::system::workspace::dialog::{
    AREA_MARGIN, BODY, INK, PAD, SHADOW_OFFSET, draw_button, draw_window, fill, text_cells as cells,
};
use hsv::{HUE_STEPS, Hsv, format_hex, parse_hex};

const TITLE: &str = "EDIT COLOR";
const CANCEL: &str = "CANCEL";
/// The confirm button while only the edited color uses its master.
const APPLY: &str = "APPLY";
/// The confirm button while other palette colors share the master and
/// change with it.
const APPLY_TO_ALL: &str = "APPLY TO ALL";
const PREVIEW_LABEL: &str = "PREVIEW";
const HUE_LABEL: &str = "HUE";
const HEX_LABEL: &str = "HEX";
/// Columns and rows of the saturation/value field's color cells.
const SV_COLUMNS: usize = 8;
const SV_ROWS: usize = 8;
const SV_CELL_W: i32 = 20;
const SV_CELL_H: i32 = 10;
const SV_W: i32 = SV_COLUMNS as i32 * SV_CELL_W;
const SV_H: i32 = SV_ROWS as i32 * SV_CELL_H;
/// Color cells along the hue strip.
const HUE_CELLS: usize = 32;
const HUE_CELL_W: i32 = 6;
const HUE_W: i32 = HUE_CELLS as i32 * HUE_CELL_W;
const HUE_H: i32 = 8;
/// Space above and below the hue strip for its marker.
const HUE_MARGIN: i32 = 3;
/// Screen pixels per tile pixel in the preview.
const PREVIEW_SCALE: i32 = 6;
const PREVIEW: i32 = OBJ_TILE_WIDTH as i32 * PREVIEW_SCALE;
/// Height of the old and new color swatches under the preview.
const SWATCH_HEIGHT: i32 = 20;
/// Space between the field and the preview column.
const COLUMN_GAP: i32 = 16;
/// Space between the dialog's sections.
const SECTION_GAP: i32 = 5;
/// Space between the RGB fields and between a label and its field.
const FIELD_GAP: i32 = 6;
const BUTTON_GAP: i32 = 4;
/// Lines the impact note under the fields may take.
pub const IMPACT_LINES: usize = 3;
/// Side of the ring marking the picked saturation and value.
const CURSOR: i32 = 5;
/// Square size of the checkerboard behind the preview's transparent pixels.
const CHECKER: i32 = 3;
/// Keyboard steps: saturation or value per arrow press, and hue per arrow
/// press, one hue cell. Shift with Left or Right takes four.
const SV_STEP: u8 = 8;
const HUE_STEP: u16 = HUE_STEPS / HUE_CELLS as u16;
const SHIFT_STEPS: u8 = 4;
/// Side of the scope rows' radio marker.
const RADIO: i32 = 5;

/// Frame palette entries the dialog draws its gradients and new color
/// with: the field's cells, then the hue strip's, then the new color.
pub const SCRATCH_COLORS: usize = SV_COLUMNS * SV_ROWS + HUE_CELLS + 1;
/// The first of those entries. The workspace keeps them at the end of the
/// frame palette, clear of the UI colors and the cartridge's.
pub const SCRATCH_BASE: usize = PALETTE_LEN - SCRATCH_COLORS;
const HUE_BASE: usize = SCRATCH_BASE + SV_COLUMNS * SV_ROWS;
// The last entry, below PALETTE_LEN, so it fits in u8.
const NEW_SLOT: u8 = (PALETTE_LEN - 1) as u8;

/// What closing the dialog asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorOutcome {
    /// Make the edited color `color`, as far as `scope` reaches.
    Apply { color: Rgb, scope: ColorScope },
    /// Leave the document as it is.
    Cancel,
}

/// The parts of the dialog that take the keyboard, in Tab order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Control {
    SatVal,
    Hue,
    Field(Field),
    /// The choice between changing every palette or only this one.
    Scope,
    Cancel,
    Apply,
}

impl Control {
    const ORDER: [Self; 9] = [
        Self::SatVal,
        Self::Hue,
        Self::Field(Field::Hex),
        Self::Field(Field::Red),
        Self::Field(Field::Green),
        Self::Field(Field::Blue),
        Self::Scope,
        Self::Cancel,
        Self::Apply,
    ];

    /// The control after this one, skipping [`Scope`](Self::Scope) unless
    /// the dialog offers a choice of scope.
    fn next(self, scoped: bool) -> Self {
        let at = Self::ORDER.iter().position(|&c| c == self).unwrap_or(0);
        let next = Self::ORDER[(at + 1) % Self::ORDER.len()];
        if next == Self::Scope && !scoped {
            next.next(scoped)
        } else {
            next
        }
    }
}

/// A text field for typing the color exactly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Hex,
    Red,
    Green,
    Blue,
}

impl Field {
    const ALL: [Self; 4] = [Self::Hex, Self::Red, Self::Green, Self::Blue];

    /// Characters the field holds at most.
    fn capacity(self) -> usize {
        match self {
            Self::Hex => 7,
            _ => 3,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Hex => HEX_LABEL,
            Self::Red => "R",
            Self::Green => "G",
            Self::Blue => "B",
        }
    }

    /// How the field shows `color`.
    fn format(self, color: Rgb) -> String {
        match self {
            Self::Hex => format_hex(color),
            Self::Red => color.r.to_string(),
            Self::Green => color.g.to_string(),
            Self::Blue => color.b.to_string(),
        }
    }

    /// The color `text` makes of `color` in this field, if it is valid.
    fn parse(self, text: &str, color: Rgb) -> Option<Rgb> {
        match self {
            Self::Hex => parse_hex(text),
            Self::Red => text.parse().ok().map(|r| Rgb { r, ..color }),
            Self::Green => text.parse().ok().map(|g| Rgb { g, ..color }),
            Self::Blue => text.parse().ok().map(|b| Rgb { b, ..color }),
        }
    }

    /// Whether `c` may follow `text` in the field.
    fn accepts(self, text: &str, c: char) -> bool {
        if text.chars().count() >= self.capacity() {
            return false;
        }
        match self {
            Self::Hex if c == '#' => text.is_empty(),
            Self::Hex => {
                c.is_ascii_hexdigit() && text.chars().filter(char::is_ascii_hexdigit).count() < 6
            }
            _ => c.is_ascii_digit(),
        }
    }

    /// The channel Up and Down step, if the field holds one.
    fn step(self, color: Rgb, up: bool) -> Rgb {
        let step = |value: u8| {
            if up {
                value.saturating_add(1)
            } else {
                value.saturating_sub(1)
            }
        };
        match self {
            Self::Hex => color,
            Self::Red => Rgb {
                r: step(color.r),
                ..color
            },
            Self::Green => Rgb {
                g: step(color.g),
                ..color
            },
            Self::Blue => Rgb {
                b: step(color.b),
                ..color
            },
        }
    }
}

/// What the user typed into the focused field and has not yet confirmed.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Typing {
    text: String,
    /// The color before the typing began, which Escape restores.
    before: Rgb,
}

/// A picker area the pointer is dragging across.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Drag {
    SatVal,
    Hue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Button {
    Cancel,
    Apply,
}

/// The open EDIT COLOR dialog for one color of one tile's palette.
///
/// Nothing it does touches the document: the color it proposes only
/// reaches the document through [`ColorOutcome::Apply`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColorEditor {
    tile: usize,
    target: ColorTarget,
    uses: ColorUses,
    /// The color when the dialog opened.
    old: Rgb,
    /// The proposed color, exactly as Apply gives it.
    rgb: Rgb,
    /// Where the pickers' markers sit. It keeps hue and saturation through
    /// colors that leave them undefined, which `rgb` cannot.
    hsv: Hsv,
    /// The chosen scope, `None` when there is no choice to make because no
    /// other palette shows the master; applying then changes the master.
    scope: Option<ColorScope>,
    focus: Control,
    /// Text typed into the focused field, while unconfirmed.
    typing: Option<Typing>,
    drag: Option<Drag>,
    /// The button a primary press started on. Releasing on the same button
    /// chooses it.
    armed: Option<Button>,
}

impl ColorEditor {
    /// A dialog editing `target`, the color `old`, as seen on `tile`, with
    /// `uses` saying what else shows its master.
    ///
    /// When another palette shows the master, the dialog offers to change
    /// only the target's palette, and starts with that choice.
    pub fn new(tile: usize, target: ColorTarget, uses: ColorUses, old: Rgb) -> Self {
        let scope = (!uses.other_banks(target.bank).is_empty()).then_some(ColorScope::Palette);
        Self {
            tile,
            target,
            uses,
            old,
            rgb: old,
            hsv: Hsv::from_rgb(old, Hsv { h: 0, s: 0, v: 0 }),
            scope,
            focus: Control::SatVal,
            typing: None,
            drag: None,
            armed: None,
        }
    }

    pub fn target(&self) -> ColorTarget {
        self.target
    }

    #[cfg(test)]
    pub fn color(&self) -> Rgb {
        self.rgb
    }

    /// The chosen scope, if the dialog offers a choice.
    #[cfg(test)]
    pub fn scope(&self) -> Option<ColorScope> {
        self.scope
    }

    /// The scope applying uses.
    fn applied_scope(&self) -> ColorScope {
        self.scope.unwrap_or(ColorScope::All)
    }

    fn apply(&self) -> ColorOutcome {
        ColorOutcome::Apply {
            color: self.rgb,
            scope: self.applied_scope(),
        }
    }

    /// Switches between the two scopes, when there is a choice.
    fn toggle_scope(&mut self) {
        self.scope = self.scope.map(|scope| match scope {
            ColorScope::All => ColorScope::Palette,
            ColorScope::Palette => ColorScope::All,
        });
    }

    #[cfg(test)]
    pub fn focus(&self) -> Control {
        self.focus
    }

    #[cfg(test)]
    pub fn hsv(&self) -> Hsv {
        self.hsv
    }

    /// The text `field` shows: what is being typed into it, or the color.
    pub fn field_text(&self, field: Field) -> String {
        match &self.typing {
            Some(typing) if self.focus == Control::Field(field) => typing.text.clone(),
            _ => field.format(self.rgb),
        }
    }

    /// The confirm button's label: it says when other colors change too.
    pub fn apply_label(&self) -> &'static str {
        if self.uses.is_shared() && self.applied_scope() == ColorScope::All {
            APPLY_TO_ALL
        } else {
            APPLY
        }
    }

    /// The colors the dialog draws with, for frame palette entries
    /// [`SCRATCH_BASE`] onward.
    pub fn scratch_colors(&self) -> [Rgb; SCRATCH_COLORS] {
        let centre = |cell: usize, cells: usize| ((2 * cell + 1) * 255 / (2 * cells)) as u8;
        std::array::from_fn(|n| {
            if n < SV_COLUMNS * SV_ROWS {
                let (column, row) = (n % SV_COLUMNS, n / SV_COLUMNS);
                Hsv {
                    h: self.hsv.h,
                    s: centre(column, SV_COLUMNS),
                    v: 255 - centre(row, SV_ROWS),
                }
                .to_rgb()
            } else if n < SCRATCH_COLORS - 1 {
                let cell = n - SV_COLUMNS * SV_ROWS;
                Hsv {
                    // Below HUE_STEPS, so it fits in u16.
                    h: ((2 * cell + 1) * usize::from(HUE_STEPS) / (2 * HUE_CELLS)) as u16,
                    s: 255,
                    v: 255,
                }
                .to_rgb()
            } else {
                self.rgb
            }
        })
    }

    /// Applies `event`, returning how the dialog closes, if it does. Every
    /// event is taken, so nothing reaches the editor under it.
    ///
    /// Pressing the field or the hue strip picks there, and dragging keeps
    /// picking, clamped to its edges, until the release. Tab moves the
    /// focus. Arrows step the focused picker, Up and Down the focused RGB
    /// channel, and any arrow switches the focused scope choice; a press on
    /// a scope row chooses it. Typing into a field changes the color as soon as the
    /// text is valid. Enter confirms the text being typed, or else chooses
    /// the focused button, APPLY unless CANCEL is focused. Escape restores
    /// the color the typing began from, or else cancels.
    pub fn handle_input(
        &mut self,
        event: InputEvent,
        layout: &ColorEditorLayout,
    ) -> Option<ColorOutcome> {
        match event {
            InputEvent::Tab => self.focus_on(self.focus.next(self.scope.is_some())),
            InputEvent::Enter => {
                if self.typing.take().is_some() {
                    return None;
                }
                return Some(match self.focus {
                    Control::Cancel => ColorOutcome::Cancel,
                    _ => self.apply(),
                });
            }
            InputEvent::Break => match self.typing.take() {
                Some(typing) => self.set_rgb(typing.before),
                None => return Some(ColorOutcome::Cancel),
            },
            InputEvent::Text(c) => self.type_char(c),
            InputEvent::Backspace => {
                if let Control::Field(field) = self.focus {
                    let rgb = self.rgb;
                    let typing = self.typing.get_or_insert_with(|| Typing {
                        text: String::new(),
                        before: rgb,
                    });
                    typing.text.pop();
                    self.sync(field);
                }
            }
            InputEvent::Left { select } => self.step_across(false, select),
            InputEvent::Right { select } => self.step_across(true, select),
            InputEvent::Up => self.step_along(true),
            InputEvent::Down => self.step_along(false),
            InputEvent::PointerDown {
                button: PointerButton::Primary,
                at,
            } => self.press(at, layout),
            InputEvent::PointerMove(Some(at)) => self.drag_to(at, layout),
            InputEvent::PointerUp {
                button: PointerButton::Primary,
                at,
            } => {
                if self.drag.is_some() {
                    self.drag_to(at, layout);
                    self.drag = None;
                    return None;
                }
                let armed = self.armed.take();
                if armed.is_some() && armed == layout.button_at(at) {
                    return armed.map(|button| match button {
                        Button::Cancel => ColorOutcome::Cancel,
                        Button::Apply => self.apply(),
                    });
                }
            }
            InputEvent::PointerCancel => {
                self.drag = None;
                self.armed = None;
            }
            _ => {}
        }
        None
    }

    /// Focuses `control`, dropping unconfirmed typing: the color keeps what
    /// was valid of it.
    fn focus_on(&mut self, control: Control) {
        self.focus = control;
        self.typing = None;
    }

    fn set_rgb(&mut self, rgb: Rgb) {
        self.rgb = rgb;
        self.hsv = Hsv::from_rgb(rgb, self.hsv);
    }

    fn set_hsv(&mut self, hsv: Hsv) {
        self.hsv = hsv;
        self.rgb = hsv.to_rgb();
    }

    fn type_char(&mut self, c: char) {
        let Control::Field(field) = self.focus else {
            return;
        };
        let rgb = self.rgb;
        // Typing into a field that shows its value replaces the value.
        let typing = self.typing.get_or_insert_with(|| Typing {
            text: String::new(),
            before: rgb,
        });
        if field.accepts(&typing.text, c) {
            typing.text.push(c.to_ascii_uppercase());
            self.sync(field);
        }
    }

    /// Takes the color from `field`'s text while that is valid.
    fn sync(&mut self, field: Field) {
        let parsed = self
            .typing
            .as_ref()
            .and_then(|typing| field.parse(&typing.text, self.rgb));
        if let Some(rgb) = parsed {
            self.set_rgb(rgb);
        }
    }

    /// Left or Right: saturation in the field, hue on the strip, and the
    /// other button between the buttons.
    fn step_across(&mut self, forward: bool, shift: bool) {
        let times = if shift { SHIFT_STEPS } else { 1 };
        match self.focus {
            Control::SatVal => {
                let by = SV_STEP * times;
                let s = if forward {
                    self.hsv.s.saturating_add(by)
                } else {
                    self.hsv.s.saturating_sub(by)
                };
                self.set_hsv(Hsv { s, ..self.hsv });
            }
            Control::Hue => {
                let by = HUE_STEP * u16::from(times);
                let h = if forward {
                    self.hsv.h.saturating_add(by).min(HUE_STEPS - 1)
                } else {
                    self.hsv.h.saturating_sub(by)
                };
                self.set_hsv(Hsv { h, ..self.hsv });
            }
            Control::Cancel => self.focus = Control::Apply,
            Control::Apply => self.focus = Control::Cancel,
            Control::Scope => self.toggle_scope(),
            Control::Field(_) => {}
        }
    }

    /// Up or Down: value in the field, or the focused RGB channel.
    fn step_along(&mut self, up: bool) {
        match self.focus {
            Control::SatVal => {
                let v = if up {
                    self.hsv.v.saturating_add(SV_STEP)
                } else {
                    self.hsv.v.saturating_sub(SV_STEP)
                };
                self.set_hsv(Hsv { v, ..self.hsv });
            }
            Control::Field(field) => {
                self.typing = None;
                self.set_rgb(field.step(self.rgb, up));
            }
            Control::Scope => self.toggle_scope(),
            Control::Hue | Control::Cancel | Control::Apply => {}
        }
    }

    fn press(&mut self, at: Point, layout: &ColorEditorLayout) {
        if layout.sat_val.contains(at) {
            self.focus_on(Control::SatVal);
            self.drag = Some(Drag::SatVal);
            self.drag_to(at, layout);
        } else if layout.hue_target().contains(at) {
            self.focus_on(Control::Hue);
            self.drag = Some(Drag::Hue);
            self.drag_to(at, layout);
        } else if let Some(field) = layout.field_at(at) {
            self.focus_on(Control::Field(field));
        } else if let Some(scope) = layout.scope_at(at).filter(|_| self.scope.is_some()) {
            self.scope = Some(scope);
            self.focus_on(Control::Scope);
        } else if let Some(button) = layout.button_at(at) {
            self.armed = Some(button);
            self.focus_on(match button {
                Button::Cancel => Control::Cancel,
                Button::Apply => Control::Apply,
            });
        }
    }

    /// Picks at `at` on the area being dragged, clamped to its edges.
    fn drag_to(&mut self, at: Point, layout: &ColorEditorLayout) {
        let (x, y) = (i32::from(at.x), i32::from(at.y));
        match self.drag {
            Some(Drag::SatVal) => {
                let area = layout.sat_val;
                let s = scale(x - area.x, area.width - 1, 255);
                let v = 255 - scale(y - area.y, area.height - 1, 255);
                // Both are within 0..=255.
                self.set_hsv(Hsv {
                    s: s as u8,
                    v: v as u8,
                    ..self.hsv
                });
            }
            Some(Drag::Hue) => {
                let strip = layout.hue;
                let h = scale(x - strip.x, strip.width - 1, i32::from(HUE_STEPS) - 1);
                // Within 0..HUE_STEPS.
                self.set_hsv(Hsv {
                    h: h as u16,
                    ..self.hsv
                });
            }
            None => {}
        }
    }

    /// Draws the dialog over the editor from `source`.
    pub fn render(
        &self,
        framebuffer: &mut Framebuffer,
        font: &BitmapFont,
        layout: &ColorEditorLayout,
        source: &SpriteSource<'_>,
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

        let (heading, tile) = header(self.target, &self.uses, self.tile, layout.advanced);
        text(framebuffer, &heading, layout.text_x, layout.header_y, INK);
        let tile_x = layout.text_right - cells(&tile) * cell_w;
        text(framebuffer, &tile, tile_x, layout.header_y, INK);

        // The saturation/value field and its marker.
        let area = layout.sat_val;
        self.frame(framebuffer, area, Control::SatVal);
        for row in 0..SV_ROWS {
            for column in 0..SV_COLUMNS {
                let cell = Rect {
                    x: area.x + column as i32 * SV_CELL_W,
                    y: area.y + row as i32 * SV_CELL_H,
                    width: SV_CELL_W,
                    height: SV_CELL_H,
                };
                // Below PALETTE_LEN, so it fits in u8.
                fill(
                    framebuffer,
                    cell,
                    (SCRATCH_BASE + row * SV_COLUMNS + column) as u8,
                );
            }
        }
        let (cursor_x, cursor_y) = layout.sat_val_point(self.hsv);
        outline(
            framebuffer,
            Rect {
                x: cursor_x - CURSOR / 2,
                y: cursor_y - CURSOR / 2,
                width: CURSOR,
                height: CURSOR,
            },
            contrasting(self.rgb),
        );

        // The tile with the proposed color, then the old and new colors.
        let label_x = layout.preview.x + (PREVIEW - cells(PREVIEW_LABEL) * cell_w) / 2;
        text(
            framebuffer,
            PREVIEW_LABEL,
            label_x,
            layout.preview_label_y,
            INK,
        );
        outline(framebuffer, layout.preview.inset(-1), INK);
        checker(framebuffer, layout.preview, CHECKER);
        let mut slots = *source.preview.slots();
        slots[usize::from(self.uses.master)] = NEW_SLOT;
        draw_obj_tile_scaled(
            framebuffer,
            source.obj,
            self.tile,
            ObjPaletteChoice::Bank(self.target.bank),
            (layout.preview.x, layout.preview.y),
            PREVIEW_SCALE,
            &slots,
        );
        let old_slot = source.preview.slots()[usize::from(self.uses.master)];
        fill(framebuffer, layout.swatches, INK);
        for (rect, slot, color, label) in [
            (layout.old, old_slot, self.old, "OLD"),
            (layout.new, NEW_SLOT, self.rgb, "NEW"),
        ] {
            fill(framebuffer, rect, slot);
            let x = rect.x + (rect.width - cells(label) * cell_w) / 2;
            let y = rect.y + (rect.height - cell_h) / 2 + 1;
            text(framebuffer, label, x, y, contrasting(color));
        }

        // The hue strip and its marker.
        text(
            framebuffer,
            HUE_LABEL,
            layout.text_x,
            layout.hue_label_y,
            INK,
        );
        let strip = layout.hue;
        self.frame(framebuffer, strip, Control::Hue);
        for cell in 0..HUE_CELLS {
            let rect = Rect {
                x: strip.x + cell as i32 * HUE_CELL_W,
                width: HUE_CELL_W,
                ..strip
            };
            // Below PALETTE_LEN, so it fits in u8.
            fill(framebuffer, rect, (HUE_BASE + cell) as u8);
        }
        let marker_x = layout.hue_x(self.hsv.h);
        fill_rect(
            framebuffer,
            marker_x - 1,
            strip.y - HUE_MARGIN,
            3,
            HUE_H + 2 * HUE_MARGIN,
            INK,
        );
        fill_rect(
            framebuffer,
            marker_x,
            strip.y - HUE_MARGIN + 1,
            1,
            HUE_H + 2 * HUE_MARGIN - 2,
            BODY,
        );

        // The fields.
        for (field, rect) in Field::ALL.into_iter().zip(layout.fields) {
            let label_x = rect.x - FIELD_GAP / 2 - cells(field.label()) * cell_w;
            let label_x = if field == Field::Hex {
                layout.text_x
            } else {
                label_x
            };
            let text_y = rect.y + (rect.height - cell_h) / 2 + 1;
            text(framebuffer, field.label(), label_x, text_y, INK);
            let focused = self.focus == Control::Field(field);
            let typing = focused && self.typing.is_some();
            let value = self.field_text(field);
            fill(framebuffer, rect, INK);
            // A focused field shows its value selected until typing
            // replaces it.
            let (face, ink) = if focused && !typing {
                (INK, BODY)
            } else {
                (BODY, INK)
            };
            fill(framebuffer, rect.inset(1), face);
            let ink = if typing && field.parse(&value, self.rgb).is_none() {
                ERROR
            } else {
                ink
            };
            let shown = if typing { format!("{value}_") } else { value };
            text(framebuffer, &shown, rect.x + 3, text_y, ink);
        }

        match self.scope {
            None => {
                for (line, y) in impact_lines(self.target, &self.uses, layout.advanced)
                    .iter()
                    .zip(layout.impact_ys())
                {
                    let line = fit(line, layout.columns());
                    text(framebuffer, &line, layout.text_x, y, INK);
                }
            }
            Some(scope) => {
                let spare = spare_master(
                    source.master,
                    source.obj.palettes(),
                    self.uses.master,
                    self.rgb,
                );
                let lines = scope_lines(self.target, &self.uses, scope, spare, layout.advanced);
                let indent = 2 * cell_w;
                let row_columns = layout.columns().saturating_sub(2);
                for ((row, line), row_scope) in layout
                    .scope_rows()
                    .into_iter()
                    .zip(&lines)
                    .zip([ColorScope::All, ColorScope::Palette])
                {
                    let chosen = row_scope == scope;
                    let ink = if chosen && self.focus == Control::Scope {
                        fill(framebuffer, row, INK);
                        BODY
                    } else {
                        INK
                    };
                    let marker = Rect {
                        x: row.x + 1,
                        y: row.y + (row.height - RADIO) / 2,
                        width: RADIO,
                        height: RADIO,
                    };
                    outline(framebuffer, marker, ink);
                    if chosen {
                        fill(framebuffer, marker.inset(1), ink);
                    }
                    let line = fit(line, row_columns);
                    text(framebuffer, &line, row.x + indent, row.y, ink);
                }
                if let Some(y) = layout.impact_ys().nth(2) {
                    let line = fit(&lines[2], layout.columns());
                    text(framebuffer, &line, layout.text_x, y, INK);
                }
            }
        }

        draw_button(
            framebuffer,
            font,
            layout.cancel,
            CANCEL,
            self.focus == Control::Cancel,
            cell_w,
            cell_h,
        );
        draw_button(
            framebuffer,
            font,
            layout.apply,
            self.apply_label(),
            self.focus != Control::Cancel,
            cell_w,
            cell_h,
        );
    }

    /// Frames a picker area, twice while it has the focus.
    fn frame(&self, framebuffer: &mut Framebuffer, area: Rect, control: Control) {
        outline(framebuffer, area.inset(-1), INK);
        if self.focus == control {
            outline(framebuffer, area.inset(-2), INK);
        }
    }
}

/// `offset` along a span of `span + 1` pixels, clamped to it, as a value
/// in `0..=max`, rounded.
fn scale(offset: i32, span: i32, max: i32) -> i32 {
    let span = span.max(1);
    (offset.clamp(0, span) * max + span / 2) / span
}

/// The header's left and right text: the edited color, and the tile it
/// is seen on.
fn header(target: ColorTarget, uses: &ColorUses, tile: usize, advanced: bool) -> (String, String) {
    let ColorTarget { bank, local } = target;
    let heading = if advanced {
        let master = uses.master;
        format!(
            "OBP{} L{local:X}  Master ${master:02X} ({master})",
            bank.get()
        )
    } else {
        format!("Palette {}, color {local:X}", bank.get())
    };
    (heading, format!("Tile {tile:04}"))
}

/// `count` followed by `one` or `many`.
fn counted(count: usize, one: &str, many: &str) -> String {
    format!("{count} {}", if count == 1 { one } else { many })
}

/// What applying would change beyond the edited color, as far as the
/// document can tell, and what it cannot tell. Each count keeps its own
/// unit: other palettes (banks), palette colors (bank entries) and tiles
/// are never mixed. Empty lines stay blank.
pub fn impact_lines(
    target: ColorTarget,
    uses: &ColorUses,
    advanced: bool,
) -> [String; IMPACT_LINES] {
    let master = uses.master;
    let entries = uses.entries.len();
    let banks = uses.other_banks(target.bank);
    let tiles = other_tiles(uses.tiles);
    let scope = if uses.is_shared() || master < CODE_COLORS {
        code_note(master, advanced)
    } else {
        String::new()
    };

    if !uses.is_shared() {
        // The count only sees stored banks; code may draw more tiles with
        // this palette, so no count is shown as "only this tile".
        let first = if uses.tiles == 0 {
            "Changes it in every tile that uses it.".to_owned()
        } else {
            format!("Also shows in {tiles}.")
        };
        let second = if advanced {
            format!("Master ${master:02X} ({master}) is not shared.")
        } else {
            String::new()
        };
        return [first, second, scope];
    }

    if advanced {
        let in_banks = if banks.is_empty() {
            "this bank".to_owned()
        } else {
            counted(banks.len(), "bank", "banks")
        };
        let first = format!(
            "! Master ${master:02X}: {}, {in_banks}",
            counted(entries, "more entry", "more entries")
        );
        let list: Vec<String> = uses
            .entries
            .iter()
            .map(|entry| format!("OBP{} L{:X}", entry.bank.get(), entry.local))
            .collect();
        let second = format!("  {}", list.join(", "));
        return [first, second, advanced_tiles_note(uses)];
    }

    let colors = counted(entries, "palette color", "palette colors");
    let (first, second) = if banks.is_empty() {
        (
            format!(
                "! Shared with {} of",
                counted(entries, "other color", "other colors")
            ),
            format!("  this palette; {tiles}."),
        )
    } else {
        (
            format!(
                "! Shared: also in {}",
                counted(banks.len(), "other palette", "other palettes")
            ),
            format!("  ({colors}), {tiles}."),
        )
    };
    [first, second, scope]
}

/// What the scope rows say, the first for [`ColorScope::All`] and the
/// second for [`ColorScope::Palette`], and what the chosen `scope` means
/// beyond them, given `spare`, where [`ColorScope::Palette`] would put the
/// color.
pub fn scope_lines(
    target: ColorTarget,
    uses: &ColorUses,
    scope: ColorScope,
    spare: Spare,
    advanced: bool,
) -> [String; IMPACT_LINES] {
    let master = uses.master;
    let bank = target.bank.get();
    let all = if advanced {
        format!(
            "All: master ${master:02X}, {}",
            counted(uses.entries.len(), "more entry", "more entries")
        )
    } else {
        format!(
            "All {} palettes, {}",
            uses.other_banks(target.bank).len() + 1,
            other_tiles(uses.tiles)
        )
    };
    let index = spare.master();
    let only = if advanced {
        format!("OBP{bank} only: master ${index:02X} ({index})")
    } else {
        format!("Only Palette {bank}, {}", other_tiles(uses.palette_tiles))
    };
    // A spare no palette maps may still be drawn by number from code.
    let consequence = match (scope, spare, advanced) {
        (ColorScope::All, _, false) => code_note(master, false),
        (ColorScope::All, _, true) => advanced_tiles_note(uses),
        (ColorScope::Palette, Spare::Reused(_), false) => {
            "Other palettes keep the old color.".to_owned()
        }
        (ColorScope::Palette, Spare::Reused(_), true) => {
            format!("${index:02X} already has this color.")
        }
        (ColorScope::Palette, Spare::Unmapped(_), false) => {
            "Code using that color number changes.".to_owned()
        }
        (ColorScope::Palette, Spare::Unmapped(_), true) => {
            format!("Code drawing {index} changes too.")
        }
    };
    [all, only, consequence]
}

/// `count` other tiles, in words.
fn other_tiles(count: usize) -> String {
    match count {
        0 => "no other tiles".to_owned(),
        n => counted(n, "other tile", "other tiles"),
    }
}

/// What changing `master` itself means beyond the document: code may draw
/// any master index directly, and every frame starts cleared to master 0;
/// the document sees neither.
fn code_note(master: u8, advanced: bool) -> String {
    match (advanced, master) {
        (false, 0) => "Game background; code may use it too.".to_owned(),
        (false, _) => "Game code may use it too.".to_owned(),
        (true, 0) => "Background and code using 0 change too.".to_owned(),
        (true, _) => format!("Code drawing color {master} changes too."),
    }
}

/// Advanced mode's note on the other tiles and the code a shared master
/// reaches.
fn advanced_tiles_note(uses: &ColorUses) -> String {
    let tiles = other_tiles(uses.tiles);
    match (uses.tiles, uses.master) {
        (0, master) => code_note(master, true),
        (_, 0) => format!("{}; code, background too.", capitalized(&tiles)),
        (_, master) => format!("{}; code using {master} too.", capitalized(&tiles)),
    }
}

fn capitalized(text: &str) -> String {
    let mut chars = text.chars();
    chars
        .next()
        .map(|c| c.to_ascii_uppercase().to_string() + chars.as_str())
        .unwrap_or_default()
}

/// `line` cut to `columns` characters, ending in `..` when cut.
pub(super) fn fit(line: &str, columns: usize) -> String {
    if line.chars().count() <= columns {
        return line.to_owned();
    }
    let kept = columns.saturating_sub(2);
    line.chars().take(kept).chain("..".chars()).collect()
}

/// Where the dialog's parts sit for one area, font and mode. Rendering and
/// hit-testing both read it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColorEditorLayout {
    pub window: Rect,
    pub title_bar: Rect,
    pub header_y: i32,
    pub text_x: i32,
    pub text_right: i32,
    /// The saturation/value field, inside its frame.
    pub sat_val: Rect,
    pub preview_label_y: i32,
    /// The tile preview, inside its frame.
    pub preview: Rect,
    /// The old and new swatches with the frame around them.
    pub swatches: Rect,
    pub old: Rect,
    pub new: Rect,
    pub hue_label_y: i32,
    /// The hue strip, inside its frame.
    pub hue: Rect,
    /// HEX, R, G and B, in [`Field::ALL`] order.
    pub fields: [Rect; 4],
    /// Top of the impact note's first line.
    pub impact_y: i32,
    pub cancel: Rect,
    pub apply: Rect,
    advanced: bool,
    cell_w: i32,
    cell_h: i32,
}

impl ColorEditorLayout {
    /// Lays the dialog out centred in `area`, with Advanced mode's details
    /// when `advanced`.
    pub fn new(area: Rect, metrics: Metrics, advanced: bool) -> Self {
        let Metrics { cell_w, cell_h, .. } = metrics;
        let column_width = PREVIEW + 2;
        let inner_width = SV_W + 2 + COLUMN_GAP + column_width;
        let field_height = cell_h + 2;
        let button_height = cell_h + 4;
        let hue_block = 2 * HUE_MARGIN + HUE_H;
        let impact_height = IMPACT_LINES as i32 * cell_h;

        let title_height = cell_h + 2;
        let body_height = PAD
            + cell_h
            + SECTION_GAP
            + SV_H
            + 2
            + SECTION_GAP
            + hue_block
            + SECTION_GAP
            + field_height
            + SECTION_GAP
            + impact_height
            + SECTION_GAP
            + button_height
            + PAD;
        let width = inner_width + 2 * PAD + 2;
        let height = 1 + title_height + body_height + 1;
        let window = Rect {
            x: area.x + (area.width - width - SHADOW_OFFSET) / 2,
            y: area.y + (area.height - height - SHADOW_OFFSET) / 2,
            width,
            height,
        };
        debug_assert!(
            width + SHADOW_OFFSET <= area.width - 2 * AREA_MARGIN
                && height + SHADOW_OFFSET <= area.height,
            "the color editor must fit its area"
        );
        let title_bar = Rect {
            x: window.x + 1,
            y: window.y + 1,
            width: width - 2,
            height: title_height,
        };
        let text_x = window.x + 1 + PAD;
        let text_right = text_x + inner_width;
        let header_y = title_bar.bottom() + PAD;

        let top = header_y + cell_h + SECTION_GAP;
        let sat_val = Rect {
            x: text_x + 1,
            y: top + 1,
            width: SV_W,
            height: SV_H,
        };
        let column_x = text_right - column_width;
        let preview_label_y = top;
        let preview = Rect {
            x: column_x + 1,
            y: top + cell_h + 2,
            width: PREVIEW,
            height: PREVIEW,
        };
        let swatches = Rect {
            x: column_x,
            y: top + SV_H + 2 - SWATCH_HEIGHT,
            width: column_width,
            height: SWATCH_HEIGHT,
        };
        let old_width = (column_width - 3) / 2 + 1;
        let old = Rect {
            x: swatches.x + 1,
            y: swatches.y + 1,
            width: old_width,
            height: SWATCH_HEIGHT - 2,
        };
        let new = Rect {
            x: old.right() + 1,
            width: swatches.right() - 1 - (old.right() + 1),
            ..old
        };

        let hue_top = top + SV_H + 2 + SECTION_GAP;
        let hue = Rect {
            x: text_right - 1 - HUE_W,
            y: hue_top + HUE_MARGIN,
            width: HUE_W,
            height: HUE_H,
        };
        let hue_label_y = hue.y + (HUE_H - cell_h) / 2 + 1;

        let fields_y = hue_top + hue_block + SECTION_GAP;
        let field = |x: i32, cells: i32| Rect {
            x,
            y: fields_y,
            width: cells * cell_w + 6,
            height: field_height,
        };
        let hex = field(hue.x - 1, Field::Hex.capacity() as i32);
        let channel_width = field(0, 3).width;
        let mut right = text_right;
        let mut channel = || {
            right -= channel_width;
            let rect = field(right, 3);
            right -= cell_w + FIELD_GAP;
            rect
        };
        let blue = channel();
        let green = channel();
        let red = channel();

        let impact_y = fields_y + field_height + SECTION_GAP;
        let buttons_y = impact_y + impact_height + SECTION_GAP;
        let apply_width = (cells(APPLY_TO_ALL) + 4) * cell_w;
        let cancel_width = (cells(CANCEL) + 4) * cell_w;
        let apply = Rect {
            x: text_right - apply_width,
            y: buttons_y,
            width: apply_width,
            height: button_height,
        };
        let cancel = Rect {
            x: apply.x - BUTTON_GAP - cancel_width,
            width: cancel_width,
            ..apply
        };
        Self {
            window,
            title_bar,
            header_y,
            text_x,
            text_right,
            sat_val,
            preview_label_y,
            preview,
            swatches,
            old,
            new,
            hue_label_y,
            hue,
            fields: [hex, red, green, blue],
            impact_y,
            cancel,
            apply,
            advanced,
            cell_w,
            cell_h,
        }
    }

    /// Where the marker for `hsv` sits in the saturation/value field.
    pub fn sat_val_point(&self, hsv: Hsv) -> (i32, i32) {
        let area = self.sat_val;
        (
            area.x + scale(i32::from(hsv.s), 255, area.width - 1),
            area.y + scale(255 - i32::from(hsv.v), 255, area.height - 1),
        )
    }

    /// The column of the hue strip's marker for hue `h`.
    pub fn hue_x(&self, h: u16) -> i32 {
        let strip = self.hue;
        strip.x + scale(i32::from(h), i32::from(HUE_STEPS) - 1, strip.width - 1)
    }

    /// Where a press picks a hue: the strip with its marker's margin.
    fn hue_target(&self) -> Rect {
        Rect {
            y: self.hue.y - HUE_MARGIN,
            height: self.hue.height + 2 * HUE_MARGIN,
            ..self.hue
        }
    }

    /// Tops of the impact note's lines.
    fn impact_ys(&self) -> impl Iterator<Item = i32> + '_ {
        (0..IMPACT_LINES as i32).map(|n| self.impact_y + n * self.cell_h)
    }

    /// The scope rows, top to bottom for [`ColorScope::All`] and
    /// [`ColorScope::Palette`]: the impact note's first two lines.
    pub fn scope_rows(&self) -> [Rect; 2] {
        [0, 1].map(|n| Rect {
            x: self.text_x,
            y: self.impact_y + n * self.cell_h,
            width: self.text_right - self.text_x,
            height: self.cell_h,
        })
    }

    fn scope_at(&self, point: Point) -> Option<ColorScope> {
        let [all, palette] = self.scope_rows();
        if all.contains(point) {
            Some(ColorScope::All)
        } else if palette.contains(point) {
            Some(ColorScope::Palette)
        } else {
            None
        }
    }

    /// Text columns between the window's padding.
    fn columns(&self) -> usize {
        usize::try_from((self.text_right - self.text_x) / self.cell_w.max(1)).unwrap_or(0)
    }

    fn field_at(&self, point: Point) -> Option<Field> {
        Field::ALL
            .into_iter()
            .zip(self.fields)
            .find(|(_, rect)| rect.contains(point))
            .map(|(field, _)| field)
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
