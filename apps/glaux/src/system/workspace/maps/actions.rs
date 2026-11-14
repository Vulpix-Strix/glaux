//! Layer, tile and clipboard actions of the Map editor.
use glaux_i18n::Arg;
use glaux_input::Point;
use glaux_machine::tilemap::{BgLayer, TileEntry};

use super::editing::{LayerAction, TileAction, map_size};
use super::frame::Frame;
use super::pattern::{Clip, bounds_of, uses};
use super::tools::Selection;
use super::{Confirm, MapEditor, MapOut, Overlay};
use crate::system::map_document::MapDocument;
use crate::system::workspace::context_menu::{ContextMenu, ItemStyle, MenuEntry};
use crate::system::workspace::dialog::Dialog;
use crate::ui_text;

const LAYER_KEYS: [&str; 3] = ["0", "1", "2"];

impl MapEditor {
    pub(super) fn toggle_visible(&mut self, layer: BgLayer) {
        let shown = &mut self.edit.visible[layer.index()];
        *shown = !*shown;
    }

    pub(super) fn toggle_lock(&mut self, layer: BgLayer) {
        let locked = &mut self.edit.locked[layer.index()];
        *locked = !*locked;
    }

    pub(super) fn show_only(&mut self, layer: BgLayer) {
        let alone = BgLayer::ALL
            .iter()
            .all(|other| self.edit.visible[other.index()] == (*other == layer));
        self.edit.visible = if alone {
            [true; 3]
        } else {
            BgLayer::ALL.map(|other| other == layer)
        };
    }

    pub(super) fn select_all(&mut self, document: &MapDocument) {
        let (width, height) = map_size(document);
        self.edit.selection = Selection::between(
            (0, 0),
            (width as i32 - 1, height as i32 - 1),
            (width, height),
        );
    }

    pub(super) fn open_layer_menu(&mut self, layer: BgLayer, at: Point, document: &MapDocument) {
        let index = layer.index();
        let locked = self.edit.locked[index];
        let head = match &document.meta().layer_names[index] {
            Some(name) => format!("{layer}  {name}"),
            None => layer.to_string(),
        };
        let only = ui_text::get_with("maps-layer-menu-only", &[("layer", Arg::Text(&layer.to_string()))]);
        let entries = vec![
            MenuEntry::header(&head),
            MenuEntry::item(&ui_text::get("maps-layer-menu-rename"), Some("F2"), LayerAction::Rename),
            MenuEntry::item(&ui_text::get("maps-layer-menu-edit"), Some(LAYER_KEYS[index]), LayerAction::Edit),
            MenuEntry::Separator,
            MenuEntry::item(&ui_text::get("maps-layer-menu-visible"), Some("V"), LayerAction::ToggleVisible)
                .checked(self.edit.visible[index]),
            MenuEntry::item(&ui_text::get("maps-layer-menu-locked"), Some("L"), LayerAction::ToggleLock)
                .checked(locked),
            MenuEntry::item(&only, Some("Shift+V"), LayerAction::ShowOnly),
            MenuEntry::Separator,
            MenuEntry::item(&ui_text::get("maps-layer-menu-select-all"), Some("Ctrl+A"), LayerAction::SelectAll),
            MenuEntry::Separator,
            MenuEntry::when(
                !locked && !document.layer(layer).is_blank(),
                &ui_text::get("maps-layer-menu-clear"),
                None,
                LayerAction::Clear,
            )
            .styled(ItemStyle::Danger),
        ];
        self.overlay = Some(Overlay::LayerMenu(layer, ContextMenu::new(entries, at)));
    }

