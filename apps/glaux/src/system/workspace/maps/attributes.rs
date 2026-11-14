//! The attribute panel of the Map editor.
use glaux_i18n::Arg;
use glaux_input::{InputEvent, Point, PointerButton};
use glaux_machine::framebuffer::Framebuffer;
use glaux_machine::tilemap::{BG_PALETTE_BANKS, BgBank, TileEntry};
use glaux_machine::system_rom::SystemIcon;
use glaux_video::draw::{draw_ink_image_centered, draw_text, fill_rect};

use super::frame::Frame;
use super::pattern::{Attr, Shared, Summary, summarize};
use super::{MapEditor, MapOut};
use crate::system::map_document::MapDocument;
use crate::system::screen::{ACCENT, BACKGROUND, DIM, PALE, TEXT};
use crate::system::workspace::{BAR, HOVER_SHADE, OUTLINE, Rect, RenderContext};
use crate::ui_text;

const WIDTH: i32 = 152;
const LABEL_WIDTH: i32 = 54;
const ROW: i32 = 12;
const BANK_W: i32 = 12;
const BANK_H: i32 = 10;
const CHECK: i32 = 16;
const BOX: i32 = 9;
const RADIO: i32 = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AttrTab {
    Brush,
    Cells,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct AttrPanel {
    pub(super) tab: AttrTab,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AttrHit {
    Tab(AttrTab),
    Bank(u8),
    FlipX,
    FlipY,
    Priority(bool),
    Nothing,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct AttrLayout {
    pub(super) window: Rect,
    tabs: [Rect; 2],
    count: (i32, i32),
    banks: [Rect; BG_PALETTE_BANKS],
    mixed: (i32, i32),
    flips: [Rect; 2],
    priorities: [Rect; 2],
    foot: Rect,
}

impl AttrLayout {
    pub(super) fn new(frame: &Frame, cell_w: i32, cells_label_cells: i32) -> Self {
        let height = 1 + ROW + 13 + 2 + 3 * ROW + 2 * 2 + 4 + ROW + 1;
        let window = Rect {
            x: frame.drawer.right() - WIDTH,
            y: frame.options.bottom() + 1,
            width: WIDTH,
            height,
        };
        let inner = window.inset(1);
        let tab_w = |cells: i32| cells * cell_w + 8;
        let brush = Rect {
            x: inner.x + 4,
            y: inner.y,
            width: tab_w(5),
            height: ROW - 1,
        };
        let cells = Rect {
            x: brush.right() + 1,
            width: tab_w(cells_label_cells),
            ..brush
        };
        let count = (inner.x + 4, inner.y + ROW + 1);
        let body = inner.y + ROW + 13 + 2;
        let controls = inner.x + 4 + LABEL_WIDTH;
        let row = |n: i32| body + n * (ROW + 2);
        let banks = std::array::from_fn(|bank| Rect {
            x: controls + bank as i32 * (BANK_W + 1),
            y: row(0) + 1,
            width: BANK_W,
            height: BANK_H,
        });
        let mixed = (banks[BG_PALETTE_BANKS - 1].right() + 6, row(0));
        let item = |x: i32, y: i32, cells: i32, box_w: i32| Rect {
            x,
            y,
            width: box_w + 6 + cells * cell_w,
            height: ROW,
        };
        let flip_x = item(controls, row(1), 1, CHECK - 4);
        let flip_y = item(flip_x.right() + 12, row(1), 1, CHECK - 4);
        let low = item(controls, row(2), 3, RADIO);
        let high = item(low.right() + 12, row(2), 4, RADIO);
        let foot = Rect {
            x: inner.x,
            y: inner.bottom() - ROW,
            width: inner.width,
            height: ROW,
        };
        Self {
            window,
            tabs: [brush, cells],
            count,
            banks,
            mixed,
            flips: [flip_x, flip_y],
            priorities: [low, high],
            foot,
        }
    }

    pub(super) fn hit(&self, at: Point, cells_tab: bool) -> AttrHit {
        let tabs = [AttrTab::Brush, AttrTab::Cells];
        for (rect, tab) in self.tabs.iter().zip(tabs) {
            if rect.contains(at) && (tab == AttrTab::Brush || cells_tab) {
                return AttrHit::Tab(tab);
            }
        }
        for (bank, rect) in self.banks.iter().enumerate() {
            if rect.contains(at) {
                return AttrHit::Bank(bank as u8);
            }
        }
        if self.flips[0].contains(at) {
            return AttrHit::FlipX;
        }
        if self.flips[1].contains(at) {
            return AttrHit::FlipY;
        }
        for (rect, high) in self.priorities.iter().zip([false, true]) {
            if rect.contains(at) {
                return AttrHit::Priority(high);
            }
        }
        AttrHit::Nothing
    }
}

impl MapEditor {
    pub(super) fn toggle_attr(&mut self) {
        self.edit.attrs = match self.edit.attrs {
            Some(_) => None,
            None => Some(AttrPanel {
                tab: if self.edit.selection.is_some() {
                    AttrTab::Cells
                } else {
                    AttrTab::Brush
                },
            }),
        };
    }

    fn cells_label(&self) -> String {
        let (width, height) = self
            .edit
            .selection
            .map_or((0, 0), |selection| (selection.width(), selection.height()));
        ui_text::get_with(
            "maps-attr-cells-tab",
            &[
                ("width", Arg::Number(width as f64)),
                ("height", Arg::Number(height as f64)),
            ],
        )
    }

    fn attr_layout(&self, frame: &Frame) -> AttrLayout {
        let cells = i32::try_from(self.cells_label().chars().count()).unwrap_or(0);
        AttrLayout::new(frame, self.metrics.cell_w, cells)
    }

    fn attr_tab(&self, panel: AttrPanel) -> AttrTab {
        if self.edit.selection.is_none() {
            AttrTab::Brush
        } else {
            panel.tab
        }
    }

    pub(super) fn attr_input(
        &mut self,
        event: InputEvent,
        frame: &Frame,
        document: &mut MapDocument,
    ) -> Option<Option<MapOut>> {
        let panel = self.edit.attrs?;
        let layout = self.attr_layout(frame);
        match event {
            InputEvent::Break | InputEvent::Text('a' | 'A') => {
                self.edit.attrs = None;
                Some(None)
            }
            InputEvent::PointerDown {
                button: PointerButton::Primary,
                at,
                ..
            } if layout.window.contains(at) => {
                let cells_tab = self.edit.selection.is_some();
                let out = match layout.hit(at, cells_tab) {
                    AttrHit::Tab(tab) => {
                        self.edit.attrs = Some(AttrPanel { tab });
                        None
                    }
                    AttrHit::Bank(bank) => self.change_attr(document, panel, BgBank::new(bank).map(Attr::Bank)),
                    AttrHit::FlipX => self.toggle_flip(document, panel, true),
                    AttrHit::FlipY => self.toggle_flip(document, panel, false),
                    AttrHit::Priority(high) => self.change_attr(document, panel, Some(Attr::Priority(high))),
                    AttrHit::Nothing => None,
                };
                Some(out)
            }
            InputEvent::PointerDown { at, .. } | InputEvent::PointerUp { at, .. }
                if layout.window.contains(at) =>
            {
                Some(None)
            }
            _ => None,
        }
    }

    fn summary(&self, document: &MapDocument, panel: AttrPanel) -> Option<Summary> {
        match (self.attr_tab(panel), self.edit.selection) {
            (AttrTab::Cells, Some(selection)) => summarize(document.layer(self.edit.layer), selection),
            (AttrTab::Cells, None) => None,
            (AttrTab::Brush, _) => {
                let brush = self.edit.brush;
                Some(Summary {
                    bank: Shared::Same(brush.bank()),
                    flip_x: Shared::Same(brush.flip_x()),
                    flip_y: Shared::Same(brush.flip_y()),
                    priority: Shared::Same(brush.priority()),
                    cells: 1,
                })
            }
        }
    }

    fn toggle_flip(&mut self, document: &mut MapDocument, panel: AttrPanel, x: bool) -> Option<MapOut> {
        let summary = self.summary(document, panel)?;
        let shared = if x { summary.flip_x } else { summary.flip_y };
        // A mixed flip goes to on first.
        let next = !matches!(shared, Shared::Same(true));
        self.change_attr(
            document,
            panel,
            Some(if x { Attr::FlipX(next) } else { Attr::FlipY(next) }),
        )
    }

    fn change_attr(
        &mut self,
        document: &mut MapDocument,
        panel: AttrPanel,
        attr: Option<Attr>,
    ) -> Option<MapOut> {
        let attr = attr?;
        let Some(selection) = self.edit.selection.filter(|_| self.attr_tab(panel) == AttrTab::Cells) else {
            self.edit.brush = attr.apply(self.edit.brush);
            return None;
        };
        if self.edit.locked[self.edit.layer.index()] {
            return Some(self.locked_notice());
        }
        let layer = document.layer(self.edit.layer);
        let changes: Vec<_> = (selection.top..=selection.bottom)
            .flat_map(|y| (selection.left..=selection.right).map(move |x| (x, y)))
            .filter_map(|(x, y)| {
                let entry: TileEntry = layer.get(x, y)?;
                let next = attr.apply(entry);
                (entry.tile() != 0 && next != entry).then_some((x, y, next))
            })
            .collect();
        if changes.is_empty() {
            return None;
        }
        document.begin_edit();
        for (x, y, entry) in changes {
            self.write(document, (x as i32, y as i32), entry);
        }
        document.end_edit();
        None
    }

    pub(super) fn attr_status(&self) -> Option<String> {
        self.edit.selection.map(|selection| {
            ui_text::get_with(
                "maps-status-selection",
                &[
                    ("width", Arg::Number(selection.width() as f64)),
                    ("height", Arg::Number(selection.height() as f64)),
                    ("x", Arg::Number(selection.left as f64)),
                    ("y", Arg::Number(selection.top as f64)),
                ],
            )
        })
    }

    pub(super) fn render_attr(
        &self,
        ctx: &mut RenderContext<'_>,
        frame: &Frame,
        document: &MapDocument,
        pointer: Option<Point>,
        panel: AttrPanel,
    ) {
        let layout = self.attr_layout(frame);
        let (framebuffer, font) = (&mut *ctx.framebuffer, ctx.font);
        let fill = |framebuffer: &mut Framebuffer, rect: Rect, color| {
            fill_rect(framebuffer, rect.x, rect.y, rect.width, rect.height, color);
        };
        let window = layout.window;
        fill(framebuffer, Rect { x: window.x + 2, y: window.y + 2, ..window }, BAR);
        fill(framebuffer, window, DIM);
        fill(framebuffer, window.inset(1), BACKGROUND);

        let tab = self.attr_tab(panel);
        let hover = pointer.map_or(AttrHit::Nothing, |at| layout.hit(at, self.edit.selection.is_some()));
        let label = |tab: AttrTab| match tab {
            AttrTab::Brush => ui_text::get("maps-attr-brush-tab"),
            AttrTab::Cells => self.cells_label(),
        };
        let tabs = if self.edit.selection.is_some() {
            vec![(AttrTab::Brush, layout.tabs[0]), (AttrTab::Cells, layout.tabs[1])]
        } else {
            vec![(AttrTab::Brush, layout.tabs[0])]
        };
        let inner = window.inset(1);
        fill(framebuffer, Rect { y: inner.y + ROW - 1, height: 1, ..inner }, BAR);
        for (which, rect) in tabs {
            fill(framebuffer, rect, BAR);
            let on = which == tab;
            let ink = if on { TEXT } else if hover == AttrHit::Tab(which) { PALE } else { DIM };
            draw_text(framebuffer, font, &label(which), rect.x + 4, rect.y, ink);
            if on {
                fill(framebuffer, Rect { y: rect.bottom() - 1, height: 1, ..rect }, ACCENT);
            }
        }

        let summary = self.summary(document, panel);
        let count = match tab {
            AttrTab::Cells => ui_text::get_with(
                "maps-attr-cells",
                &[
                    ("count", Arg::Number(summary.map_or(0, |s| s.cells) as f64)),
                    ("layer", Arg::Text(&self.edit.layer.to_string())),
                ],
            ),
            AttrTab::Brush => ui_text::get_with(
                "maps-attr-brush",
                &[("tile", Arg::Text(&format!("{:04}", self.edit.brush.tile())))],
            ),
        };
        draw_text(framebuffer, font, &count, layout.count.0, layout.count.1, PALE);

        let label_x = inner.x + 4;
        let rows = [
            (layout.banks[0].y - 1, "maps-attr-palette"),
            (layout.flips[0].y, "maps-attr-flip"),
            (layout.priorities[0].y, "maps-attr-priority"),
        ];
        for (y, id) in rows {
            draw_text(framebuffer, font, &ui_text::get(id), label_x, y, DIM);
        }
        let Some(summary) = summary else {
            self.attr_foot(framebuffer, font, &layout);
            return;
        };
        for (bank, rect) in layout.banks.iter().enumerate() {
            let chosen = summary.bank == Shared::Same(BgBank::new(bank as u8).unwrap_or(BgBank::ZERO));
            let (face, ink) = if chosen {
                (PALE, BACKGROUND)
            } else if hover == AttrHit::Bank(bank as u8) {
                (HOVER_SHADE, TEXT)
            } else {
                (BACKGROUND, DIM)
            };
            fill(framebuffer, *rect, face);
            let text = bank.to_string();
            let x = rect.x + (rect.width - self.metrics.cell_w) / 2;
            draw_text(framebuffer, font, &text, x, rect.y - 1, ink);
        }
        if summary.bank == Shared::Mixed {
            draw_text(framebuffer, font, &ui_text::get("maps-attr-mixed"), layout.mixed.0, layout.mixed.1, PALE);
        }
        for (rect, shared, letter, which) in [
            (layout.flips[0], summary.flip_x, "X", AttrHit::FlipX),
            (layout.flips[1], summary.flip_y, "Y", AttrHit::FlipY),
        ] {
            let ink = if hover == which { PALE } else { TEXT };
            let slot = Rect { width: CHECK, ..rect };
            if shared == Shared::Same(true) {
                draw_ink_image_centered(
                    framebuffer,
                    ctx.ui.icon(SystemIcon::Check),
                    (slot.x, slot.y, slot.width, slot.height),
                    &[OUTLINE, ink, ink],
                );
            } else {
                let outer = Rect {
                    x: slot.x + (CHECK - BOX) / 2,
                    y: slot.y + (ROW - BOX) / 2,
                    width: BOX,
                    height: BOX,
                };
                outline_box(framebuffer, outer, ink);
                if shared == Shared::Mixed {
                    let dash = Rect { x: outer.x + 2, y: outer.y + BOX / 2, width: BOX - 4, height: 1 };
                    fill(framebuffer, dash, ink);
                }
            }
            draw_text(framebuffer, font, letter, rect.x + CHECK + 2, rect.y, ink);
        }
        for (rect, high, text) in [
            (layout.priorities[0], false, ui_text::get("maps-attr-low")),
            (layout.priorities[1], true, ui_text::get("maps-attr-high")),
        ] {
            let ink = if hover == AttrHit::Priority(high) { PALE } else { TEXT };
            let outer = Rect { y: rect.y + 2, width: RADIO, height: RADIO, ..rect };
            outline_box(framebuffer, outer, ink);
            if summary.priority == Shared::Same(high) {
                fill(framebuffer, outer.inset(1), ink);
            }
            draw_text(framebuffer, font, &text, rect.x + RADIO + 6, rect.y, ink);
        }
        if summary.priority == Shared::Mixed {
            let x = layout.priorities[1].right() + 6;
            draw_text(framebuffer, font, &ui_text::get("maps-attr-mixed"), x, layout.priorities[1].y, PALE);
        }
        self.attr_foot(framebuffer, font, &layout);
    }

    fn attr_foot(
        &self,
        framebuffer: &mut Framebuffer,
        font: &glaux_machine::font::BitmapFont,
        layout: &AttrLayout,
    ) {
        fill_rect(framebuffer, layout.foot.x, layout.foot.y, layout.foot.width, layout.foot.height, BAR);
        draw_text(framebuffer, font, &ui_text::get("maps-attr-applies"), layout.foot.x + 4, layout.foot.y, DIM);
        let undo = ui_text::get("maps-attr-undo");
        let width = i32::try_from(font.text_cells(&undo)).unwrap_or(0) * self.metrics.cell_w;
        draw_text(framebuffer, font, &undo, layout.foot.right() - 4 - width, layout.foot.y, DIM);
    }
}

fn outline_box(framebuffer: &mut Framebuffer, rect: Rect, color: u8) {
    crate::system::workspace::sprites::outline(framebuffer, rect, color);
}

#[cfg(test)]
mod tests;
