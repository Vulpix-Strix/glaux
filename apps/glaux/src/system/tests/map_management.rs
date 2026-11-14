//! Choosing, creating, renaming and deleting maps from the Map view, and
//! settling files that changed outside Glaux.

use super::code_tabs::loaded_with;
use super::*;
use crate::cartridge::map::MapMeta;
use crate::system::map_document::MapDocument;
use crate::system::workspace::Rect;
use glaux_machine::system_rom::SystemCursor;
use glaux_machine::tilemap::{BgLayer, TileEntry};
use pretty_assertions::assert_eq;
use std::path::PathBuf;

pub(super) fn map_dir(fixture: &Fixture, id: &str) -> PathBuf {
    fixture.root().join("pong/maps").join(id)
}

/// Writes map `id` named `name`, 2x1 tiles, into `pong`.
pub(super) fn write_map(fixture: &Fixture, id: &str, name: &str) {
    let dir = map_dir(fixture, id);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("map.ron"),
        MapMeta::new(name, 2, 1).unwrap().to_bytes(),
    )
    .unwrap();
}

pub(super) fn centre(rect: Rect) -> Point {
    Point::new(
        (rect.x + rect.width / 2) as u16,
        (rect.y + rect.height / 2) as u16,
    )
}

fn press(system: System, rect: Rect) -> System {
    system.update(&click(centre(rect)))
}

/// Runs the operation `label` the way the view offers it: SAVE with
/// Ctrl+S and the others from the list's last rows.
fn press_button(system: System, label: &str) -> System {
    if label == "SAVE" {
        return system.update(&[InputEvent::Save]);
    }
    let system = press_selector(system);
    let rect = workspace::tests_support::map_button(&system.workspace, label);
    press(system, rect)
}

fn press_selector(system: System) -> System {
    let rect = workspace::tests_support::map_selector(&system.workspace);
    press(system, rect)
}

fn press_row(system: System, slot: usize) -> System {
    let rect = workspace::tests_support::map_row(&system.workspace, slot);
    press(system, rect)
}

fn press_create(system: System) -> System {
    let rect = workspace::tests_support::map_create_button(&system.workspace);
    press(system, rect)
}

fn typed(system: System, text: &str) -> System {
    let events: Vec<InputEvent> = text.chars().map(InputEvent::Text).collect();
    system.update(&events)
}

/// The workspace on the Map view, with the maps listed and one opened.
pub(super) fn open_maps(system: System) -> System {
    let system = press_break(system).update(&click(tab_centre(Tab::Map)));
    system.update(&[])
}

fn open_id(system: &System) -> Option<String> {
    system.maps.as_ref().map(|map| map.id().to_string())
}

pub(super) fn map(system: &mut System) -> &mut MapDocument {
    system.maps.as_mut().unwrap()
}

pub(super) fn paint(system: &mut System) {
    let entry = TileEntry::from_raw(1).unwrap();
    assert!(map(system).set_cell(BgLayer::Bg0, 0, 0, entry));
}

fn modal(system: &System) -> Option<(&str, Vec<&str>)> {
    workspace::tests_support::modal(&system.workspace)
}

pub(super) fn notice(system: &System) -> Option<&str> {
    system.workspace.notice_text()
}

fn up() -> InputEvent {
    InputEvent::Up { select: false }
}

/// Chooses the dialog button `steps` above the focused last one.
fn choose_above(system: System, steps: usize) -> System {
    let mut events = vec![up(); steps];
    events.push(InputEvent::Enter);
    system.update(&events)
}

pub(super) fn disk_name(fixture: &Fixture, id: &str) -> String {
    let bytes = std::fs::read(map_dir(fixture, id).join("map.ron")).unwrap();
    MapMeta::parse(&bytes).unwrap().name
}

#[test]
fn entering_the_view_lists_the_maps_and_opens_the_first() {
    let fixture = Fixture::new("sys-maps-open-first");
    write_map(&fixture, "beta", "Beta");
    write_map(&fixture, "alpha", "Alpha");
    let system = open_maps(loaded_with(&fixture, &[]));
    assert_eq!(workspace_tab(&system), Tab::Map);
    assert_eq!(open_id(&system).as_deref(), Some("alpha"));
}

