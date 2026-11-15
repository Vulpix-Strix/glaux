//! The full-map overview.

use glaux_i18n::Arg;
use glaux_input::{InputEvent, Point, PointerButton};
use glaux_machine::bg::BgVram;
use glaux_machine::framebuffer::{Framebuffer, HEIGHT, WIDTH};
use glaux_machine::tilemap::{BG_LAYERS, BgLayer};
use glaux_video::draw::draw_text;

use super::canvas::{fill_clipped, outline_clipped};
use super::compose::{Composer, TileColors};
use super::editing::map_size;
use super::frame::{BAR as BAR_HEIGHT, DrawerSize, Frame, Hit};
use super::view::{TILE, Viewport};
use super::{MapEditor, MapOut};
use crate::system::map_document::MapDocument;
use crate::system::screen::{ACCENT, BACKGROUND, DIM, PALE, TEXT};
use crate::system::workspace::dialog::text_cells;
use crate::system::workspace::sprites::{SpriteSource, checker, outline};
use crate::system::workspace::{BAR, Rect, RenderContext};
use crate::ui_text;

const MAX_SHOWN: (i32, i32) = (256, 128);
const LEFT: i32 = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum OverviewZoom {
    X1,
    X2,
    X4,
    X8,
    X16,
}

impl OverviewZoom {
    const ALL: [Self; 5] = [Self::X1, Self::X2, Self::X4, Self::X8, Self::X16];

    /// Map pixels per screen pixel along each axis.
    pub(super) const fn divisor(self) -> i32 {
        match self {
            Self::X1 => 1,
            Self::X2 => 2,
            Self::X4 => 4,
            Self::X8 => 8,
            Self::X16 => 16,
        }
    }

    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::X1 => "1/1",
            Self::X2 => "1/2",
            Self::X4 => "1/4",
            Self::X8 => "1/8",
            Self::X16 => "1/16",
        }
    }

    pub(super) fn shown(self, map: (i32, i32)) -> (i32, i32) {
        let divisor = self.divisor();
        (ceil_div(map.0, divisor), ceil_div(map.1, divisor))
    }

    /// The closest zoom that shows all of `map`, a size in pixels.
    pub(super) fn fit(map: (i32, i32)) -> Self {
        Self::ALL
            .into_iter()
            .find(|zoom| {
                let shown = zoom.shown(map);
                shown.0 <= MAX_SHOWN.0 && shown.1 <= MAX_SHOWN.1
            })
            .unwrap_or(Self::X16)
    }

    /// Zooming in stops at `fit`, so the whole map stays in view.
    pub(super) fn zoomed_in(self, fit: Self) -> Option<Self> {
        let index = Self::ALL.iter().position(|zoom| *zoom == self)?;
        let next = Self::ALL.get(index.checked_sub(1)?)?;
        (*next >= fit).then_some(*next)
    }

    pub(super) fn zoomed_out(self) -> Option<Self> {
        let index = Self::ALL.iter().position(|zoom| *zoom == self)?;
        Self::ALL.get(index + 1).copied()
    }
}

/// The overview's own state. Its layer toggles start as the editor's eyes
/// and never change them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Overview {
    /// `None` follows the open map's fit.
    zoom: Option<OverviewZoom>,
    pub(super) shown: [bool; BG_LAYERS],
    pub(super) screens: bool,
    /// Where the viewport was grabbed, in map pixels from its top-left.
    pub(super) drag: Option<(i32, i32)>,
    pub(super) hover: Option<Point>,
    /// The drawer's size when the overview opened, which comes back when it
    /// closes and sets the canvas the viewport is measured against.
    pub(super) restore: DrawerSize,
}

impl Overview {
    pub(super) fn new(restore: DrawerSize, shown: [bool; BG_LAYERS]) -> Self {
        Self {
            zoom: None,
            shown,
            screens: true,
            drag: None,
            hover: None,
            restore,
        }
    }

    pub(super) fn zoom(&self, map: (i32, i32)) -> OverviewZoom {
        let fit = OverviewZoom::fit(map);
        self.zoom.map_or(fit, |zoom| zoom.max(fit))
    }

    pub(super) fn new_map(&mut self) {
        self.zoom = None;
        self.drag = None;
        self.hover = None;
    }
}