    pub(super) fn run_layer_action(
        &mut self,
        layer: BgLayer,
        action: LayerAction,
        document: Option<&MapDocument>,
    ) {
        match action {
            LayerAction::Edit => self.edit.layer = layer,
            LayerAction::Rename => {
                self.edit.layer = layer;
                if let Some(document) = document {
                    self.open_layer_rename(document);
                }
            }
            LayerAction::ToggleVisible => self.toggle_visible(layer),
            LayerAction::ToggleLock => self.toggle_lock(layer),
            LayerAction::ShowOnly => self.show_only(layer),
            LayerAction::SelectAll => {
                self.edit.layer = layer;
                if let Some(document) = document {
                    self.select_all(document);
                }
            }
            LayerAction::Clear => {
                self.edit.layer = layer;
                if let Some(document) = document {
                    self.confirm_clear(layer, document);
                }
            }
        }
    }

    pub(super) fn open_tile_menu(&mut self, tile: usize, at: Point, document: &MapDocument) {
        let Ok(tile) = u16::try_from(tile) else {
            return;
        };
        let layer = self.edit.layer;
        let used = uses(document.layer(layer), tile) > 0;
        let locked = self.edit.locked[layer.index()];
        let brush = self.edit.brush.tile();
        let select = ui_text::get_with("maps-tile-menu-select", &[("layer", Arg::Text(&layer.to_string()))]);
        let entries = vec![
            MenuEntry::header(&ui_text::get_with(
                "maps-tile-menu-title",
                &[("tile", Arg::Text(&format!("{tile:04}")))],
            )),
            MenuEntry::when(tile != 0, &ui_text::get("maps-tile-menu-brush"), Some("Click"), TileAction::UseAsBrush),
            MenuEntry::item(&ui_text::get("maps-tile-menu-edit"), Some("F2"), TileAction::EditInSprites),
            MenuEntry::Separator,
            MenuEntry::when(used, &select, None, TileAction::SelectOnLayer),
            MenuEntry::when(
                used && !locked && brush != 0 && brush != tile,
                &ui_text::get("maps-tile-menu-replace"),
                None,
                TileAction::Replace,
            ),
        ];
        self.overlay = Some(Overlay::TileMenu(tile, ContextMenu::new(entries, at)));
    }

    pub(super) fn run_tile_action(
        &mut self,
        tile: u16,
        action: TileAction,
        document: Option<&MapDocument>,
    ) -> Option<MapOut> {
        match action {
            TileAction::UseAsBrush => self.pick_tile(usize::from(tile)),
            TileAction::EditInSprites => self.edit_tile_out(usize::from(tile)),
            TileAction::SelectOnLayer => {
                let layer = document?.layer(self.edit.layer);
                self.edit.selection = bounds_of(layer, tile);
                None
            }
            TileAction::Replace => {
                self.confirm_replace(tile, document?);
                None
            }
        }
    }

    pub(super) fn edit_tile_out(&self, tile: usize) -> Option<MapOut> {
        let tile = u16::try_from(tile).ok()?;
        Some(MapOut::EditTile {
            tile,
            bank: self.edit.brush.bank(),
        })
    }

    fn confirm_replace(&mut self, from: u16, document: &MapDocument) {
        let to = self.edit.brush.tile();
        let layer = self.edit.layer;
        let count = uses(document.layer(layer), from);
        let message = ui_text::get_with(
            "maps-replace-message",
            &[
                ("from", Arg::Text(&format!("{from:04}"))),
                ("to", Arg::Text(&format!("{to:04}"))),
                ("layer", Arg::Text(&layer.to_string())),
                ("count", Arg::Number(count as f64)),
            ],
        );
        let dialog = Dialog::new(
            ui_text::get("maps-replace-title"),
            vec![message],
            vec![
                (&ui_text::get("maps-replace-confirm"), true),
                (&ui_text::get("maps-cancel"), false),
            ],
            1,
            Some(false),
        );
        self.overlay = Some(Overlay::Confirm(Confirm::Replace { from, to }, dialog));
    }