#[test]
fn the_map_last_open_opens_again_when_the_view_is_revisited() {
    let fixture = Fixture::new("sys-maps-remembered");
    write_map(&fixture, "alpha", "Alpha");
    write_map(&fixture, "beta", "Beta");
    let system = open_maps(loaded_with(&fixture, &[]));
    let system = press_selector(system);
    let system = press_row(system, 1);
    assert_eq!(open_id(&system).as_deref(), Some("beta"));

    // A trip to the shell and back keeps the map, and so does a visit to
    // another view, even though the document is gone.
    let mut system = press_break(press_break(system));
    system.maps = None;
    let system = open_maps_again(system);
    assert_eq!(open_id(&system).as_deref(), Some("beta"));
}

fn open_maps_again(system: System) -> System {
    system
        .update(&click(tab_centre(Tab::Code)))
        .update(&click(tab_centre(Tab::Map)))
        .update(&[])
}

#[test]
fn a_cartridge_without_maps_offers_to_create_one() {
    let fixture = Fixture::new("sys-maps-empty");
    let system = open_maps(loaded_with(&fixture, &[]));
    assert_eq!(open_id(&system), None);

    let system = press_create(system);
    assert_eq!(
        workspace::tests_support::map_form_message(&system.workspace).as_deref(),
        Some("The id names the map's folder and never changes.")
    );
    let system = typed(system, "Over World");
    let system = system.update(&[InputEvent::Enter]);

    assert_eq!(open_id(&system).as_deref(), Some("over-world"));
    assert_eq!(
        workspace::tests_support::map_form_message(&system.workspace),
        None
    );
    let dir = map_dir(&fixture, "over-world");
    let meta = MapMeta::parse(&std::fs::read(dir.join("map.ron")).unwrap()).unwrap();
    assert_eq!(
        (meta.name.as_str(), meta.width, meta.height),
        ("Over World", 40, 30)
    );
    assert_eq!(notice(&system), Some("Created map over-world"));
}

#[test]
fn a_taken_id_is_refused_without_touching_the_existing_map() {
    let fixture = Fixture::new("sys-maps-taken");
    write_map(&fixture, "alpha", "Alpha");
    let system = open_maps(loaded_with(&fixture, &[]));
    let system = press_button(system, "NEW");
    let system = typed(system, "Alpha");
    assert_eq!(
        workspace::tests_support::map_form_message(&system.workspace).as_deref(),
        Some("Map alpha already exists")
    );
    let system = system.update(&[InputEvent::Enter]);
    assert_eq!(disk_name(&fixture, "alpha"), "Alpha");
    assert!(
        workspace::tests_support::map_form_message(&system.workspace).is_some(),
        "the window stays open"
    );

    let system = press_break(system);
    assert_eq!(
        workspace::tests_support::map_form_message(&system.workspace),
        None
    );
    assert!(is_workspace(&system));
}

#[test]
fn creating_beside_unsaved_edits_does_not_replace_the_open_map() {
    let fixture = Fixture::new("sys-maps-create-dirty");
    write_map(&fixture, "alpha", "Alpha");
    let mut system = open_maps(loaded_with(&fixture, &[]));
    paint(&mut system);
    let system = press_button(system, "NEW");
    let system = typed(system, "Fresh").update(&[InputEvent::Enter]);

    assert!(map_dir(&fixture, "fresh").join("map.ron").exists());
    assert_eq!(open_id(&system).as_deref(), Some("alpha"));
    assert!(system.maps.as_ref().unwrap().is_modified());
}

#[test]
fn renaming_a_clean_map_writes_the_name_at_once() {
    let fixture = Fixture::new("sys-maps-rename");
    write_map(&fixture, "alpha", "Alpha");
    let system = open_maps(loaded_with(&fixture, &[]));
    let system = press_button(system, "RENAME");
    let system = typed(system, "Renamed").update(&[InputEvent::Enter]);

    assert_eq!(disk_name(&fixture, "alpha"), "Renamed");
    assert_eq!(
        open_id(&system).as_deref(),
        Some("alpha"),
        "the id never changes"
    );
    assert!(!system.maps.as_ref().unwrap().is_modified());
    assert_eq!(notice(&system), Some("Renamed map to Renamed"));
}