fn ceil_div(value: i32, divisor: i32) -> i32 {
    -(-value).div_euclid(divisor)
}

fn holds(rect: Rect, (x, y): (i32, i32)) -> bool {
    (rect.x..rect.right()).contains(&x) && (rect.y..rect.bottom()).contains(&y)
}

pub(super) fn map_pixels(document: &MapDocument) -> (i32, i32) {
    let (width, height) = map_size(document);
    (width as i32 * TILE, height as i32 * TILE)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct OverviewLayout {
    pub(super) title: (i32, i32),
    pub(super) map: Rect,
    pub(super) show: (i32, i32),
    /// The layer toggles, by layer index.
    pub(super) layers: [Rect; BG_LAYERS],
    pub(super) screens: Rect,
    pub(super) info: (i32, i32),
    divisor: i32,
}

impl OverviewLayout {
    pub(super) fn new(frame: &Frame, map: (i32, i32), zoom: OverviewZoom, cell_w: i32) -> Self {
        let title = (LEFT, frame.options.bottom() + 4);
        let top = title.1 + 14;
        let (width, height) = zoom.shown(map);
        let show_y = top + MAX_SHOWN.1 + 6;
        let toggle = |x: i32, label: &str| Rect {
            x,
            y: show_y,
            width: text_cells(label) * cell_w + 4,
            height: BAR_HEIGHT - 1,
        };
        let mut x = LEFT + text_cells(&ui_text::get("maps-overview-show")) * cell_w + 8;
        let mut layers = [Rect {
            x: 0,
            y: 0,
            width: 0,
            height: 0,
        }; BG_LAYERS];
        for layer in [BgLayer::Bg2, BgLayer::Bg1, BgLayer::Bg0] {
            let rect = toggle(x, &layer.to_string());
            layers[layer.index()] = rect;
            x = rect.right() + 4;
        }
        let screens = toggle(x + 8, &ui_text::get("maps-overview-screens"));
        Self {
            title,
            map: Rect {
                x: LEFT,
                y: top,
                width,
                height,
            },
            show: (LEFT, show_y),
            layers,
            screens,
            info: (LEFT, show_y + 14),
            divisor: zoom.divisor(),
        }
    }

    /// The map pixel at the middle of the block under `at`, which may lie
    /// off the map.
    pub(super) fn map_point(&self, at: Point) -> (i32, i32) {
        let divisor = self.divisor;
        (
            (i32::from(at.x) - self.map.x) * divisor + divisor / 2,
            (i32::from(at.y) - self.map.y) * divisor + divisor / 2,
        )
    }

    /// Where `rect`, in map pixels, shows on screen.
    pub(super) fn on_screen(&self, rect: Rect) -> Rect {
        let divisor = self.divisor;
        let x = self.map.x + rect.x.div_euclid(divisor);
        let y = self.map.y + rect.y.div_euclid(divisor);
        Rect {
            x,
            y,
            width: self.map.x + ceil_div(rect.right(), divisor) - x,
            height: self.map.y + ceil_div(rect.bottom(), divisor) - y,
        }
    }
}

/// The part of the map `view` shows on a canvas of `area`, in map pixels.
fn view_rect(view: Viewport, area: (i32, i32), map: (i32, i32)) -> Rect {
    let scale = view.zoom().scale();
    let (x, y) = view.offset();
    Rect {
        x: x / scale,
        y: y / scale,
        width: (area.0 / scale).min(map.0),
        height: (area.1 / scale).min(map.1),
    }
}

/// The master color the overview shows for the block at `(x, y)`.
///
/// Wherever a block covers a tile that draws, the block shows a color. Up
/// to 1/4 the block lies inside one tile and shows its middle pixel, or
/// the first opaque one when the middle is clear. From 1/8 on, a block
/// covers whole tiles and shows the frontmost of their most common colors.
fn sample(
    composer: &Composer<'_>,
    bg: &BgVram,
    colors: &mut TileColors<'_>,
    zoom: OverviewZoom,
    tiles: (usize, usize),
    (x, y): (usize, usize),
) -> Option<u8> {
    let divisor = zoom.divisor() as usize;
    let side = TILE as usize;
    match zoom {
        OverviewZoom::X1 => composer
            .cell(x / side, y / side)
            .pixel(bg, x % side, y % side),
        OverviewZoom::X2 | OverviewZoom::X4 => {
            let (left, top) = (x * divisor, y * divisor);
            let cell = composer.cell(left / side, top / side);
            if cell.is_empty() {
                return None;
            }
            let (left, top) = (left % side, top % side);
            let middle = divisor / 2;
            cell.pixel(bg, left + middle, top + middle).or_else(|| {
                (0..divisor * divisor)
                    .find_map(|i| cell.pixel(bg, left + i % divisor, top + i / divisor))
            })
        }
        OverviewZoom::X8 | OverviewZoom::X16 => {
            let per = divisor / side;
            let mut best: Option<(u8, u8)> = None;
            for ty in y * per..(y * per + per).min(tiles.1) {
                for tx in x * per..(x * per + per).min(tiles.0) {
                    let front = composer.cell(tx, ty).front_color(bg, colors);
                    if let Some((master, depth)) = front
                        && best.is_none_or(|(_, shown)| depth < shown)
                    {
                        best = Some((master, depth));
                    }
                }
            }
            best.map(|(master, _)| master)
        }
    }
}

fn dashed(framebuffer: &mut Framebuffer, rect: Rect, clip: Rect, color: u8) {
    for edge in [
        Rect { height: 1, ..rect },
        Rect {
            y: rect.bottom() - 1,
            height: 1,
            ..rect
        },
        Rect { width: 1, ..rect },
        Rect {
            x: rect.right() - 1,
            width: 1,
            ..rect
        },
    ] {
        let length = edge.width.max(edge.height);
        for step in (0..length).filter(|step| step % 4 < 2) {
            let dot = if edge.width > edge.height {
                Rect {
                    x: edge.x + step,
                    width: 1,
                    ..edge
                }
            } else {
                Rect {
                    y: edge.y + step,
                    height: 1,
                    ..edge
                }
            };
            fill_clipped(framebuffer, dot, clip, color);
        }
    }
}

fn tile_pair(view: Viewport) -> String {
    let tile = view.zoom().tile_px();
    let (x, y) = view.offset();
    format!("{:02}", x / tile) + "," + &format!("{:02}", y / tile)
}

impl MapEditor {
    pub(super) fn toggle_overview(&mut self, document: &mut MapDocument) {
        if self.edit.overview.is_some() {
            self.close_overview();
            return;
        }
        self.end_stroke(Some(document));
        self.close_picker();
        self.edit.attrs = None;
        self.edit.panels.clear();
        self.edit.overview = Some(Overview::new(self.edit.drawer, self.edit.visible));
        self.edit.drawer = DrawerSize::Collapsed;
    }

    pub(super) fn close_overview(&mut self) {
        if let Some(overview) = self.edit.overview.take() {
            self.edit.drawer = overview.restore;
        }
    }

    fn editor_area(&self) -> (i32, i32) {
        let canvas = self.editor_frame().canvas;
        (canvas.width, canvas.height)
    }

    fn editor_view(&self, document: &MapDocument) -> Viewport {
        self.viewport(&self.editor_frame(), document)
    }

    /// The editor's view with its top-left at `(x, y)` in map pixels, kept
    /// on the map.
    fn view_at(&self, document: &MapDocument, (x, y): (i32, i32)) -> Viewport {
        let mut view = self.editor_view(document);
        let scale = view.zoom().scale();
        view.scroll_to(x * scale, y * scale, map_size(document), self.editor_area());
        view
    }

    fn view_around(&self, document: &MapDocument, point: (i32, i32)) -> Viewport {
        let shown = view_rect(
            self.editor_view(document),
            self.editor_area(),
            map_pixels(document),
        );
        self.view_at(
            document,
            (point.0 - shown.width / 2, point.1 - shown.height / 2),
        )
    }

    pub(super) fn overview_input(
        &mut self,
        event: InputEvent,
        frame: &Frame,
        document: &mut MapDocument,
    ) -> Option<Option<MapOut>> {
        let mut overview = self.edit.overview?;
        let map = map_pixels(document);
        let zoom = overview.zoom(map);
        let layout = OverviewLayout::new(frame, map, zoom, self.metrics.cell_w);
        let zoom_in = |overview: &mut Overview| {
            overview.zoom = Some(zoom.zoomed_in(OverviewZoom::fit(map)).unwrap_or(zoom));
        };
        let zoom_out = |overview: &mut Overview| {
            overview.zoom = Some(zoom.zoomed_out().unwrap_or(zoom));
        };
        match event {
            InputEvent::Break if overview.drag.is_some() => overview.drag = None,
            InputEvent::Break | InputEvent::Text('m' | 'M') => {
                self.close_overview();
                return Some(None);
            }
            InputEvent::Text('+' | '=') => zoom_in(&mut overview),
            InputEvent::Text('-' | '_') => zoom_out(&mut overview),
            InputEvent::PointerMove(at) => {
                self.edit.pointer = at;
                overview.hover = at;
                if let (Some(grab), Some(at)) = (overview.drag, at) {
                    let point = layout.map_point(at);
                    self.edit.view = self.view_at(document, (point.0 - grab.0, point.1 - grab.1));
                }
            }
            InputEvent::PointerDown {
                button: PointerButton::Primary,
                at,
                ..
            } => {
                if layout.map.contains(at) {
                    let point = layout.map_point(at);
                    let area = self.editor_area();
                    let mut shown = view_rect(self.editor_view(document), area, map);
                    if !holds(shown, point) {
                        self.edit.view = self.view_around(document, point);
                        shown = view_rect(self.edit.view, area, map);
                    }
                    overview.drag = Some((point.0 - shown.x, point.1 - shown.y));
                } else if let Some(layer) = BgLayer::ALL
                    .into_iter()
                    .find(|layer| layout.layers[layer.index()].contains(at))
                {
                    overview.shown[layer.index()] = !overview.shown[layer.index()];
                } else if layout.screens.contains(at) {
                    overview.screens = !overview.screens;
                } else {
                    match frame.hit(at, self.edit.strip_top) {
                        Hit::Overview => {
                            self.close_overview();
                            return Some(None);
                        }
                        Hit::ZoomIn => zoom_in(&mut overview),
                        Hit::ZoomOut => zoom_out(&mut overview),
                        Hit::Selector => self.open_list(Some(&*document)),
                        _ => {}
                    }
                }
            }
            InputEvent::PointerUp {
                button: PointerButton::Primary,
                ..
            }
            | InputEvent::PointerCancel => overview.drag = None,
            InputEvent::Undo | InputEvent::Redo | InputEvent::Save => return None,
            _ => {}
        }
        self.edit.overview = Some(overview);
        Some(None)
    }

    pub(super) fn overview_reacts(&self, at: Point, document: &MapDocument) -> bool {
        let Some(overview) = self.edit.overview else {
            return false;
        };
        let frame = self.frame();
        let map = map_pixels(document);
        let layout = OverviewLayout::new(&frame, map, overview.zoom(map), self.metrics.cell_w);
        layout.map.contains(at)
            || layout.screens.contains(at)
            || layout.layers.iter().any(|rect| rect.contains(at))
            || matches!(
                frame.hit(at, self.edit.strip_top),
                Hit::Overview | Hit::ZoomIn | Hit::ZoomOut | Hit::Selector
            )
    }

    pub(super) fn render_overview(
        &self,
        ctx: &mut RenderContext<'_>,
        frame: &Frame,
        document: &MapDocument,
        source: &SpriteSource<'_>,
        overview: &Overview,
        pointer: Option<Point>,
    ) {
        let map = map_pixels(document);
        let zoom = overview.zoom(map);
        let layout = OverviewLayout::new(frame, map, zoom, self.metrics.cell_w);
        let (framebuffer, font) = (&mut *ctx.framebuffer, ctx.font);

        let name = &document.meta().name;
        let (width, height) = map_size(document);
        let size = ui_text::get_with(
            "maps-overview-size",
            &[
                ("width", Arg::Text(&width.to_string())),
                ("height", Arg::Text(&height.to_string())),
                ("pwidth", Arg::Text(&map.0.to_string())),
                ("pheight", Arg::Text(&map.1.to_string())),
            ],
        );
        let (x, y) = layout.title;
        draw_text(framebuffer, font, name, x, y, PALE);
        draw_text(framebuffer, font, &size, x + self.cells(name) + 8, y, DIM);

        let area = layout.map;
        outline(framebuffer, area.inset(-1), DIM);
        checker(framebuffer, area, (TILE / zoom.divisor() / 2).max(2));
        self.overview_pixels(framebuffer, source, document, overview, &layout, zoom);

        if overview.screens {
            let divisor = zoom.divisor();
            let (screen_w, screen_h) = (WIDTH as i32, HEIGHT as i32);
            for x in (screen_w..map.0).step_by(WIDTH) {
                let line = Rect {
                    x: area.x + x / divisor,
                    width: 1,
                    ..area
                };
                dashed(framebuffer, line, area, DIM);
            }
            for y in (screen_h..map.1).step_by(HEIGHT) {
                let line = Rect {
                    y: area.y + y / divisor,
                    height: 1,
                    ..area
                };
                dashed(framebuffer, line, area, DIM);
            }
            let text = format!("{WIDTH}x{HEIGHT}");
            let tag = Rect {
                x: area.x + 2,
                y: area.y + 2,
                width: self.cells(&text) + 4,
                height: 9,
            };
            // The tag names the first screen, so it shows only inside it.
            if tag.right() + 2 <= area.x + screen_w / divisor {
                fill_clipped(framebuffer, tag, area, BAR);
                draw_text(framebuffer, font, &text, tag.x + 2, tag.y - 1, TEXT);
            }
        }

        let view = self.editor_view(document);
        let shown = view_rect(view, self.editor_area(), map);
        let current = layout.on_screen(shown);
        outline_clipped(framebuffer, current.inset(-1), area, BACKGROUND);
        outline_clipped(framebuffer, current, area, ACCENT);

        let target = pointer
            .filter(|at| overview.drag.is_none() && area.contains(*at))
            .map(|at| layout.map_point(at))
            .filter(|point| !holds(shown, *point))
            .map(|point| self.view_around(document, point));
        if let Some(target) = target {
            let rect = layout.on_screen(view_rect(target, self.editor_area(), map));
            dashed(framebuffer, rect, area, PALE);
        }

        let (x, y) = layout.show;
        draw_text(
            framebuffer,
            font,
            &ui_text::get("maps-overview-show"),
            x,
            y,
            DIM,
        );
        for layer in [BgLayer::Bg2, BgLayer::Bg1, BgLayer::Bg0] {
            let rect = layout.layers[layer.index()];
            let hovered = pointer.is_some_and(|at| rect.contains(at));
            let on = overview.shown[layer.index()];
            self.toggle(ctx, rect, &layer.to_string(), on, hovered, true);
        }
        let hovered = pointer.is_some_and(|at| layout.screens.contains(at));
        let label = ui_text::get("maps-overview-screens");
        self.toggle(ctx, layout.screens, &label, overview.screens, hovered, true);

        let (framebuffer, font) = (&mut *ctx.framebuffer, ctx.font);
        let (x, y) = layout.info;
        let current =
            ui_text::get_with("maps-overview-view", &[("at", Arg::Text(&tile_pair(view)))]);
        draw_text(framebuffer, font, &current, x, y, ACCENT);
        if let Some(target) = target {
            let jump = ui_text::get_with(
                "maps-overview-jump",
                &[("at", Arg::Text(&tile_pair(target)))],
            );
            draw_text(
                framebuffer,
                font,
                &jump,
                x + self.cells(&current) + 18,
                y,
                PALE,
            );
        }
    }

    /// The cost is bound by the size shown, at most 256 x 128 blocks,
    /// whatever the size of the map.
    fn overview_pixels(
        &self,
        framebuffer: &mut Framebuffer,
        source: &SpriteSource<'_>,
        document: &MapDocument,
        overview: &Overview,
        layout: &OverviewLayout,
        zoom: OverviewZoom,
    ) {
        let composer = Composer::new(
            BgLayer::ALL.map(|layer| document.layer(layer)),
            overview.shown,
        );
        let mut colors = TileColors::new(source.bg.tiles());
        let slots = source.preview.slots();
        let tiles = map_size(document);
        let area = layout.map;
        for y in 0..area.height {
            for x in 0..area.width {
                let block = (x as usize, y as usize);
                if let Some(master) = sample(&composer, source.bg, &mut colors, zoom, tiles, block)
                {
                    framebuffer.set_pixel(area.x + x, area.y + y, slots[usize::from(master)]);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