    fn confirm_clear(&mut self, layer: BgLayer, document: &MapDocument) {
        let name = document.meta().layer_names[layer.index()]
            .as_deref()
            .map_or_else(|| layer.to_string(), |name| format!("{layer} {name}"));
        let message = ui_text::get_with("maps-clear-message", &[("layer", Arg::Text(&name))]);
        let dialog = Dialog::new(
            ui_text::get("maps-clear-title"),
            vec![message],
            vec![
                (&ui_text::get("maps-clear-confirm"), true),
                (&ui_text::get("maps-cancel"), false),
            ],
            1,
            Some(false),
        )
        .destructive(0);
        self.overlay = Some(Overlay::Confirm(Confirm::Clear(layer), dialog));
    }

    pub(super) fn run_confirm(&mut self, confirm: Confirm, document: &mut MapDocument) -> Option<MapOut> {
        if self.edit.locked[self.edit.layer.index()] {
            return Some(self.locked_notice());
        }
        let layer = document.layer(self.edit.layer);
        let mut changes = Vec::new();
        for y in 0..layer.height() {
            for x in 0..layer.width() {
                let Some(entry) = layer.get(x, y) else { continue };
                let next = match confirm {
                    Confirm::Replace { from, to } if entry.tile() == from => TileEntry::new(
                        to,
                        entry.flip_x(),
                        entry.flip_y(),
                        entry.priority(),
                        entry.bank(),
                    ),
                    Confirm::Clear(_) if !entry.is_empty() => Some(TileEntry::EMPTY),
                    _ => None,
                };
                changes.extend(next.map(|next| (x, y, next)));
            }
        }
        document.begin_edit();
        for (x, y, entry) in changes {
            self.write(document, (x as i32, y as i32), entry);
        }
        document.end_edit();
        None
    }

    pub(super) fn copy(&mut self, document: &mut MapDocument, cut: bool) -> Option<MapOut> {
        let selection = self.edit.selection?;
        if cut && self.edit.locked[self.edit.layer.index()] {
            return Some(self.locked_notice());
        }
        let clip = Clip::copy(document.layer(self.edit.layer), selection);
        let (width, height) = (clip.width, clip.height);
        if cut {
            document.begin_edit();
            for ((dx, dy), _) in clip.cells() {
                let cell = ((selection.left + dx) as i32, (selection.top + dy) as i32);
                self.write(document, cell, TileEntry::EMPTY);
            }
            document.end_edit();
        }
        self.edit.clipboard = Some(clip);
        Some(MapOut::Notice(ui_text::get_with(
            if cut { "maps-cut" } else { "maps-copied" },
            &[
                ("width", Arg::Number(width as f64)),
                ("height", Arg::Number(height as f64)),
            ],
        )))
    }

    pub(super) fn paste_clip(&mut self, frame: &Frame, document: &mut MapDocument) -> Option<MapOut> {
        let Some(clip) = self.edit.clipboard.clone() else {
            return Some(MapOut::Notice(ui_text::get("maps-nothing-to-paste")));
        };
        if self.edit.locked[self.edit.layer.index()] {
            return Some(self.locked_notice());
        }
        let size = map_size(document);
        let view = self.viewport(frame, document);
        let target = match (self.edit.pointer.filter(|at| frame.canvas.contains(*at)), self.edit.selection) {
            (Some(at), _) => view.cell_at(frame.canvas, (i32::from(at.x), i32::from(at.y))),
            (None, Some(selection)) => (selection.left as i32, selection.top as i32),
            (None, None) => {
                let (first, _) = view.visible_cells(size, (frame.canvas.width, frame.canvas.height));
                (first.0 as i32, first.1 as i32)
            }
        };
        document.begin_edit();
        for ((dx, dy), entry) in clip.cells() {
            self.write(document, (target.0 + dx as i32, target.1 + dy as i32), entry);
        }
        document.end_edit();
        let far = (target.0 + clip.width as i32 - 1, target.1 + clip.height as i32 - 1);
        self.edit.selection = Selection::between(target, far, size);
        None
    }
}

#[cfg(test)]
mod tests;