#[test]
fn renaming_beside_other_edits_waits_for_the_save() {
    let fixture = Fixture::new("sys-maps-rename-dirty");
    write_map(&fixture, "alpha", "Alpha");
    let mut system = open_maps(loaded_with(&fixture, &[]));
    paint(&mut system);
    let system = press_button(system, "RENAME");
    let system = typed(system, "Renamed").update(&[InputEvent::Enter]);
    assert_eq!(disk_name(&fixture, "alpha"), "Alpha");
    assert!(system.maps.as_ref().unwrap().is_modified());

    let system = press_button(system, "SAVE");
    assert_eq!(disk_name(&fixture, "alpha"), "Renamed");
    assert_eq!(
        std::fs::read(map_dir(&fixture, "alpha").join("bg0.bin")).unwrap(),
        [1, 0, 0, 0]
    );
    assert!(!system.maps.as_ref().unwrap().is_modified());
}

#[test]
fn undo_takes_back_a_rename_that_was_not_saved() {
    let fixture = Fixture::new("sys-maps-rename-undo");
    write_map(&fixture, "alpha", "Alpha");
    let mut system = open_maps(loaded_with(&fixture, &[]));
    paint(&mut system);
    let system = press_button(system, "RENAME");
    let system = typed(system, "Renamed").update(&[InputEvent::Enter]);
    let system = system.update(&[InputEvent::Undo]);
    assert_eq!(system.maps.as_ref().unwrap().meta().name, "Alpha");
}

#[test]
fn deleting_asks_first_and_cancel_keeps_the_map() {
    let fixture = Fixture::new("sys-maps-delete-cancel");
    write_map(&fixture, "alpha", "Alpha");
    let system = open_maps(loaded_with(&fixture, &[]));
    let system = press_button(system, "DELETE");
    assert_eq!(
        modal(&system),
        Some(("DELETE MAP", vec!["DELETE", "CANCEL"]))
    );

    // CANCEL is focused, so a stray Enter deletes nothing.
    let system = system.update(&[InputEvent::Enter]);
    assert_eq!(modal(&system), None);
    assert!(map_dir(&fixture, "alpha").join("map.ron").exists());
    assert_eq!(open_id(&system).as_deref(), Some("alpha"));
}

#[test]
fn deleting_the_open_map_removes_it_and_opens_the_next() {
    let fixture = Fixture::new("sys-maps-delete");
    write_map(&fixture, "alpha", "Alpha");
    write_map(&fixture, "beta", "Beta");
    let system = open_maps(loaded_with(&fixture, &[]));
    let system = press_button(system, "DELETE");
    let system = choose_above(system, 1);

    assert!(!map_dir(&fixture, "alpha").exists());
    assert!(map_dir(&fixture, "beta").exists());
    assert_eq!(notice(&system), Some("Deleted map alpha"));
    // The view lists again and opens what is left.
    let system = system.update(&[]);
    assert_eq!(open_id(&system).as_deref(), Some("beta"));

    let system = press_button(system, "DELETE");
    let system = choose_above(system, 1).update(&[]);
    assert_eq!(open_id(&system), None);
    let system = press_create(system);
    assert!(workspace::tests_support::map_form_message(&system.workspace).is_some());
}

#[test]
fn a_map_changed_since_the_prompt_is_not_deleted() {
    let fixture = Fixture::new("sys-maps-delete-changed");
    write_map(&fixture, "alpha", "Alpha");
    let system = open_maps(loaded_with(&fixture, &[]));
    let system = press_button(system, "DELETE");
    std::fs::write(map_dir(&fixture, "alpha").join("bg2.bin"), [0, 0, 0, 0]).unwrap();
    let system = choose_above(system, 1);

    assert!(map_dir(&fixture, "alpha").join("map.ron").exists());
    let (title, _) = modal(&system).unwrap();
    assert_eq!(title, "DELETE MAP");
    let message = workspace::tests_support::modal_details(&system.workspace)
        .unwrap()
        .1;
    assert_eq!(
        message,
        ["bg2.bin of map alpha changed outside Glaux, so nothing was deleted"]
    );
}

#[test]
fn foreign_files_in_the_folder_stop_the_delete() {
    let fixture = Fixture::new("sys-maps-delete-foreign");
    write_map(&fixture, "alpha", "Alpha");
    let system = open_maps(loaded_with(&fixture, &[]));
    let system = press_button(system, "DELETE");
    std::fs::write(map_dir(&fixture, "alpha").join("notes.txt"), "mine").unwrap();
    let system = choose_above(system, 1);

    assert!(map_dir(&fixture, "alpha").join("notes.txt").exists());
    assert!(map_dir(&fixture, "alpha").join("map.ron").exists());
    let message = workspace::tests_support::modal_details(&system.workspace)
        .unwrap()
        .1;
    assert_eq!(
        message,
        ["Map alpha holds other files (notes.txt), so nothing was deleted"]
    );
}

