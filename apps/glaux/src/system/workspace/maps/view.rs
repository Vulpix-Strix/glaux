//! The canvas' zoom and scroll position.

use crate::system::workspace::Rect;

pub(super) const TILE: i32 = 8;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum Zoom {
    X1,
    #[default]
    X2,
    X4,
}

impl Zoom {
    pub(super) const fn scale(self) -> i32 {
        match self {
            Self::X1 => 1,
            Self::X2 => 2,
            Self::X4 => 4,
        }
    }

    pub(super) const fn tile_px(self) -> i32 {
        TILE * self.scale()
    }

    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::X1 => "1x",
            Self::X2 => "2x",
            Self::X4 => "4x",
        }
    }

    pub(super) const fn zoomed_in(self) -> Option<Self> {
        match self {
            Self::X1 => Some(Self::X2),
            Self::X2 => Some(Self::X4),
            Self::X4 => None,
        }
    }

    pub(super) const fn zoomed_out(self) -> Option<Self> {
        match self {
            Self::X1 => None,
            Self::X2 => Some(Self::X1),
            Self::X4 => Some(Self::X2),
        }
    }
}

/// `x` and `y` are how far the map's top-left lies above and left of the canvas, in
/// screen pixels.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct Viewport {
    zoom: Zoom,
    x: i32,
    y: i32,
}

impl Viewport {
    pub(super) fn zoom(self) -> Zoom {
        self.zoom
    }

    pub(super) fn offset(self) -> (i32, i32) {
        (self.x, self.y)
    }

    pub(super) fn content(self, map: (usize, usize)) -> (i32, i32) {
        let tile = self.zoom.tile_px();
        (map.0 as i32 * tile, map.1 as i32 * tile)
    }

    /// A map smaller than `area` stays at the top-left corner.
    pub(super) fn clamp(&mut self, map: (usize, usize), area: (i32, i32)) {
        let (width, height) = self.content(map);
        self.x = self.x.clamp(0, (width - area.0).max(0));
        self.y = self.y.clamp(0, (height - area.1).max(0));
    }

    pub(super) fn scroll_by(&mut self, dx: i32, dy: i32, map: (usize, usize), area: (i32, i32)) {
        self.x = self.x.saturating_add(dx);
        self.y = self.y.saturating_add(dy);
        self.clamp(map, area);
    }

    pub(super) fn scroll_tiles(
        &mut self,
        columns: i32,
        rows: i32,
        map: (usize, usize),
        area: (i32, i32),
    ) {
        let tile = self.zoom.tile_px();
        self.scroll_by(columns * tile, rows * tile, map, area);
    }

    pub(super) fn set_zoom(&mut self, zoom: Zoom, map: (usize, usize), area: (i32, i32)) {
        let (old, new) = (self.zoom.tile_px(), zoom.tile_px());
        let centre = (
            (self.x + area.0 / 2) * TILE / old,
            (self.y + area.1 / 2) * TILE / old,
        );
        self.zoom = zoom;
        self.x = centre.0 * new / TILE - area.0 / 2;
        self.y = centre.1 * new / TILE - area.1 / 2;
        self.clamp(map, area);
    }

    /// The cell may lie off the map, so a drag that leaves the canvas keeps its direction.
    pub(super) fn cell_at(self, canvas: Rect, at: (i32, i32)) -> (i32, i32) {
        let tile = self.zoom.tile_px();
        (
            (at.0 - canvas.x + self.x).div_euclid(tile),
            (at.1 - canvas.y + self.y).div_euclid(tile),
        )
    }

    pub(super) fn cell_rect(self, canvas: Rect, cell: (i32, i32)) -> Rect {
        let tile = self.zoom.tile_px();
        Rect {
            x: canvas.x + cell.0 * tile - self.x,
            y: canvas.y + cell.1 * tile - self.y,
            width: tile,
            height: tile,
        }
    }

    pub(super) fn visible_cells(
        self,
        map: (usize, usize),
        area: (i32, i32),
    ) -> ((usize, usize), (usize, usize)) {
        let tile = self.zoom.tile_px();
        let first = (
            (self.x / tile).max(0) as usize,
            (self.y / tile).max(0) as usize,
        );
        let last = (
            ((self.x + area.0 - 1) / tile + 1).clamp(0, map.0 as i32) as usize,
            ((self.y + area.1 - 1) / tile + 1).clamp(0, map.1 as i32) as usize,
        );
        (
            first,
            (
                last.0.saturating_sub(first.0),
                last.1.saturating_sub(first.1),
            ),
        )
    }
}

#[cfg(test)]
mod tests;
