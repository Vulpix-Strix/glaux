use super::*;

/// The sprite editor's metrics with the System ROM font, as the workspace
/// lays it out.
pub const SPRITE_METRICS: sprites::Metrics = Layout::new(FontMetrics::SYSTEM).sprite_metrics();

/// The Code view's metrics with the System ROM font, as the workspace
/// lays it out.
pub const CODE_METRICS: code::Metrics = Layout::new(FontMetrics::SYSTEM).code_metrics();

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

/// The open dialog's focused button and message lines.
pub fn modal_details(workspace: &Workspace) -> Option<(usize, Vec<String>)> {
    let dialog = &workspace.modal.as_ref()?.dialog;
    Some((dialog.focus(), dialog.message().to_vec()))
}

/// The label and whether it can be chosen of each item of the Code view's
/// open context menu.
pub fn code_menu_items(workspace: &Workspace) -> Vec<(&'static str, bool)> {
    workspace.code.menu_items()
}

/// Where the open dialog draws its window and buttons.
pub fn modal_layout(workspace: &Workspace) -> Option<dialog::DialogLayout> {
    let layout = workspace.layout;
    let dialog = &workspace.modal.as_ref()?.dialog;
    Some(dialog.layout(
        &workspace.font,
        layout.view_area(),
        layout.cell_w,
        layout.cell_h,
    ))
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

/// Where the sprite editor draws the swatch of local color `local`.
pub fn sprite_swatch(workspace: &Workspace, local: u8) -> Rect {
    workspace.sprites.swatch_rect(local)
}

/// Where the open EDIT COLOR dialog draws its tile preview, its first
/// field (HEX) and its confirm button.
pub fn color_editor_parts(workspace: &Workspace) -> (Rect, Rect, Rect) {
    let layout = workspace.sprites.color_editor_layout();
    (layout.preview, layout.fields[0], layout.apply)
}

/// What the open EDIT COLOR dialog would apply, if one is open.
pub fn color_editor_color(workspace: &Workspace) -> Option<glaux_machine::palette::Rgb> {
    workspace
        .sprites
        .color_editor()
        .map(|editor| editor.color())
}

/// Where the open EDIT COLOR dialog draws its scope rows: all palettes,
/// then only this palette.
pub fn color_editor_scope_rows(workspace: &Workspace) -> [Rect; 2] {
    workspace.sprites.color_editor_layout().scope_rows()
}

/// Where the sprite editor's Import button is.
pub fn sprite_import_button(workspace: &Workspace) -> Rect {
    workspace.sprites.import_button_rect()
}

/// The open IMPORT SPRITES dialog, if any.
pub fn import_preview(workspace: &Workspace) -> Option<&sprites::ImportPreview> {
    workspace.sprites.import_preview()
}

/// Where the open IMPORT SPRITES dialog draws the converted sheet, and its
/// transparency, CANCEL and IMPORT buttons.
pub fn import_preview_parts(workspace: &Workspace) -> (Rect, Rect, Rect, Rect) {
    let layout = workspace.sprites.import_preview_layout();
    (
        layout.preview,
        layout.transparency,
        layout.cancel,
        layout.import,
    )
}