#[test]
fn saving_over_an_outside_change_offers_overwrite_reload_or_cancel() {
    let fixture = Fixture::new("sys-maps-conflict-overwrite");
    write_map(&fixture, "alpha", "Alpha");
    let mut system = open_maps(loaded_with(&fixture, &[]));
    paint(&mut system);
    let bg0 = map_dir(&fixture, "alpha").join("bg0.bin");
    std::fs::write(&bg0, [9, 0, 0, 0]).unwrap();

    let system = press_button(system, "SAVE");
    assert_eq!(
        modal(&system),
        Some((
            "FILE CONFLICT",
            vec!["OVERWRITE DISK", "DISCARD & RELOAD", "CANCEL"]
        ))
    );
    assert_eq!(std::fs::read(&bg0).unwrap(), [9, 0, 0, 0]);

    let system = choose_above(system, 2);
    assert_eq!(std::fs::read(&bg0).unwrap(), [1, 0, 0, 0]);
    assert!(!system.maps.as_ref().unwrap().is_modified());
}

#[test]
fn discarding_and_reloading_takes_the_outside_version() {
    let fixture = Fixture::new("sys-maps-conflict-reload");
    write_map(&fixture, "alpha", "Alpha");
    let mut system = open_maps(loaded_with(&fixture, &[]));
    paint(&mut system);
    let bg0 = map_dir(&fixture, "alpha").join("bg0.bin");
    std::fs::write(&bg0, [9, 0, 0, 0]).unwrap();

    let system = press_button(system, "SAVE");
    let system = choose_above(system, 1);
    assert_eq!(std::fs::read(&bg0).unwrap(), [9, 0, 0, 0]);
    let document = system.maps.as_ref().unwrap();
    assert!(!document.is_modified());
    assert_eq!(
        document
            .layer(BgLayer::Bg0)
            .get(0, 0)
            .map(|entry| entry.raw()),
        Some(9)
    );
    assert_eq!(notice(&system), Some("Reloaded map alpha from disk"));
}

#[test]
fn cancelling_a_conflict_keeps_the_edits_and_the_disk() {
    let fixture = Fixture::new("sys-maps-conflict-cancel");
    write_map(&fixture, "alpha", "Alpha");
    let mut system = open_maps(loaded_with(&fixture, &[]));
    paint(&mut system);
    let bg0 = map_dir(&fixture, "alpha").join("bg0.bin");
    std::fs::write(&bg0, [9, 0, 0, 0]).unwrap();

    let system = press_button(system, "SAVE");
    let system = system.update(&[InputEvent::Enter]);
    assert_eq!(modal(&system), None);
    assert_eq!(std::fs::read(&bg0).unwrap(), [9, 0, 0, 0]);
    assert!(system.maps.as_ref().unwrap().is_modified());
}

#[test]
fn an_outside_change_met_on_return_is_settled_when_edits_are_unsaved() {
    let fixture = Fixture::new("sys-maps-return-conflict");
    write_map(&fixture, "alpha", "Alpha");
    let mut system = open_maps(loaded_with(&fixture, &[]));
    paint(&mut system);
    let dir = map_dir(&fixture, "alpha");
    std::fs::write(dir.join("bg0.bin"), [9, 0, 0, 0]).unwrap();

    // Leaving the view and coming back checks the map against its files.
    let system = open_maps_again(system);
    assert_eq!(
        modal(&system),
        Some((
            "FILE CONFLICT",
            vec!["OVERWRITE DISK", "DISCARD & RELOAD", "CANCEL"]
        ))
    );
    assert!(system.maps.as_ref().unwrap().is_modified());
}

#[test]
fn a_clean_map_follows_outside_changes_when_the_view_is_revisited() {
    let fixture = Fixture::new("sys-maps-return-clean");
    write_map(&fixture, "alpha", "Alpha");
    let system = open_maps(loaded_with(&fixture, &[]));
    write_map(&fixture, "alpha", "Changed");

    let system = open_maps_again(system);
    assert_eq!(system.maps.as_ref().unwrap().meta().name, "Changed");
    assert_eq!(modal(&system), None);
}

