//! The tools and what they do to a layer.

use glaux_machine::tilemap::{TileEntry, TileLayer};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum Tool {
    #[default]
    Pencil,
    Eraser,
    Fill,
    Select,
    Eyedropper,
    Hand,
}

impl Tool {
    pub(super) const ALL: [Self; 6] = [
        Self::Pencil,
        Self::Eraser,
        Self::Fill,
        Self::Select,
        Self::Eyedropper,
        Self::Hand,
    ];

    pub(super) const fn key(self) -> char {
        match self {
            Self::Pencil => 'p',
            Self::Eraser => 'e',
            Self::Fill => 'f',
            Self::Select => 's',
            Self::Eyedropper => 'i',
            Self::Hand => 'h',
        }
    }

    pub(super) fn from_key(key: char) -> Option<Self> {
        let key = key.to_ascii_lowercase();
        Self::ALL.into_iter().find(|tool| tool.key() == key)
    }

    pub(super) const fn message_id(self) -> &'static str {
        match self {
            Self::Pencil => "maps-tool-pencil",
            Self::Eraser => "maps-tool-eraser",
            Self::Fill => "maps-tool-fill",
            Self::Select => "maps-tool-select",
            Self::Eyedropper => "maps-tool-eyedropper",
            Self::Hand => "maps-tool-hand",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Selection {
    pub(super) left: usize,
    pub(super) top: usize,
    pub(super) right: usize,
    pub(super) bottom: usize,
}

impl Selection {
    /// `None` when the rectangle is wholly outside the map.
    pub(super) fn between(a: (i32, i32), b: (i32, i32), size: (usize, usize)) -> Option<Self> {
        if size.0 == 0 || size.1 == 0 {
            return None;
        }
        let clamp = |value: i32, limit: usize| value.clamp(0, limit as i32 - 1) as usize;
        let (left, right) = (a.0.min(b.0), a.0.max(b.0));
        let (top, bottom) = (a.1.min(b.1), a.1.max(b.1));
        if right < 0 || bottom < 0 || left >= size.0 as i32 || top >= size.1 as i32 {
            return None;
        }
        Some(Self {
            left: clamp(left, size.0),
            top: clamp(top, size.1),
            right: clamp(right, size.0),
            bottom: clamp(bottom, size.1),
        })
    }

    pub(super) fn width(self) -> usize {
        self.right - self.left + 1
    }

    pub(super) fn height(self) -> usize {
        self.bottom - self.top + 1
    }
}

/// The cells joined by their edges to `(x, y)` that hold the same entry, `(x, y)` first.
///
/// Iterative, so filling a 256 x 256 map uses no stack.
pub(super) fn connected(layer: &TileLayer, x: usize, y: usize) -> Vec<(usize, usize)> {
    let Some(target) = layer.get(x, y) else {
        return Vec::new();
    };
    let (width, height) = (layer.width(), layer.height());
    let mut seen = vec![false; layer.cell_count()];
    let mut found = Vec::new();
    let mut pending = vec![(x, y)];
    seen[y * width + x] = true;
    while let Some((x, y)) = pending.pop() {
        found.push((x, y));
        let mut visit = |x: usize, y: usize| {
            let index = y * width + x;
            if !seen[index] && layer.get(x, y) == Some(target) {
                seen[index] = true;
                pending.push((x, y));
            }
        };
        if x > 0 {
            visit(x - 1, y);
        }
        if x + 1 < width {
            visit(x + 1, y);
        }
        if y > 0 {
            visit(x, y - 1);
        }
        if y + 1 < height {
            visit(x, y + 1);
        }
    }
    found
}

pub(super) fn line(from: (i32, i32), to: (i32, i32)) -> impl Iterator<Item = (i32, i32)> {
    super::super::sprites::stroke::line(from, to)
}

pub(super) fn would_change(layer: &TileLayer, x: usize, y: usize, entry: TileEntry) -> bool {
    layer.get(x, y).is_some_and(|current| current != entry)
}

#[cfg(test)]
mod tests;
