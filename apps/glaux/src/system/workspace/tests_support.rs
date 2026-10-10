use super::*;

/// Where `tab` sits in the bar for a font with `metrics`.
pub fn tab_rect(tab: Tab, metrics: FontMetrics) -> Rect {
    Layout::new(metrics).tabs()[tab.index()].1
}

/// Top-left corner of the tab name hint's first character cell.
pub fn hint_origin(metrics: FontMetrics) -> (i32, i32) {
    let layout = Layout::new(metrics);
    (layout.hint_x(), layout.text_y())
}

/// Where the sprite editor draws the enlarged tile.
pub fn sprite_canvas(workspace: &Workspace) -> Rect {
    workspace.sprites.canvas_rect()
}

/// The open dialog's title and button labels.
pub fn modal(workspace: &Workspace) -> Option<(&str, Vec<&str>)> {
    let dialog = &workspace.modal.as_ref()?.dialog;
    Some((dialog.title(), dialog.labels()))
}

/// Where the open dialog draws its window and buttons.
pub fn modal_layout(workspace: &Workspace) -> Option<dialog::DialogLayout> {
    let layout = workspace.layout;
    let dialog = &workspace.modal.as_ref()?.dialog;
    Some(dialog.layout(layout.view_area(), layout.cell_w, layout.cell_h))
}

/// The status bar notice, if one is showing.
pub fn notice(workspace: &Workspace) -> Option<&str> {
    workspace.notice.as_ref().map(|notice| notice.text.as_str())
}

/// Where the sprite editor's Save button is.
pub fn sprite_save_button(workspace: &Workspace) -> Rect {
    workspace.sprites.save_button_rect()
}

/// The middle of tile pixel `(x, y)` on the sprite editor's canvas.
pub fn sprite_pixel(workspace: &Workspace, x: i32, y: i32) -> Point {
    let canvas = sprite_canvas(workspace);
    let zoom = canvas.width / glaux_machine::sprite::OBJ_TILE_WIDTH as i32;
    Point::new(
        (canvas.x + x * zoom + zoom / 2) as u16,
        (canvas.y + y * zoom + zoom / 2) as u16,
    )
}

/// The leftmost column the cartridge name may use.
pub fn name_start(metrics: FontMetrics) -> i32 {
    let layout = Layout::new(metrics);
    WIDTH as i32 - NAME_PAD - layout.name_cells() as i32 * layout.cell_w
}

/// Where the sprite editor's palette button is.
pub fn sprite_palette_button(workspace: &Workspace) -> Rect {
    workspace.sprites.palette_button_rect()
}

/// Where the open palette picker shows the tile through `bank`.
pub fn picker_preview(workspace: &Workspace, bank: usize) -> Rect {
    workspace.sprites.picker_preview_rect(bank)
}