#[test]
fn switching_maps_with_unsaved_edits_asks_and_each_answer_does_what_it_says() {
    let fixture = Fixture::new("sys-maps-switch");
    write_map(&fixture, "alpha", "Alpha");
    write_map(&fixture, "beta", "Beta");
    let switch = |system: System| press_row(press_selector(system), 1);

    let mut system = open_maps(loaded_with(&fixture, &[]));
    paint(&mut system);
    let system = switch(system);
    assert_eq!(
        modal(&system),
        Some(("UNSAVED CHANGES", vec!["SAVE", "DISCARD", "CANCEL"]))
    );

    // CANCEL stays on the map with its edits.
    let system = system.update(&[InputEvent::Enter]);
    assert_eq!(open_id(&system).as_deref(), Some("alpha"));
    assert!(system.maps.as_ref().unwrap().is_modified());

    // SAVE writes alpha, then opens beta.
    let system = choose_above(switch(system), 2);
    assert_eq!(open_id(&system).as_deref(), Some("beta"));
    assert_eq!(
        std::fs::read(map_dir(&fixture, "alpha").join("bg0.bin")).unwrap(),
        [1, 0, 0, 0]
    );

    // DISCARD drops the edits and opens the other map.
    let mut system = system;
    paint(&mut system);
    let system = choose_above(switch_back(system), 1);
    assert_eq!(open_id(&system).as_deref(), Some("alpha"));
    assert!(!map_dir(&fixture, "beta").join("bg0.bin").exists());
}

fn switch_back(system: System) -> System {
    let system = press_selector(system);
    press_row(system, 0)
}

#[test]
fn a_failed_save_while_switching_keeps_the_map_open() {
    let fixture = Fixture::new("sys-maps-switch-failed");
    write_map(&fixture, "alpha", "Alpha");
    write_map(&fixture, "beta", "Beta");
    let mut system = open_maps(loaded_with(&fixture, &[]));
    paint(&mut system);
    std::fs::write(map_dir(&fixture, "alpha").join("bg0.bin"), [9, 0, 0, 0]).unwrap();

    let system = press_selector(system);
    let system = press_row(system, 1);
    let system = choose_above(system, 2);
    assert_eq!(open_id(&system).as_deref(), Some("alpha"));
    assert_eq!(
        modal(&system).map(|(title, _)| title),
        Some("FILE CONFLICT")
    );
}

#[test]
fn a_map_that_cannot_be_opened_is_listed_with_its_reason_and_skipped() {
    let fixture = Fixture::new("sys-maps-broken");
    let broken = map_dir(&fixture, "aaa-broken");
    std::fs::create_dir_all(&broken).unwrap();
    std::fs::write(broken.join("map.ron"), "(").unwrap();
    write_map(&fixture, "good", "Good");
    let system = open_maps(loaded_with(&fixture, &[]));
    assert_eq!(open_id(&system).as_deref(), Some("good"));

    let system = press_selector(system);
    let system = press_row(system, 0);
    assert_eq!(open_id(&system).as_deref(), Some("good"));
    assert!(workspace::tests_support::map_list_open(&system.workspace));
    assert!(notice(&system).unwrap().starts_with("map.ron line"));
}

#[test]
fn escape_closes_the_list_before_leaving_the_workspace() {
    let fixture = Fixture::new("sys-maps-escape");
    write_map(&fixture, "alpha", "Alpha");
    let system = open_maps(loaded_with(&fixture, &[]));
    let system = press_selector(system);
    assert!(workspace::tests_support::map_list_open(&system.workspace));
    let system = press_break(system);
    assert!(is_workspace(&system));
    assert!(!workspace::tests_support::map_list_open(&system.workspace));
    assert!(!is_workspace(&press_break(system)));
}

#[test]
fn unloading_the_cartridge_forgets_the_maps() {
    let fixture = Fixture::new("sys-maps-unload");
    write_map(&fixture, "alpha", "Alpha");
    let system = open_maps(loaded_with(&fixture, &[]));
    let system = enter(press_break(system), "unload");
    assert!(system.maps.is_none());
    let system = enter(system, "load pong");
    let system = open_maps(system);
    assert_eq!(open_id(&system).as_deref(), Some("alpha"));
}

/// How many pixels of `rect` differ from the empty background.
fn inked(frame: &Framebuffer, rect: Rect) -> usize {
    (rect.y..rect.y + rect.height)
        .flat_map(|y| (rect.x..rect.x + rect.width).map(move |x| (x, y)))
        .filter(|&(x, y)| frame.pixel(x, y) != Some(BACKGROUND))
        .count()
}

#[test]
fn each_state_of_the_view_is_drawn() {
    let fixture = Fixture::new("sys-maps-render");
    write_map(&fixture, "alpha", "Alpha");
    let system = open_maps(loaded_with(&fixture, &[]));
    let selector = workspace::tests_support::map_selector(&system.workspace);
    let canvas = workspace::tests_support::map_canvas(&system.workspace);
    let drawer = workspace::tests_support::map_drawer(&system.workspace);

    let closed = render(&system);
    assert!(inked(&closed, selector) > 0, "the selector");
    assert!(inked(&closed, canvas) > 0, "the open map");
    assert!(inked(&closed, drawer) > 0, "the drawer");

    let system = press_selector(system);
    let row = workspace::tests_support::map_row(&system.workspace, 0);
    let listed = render(&system);
    assert!(inked(&listed, row) > 0, "the menu shows its rows");
    assert!(inked(&listed, canvas) > 0, "the canvas stays behind the menu");
    assert!(inked(&listed, drawer) > 0, "so does the drawer");
}

#[test]
fn the_empty_state_and_the_windows_are_drawn() {
    let fixture = Fixture::new("sys-maps-render-windows");
    let system = open_maps(loaded_with(&fixture, &[]));
    let create = workspace::tests_support::map_create_button(&system.workspace);
    assert!(inked(&render(&system), create) > 0, "CREATE MAP");

    let window = Rect {
        x: 40,
        y: 60,
        width: 240,
        height: 120,
    };
    let before = inked(&render(&system), window);
    let system = press_create(system);
    assert!(
        inked(&render(&system), window) > before,
        "the create window"
    );

    let fixture = Fixture::new("sys-maps-render-delete");
    write_map(&fixture, "alpha", "Alpha");
    let system = press_button(open_maps(loaded_with(&fixture, &[])), "DELETE");
    assert!(inked(&render(&system), window) > 0, "the delete dialog");
}

#[test]
fn the_pointer_is_a_hand_over_controls_only() {
    let fixture = Fixture::new("sys-maps-cursor");
    write_map(&fixture, "alpha", "Alpha");
    let system = open_maps(loaded_with(&fixture, &[]));
    let hover = |at: Point| system.workspace.cursor(at, None, system.maps.as_ref()).0;
    let selector = workspace::tests_support::map_selector(&system.workspace);
    assert_eq!(hover(centre(selector)), SystemCursor::Hand);
    let canvas = workspace::tests_support::map_canvas(&system.workspace);
    assert_eq!(hover(centre(canvas)), SystemCursor::Arrow, "the canvas");

    let drawer = workspace::tests_support::map_drawer(&system.workspace);
    let button = Point::new((drawer.x + 6) as u16, (drawer.y + 6) as u16);
    assert_eq!(
        hover(button),
        SystemCursor::Hand,
        "the drawer's size button"
    );
    let listed = press_selector(system);
    let delete = workspace::tests_support::map_button(&listed.workspace, "DELETE");
    let over_row = listed
        .workspace
        .cursor(centre(delete), None, listed.maps.as_ref());
    assert_eq!(over_row.0, SystemCursor::Hand, "a row of the list");
}

#[test]
fn edit_in_sprites_opens_the_brushs_tile_in_the_background_sprite_editor() {
    use crate::system::sprite_document::TileSet;
    use glaux_input::InputEvent;

    let fixture = Fixture::new("sys-maps-edit-tile");
    write_map(&fixture, "alpha", "Alpha");
    let system = open_maps(loaded_with(&fixture, &[]));
    let system = system.update(&[InputEvent::Tab { reverse: false }]);
    let button = workspace::tests_support::map_edit_button(&system.workspace);
    let system = press(system, button);

    assert_eq!(workspace_tab(&system), Tab::Sprites);
    let sprites = system.workspace.sprites();
    assert_eq!(sprites.tileset(), TileSet::Bg);
    assert_eq!(sprites.selected(), 1, "the brush starts on tile 1");
}
