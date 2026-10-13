use super::*;
use glaux_machine::obj_palette::ObjBank;
use glaux_machine::palette::Rgb;
use pretty_assertions::assert_eq;

pub(super) fn open_sprites(system: System) -> System {
    press_break(system).update(&click(tab_centre(Tab::Sprites)))
}

/// A master palette whose every entry is distinct and differs from the
/// fake ROM's.
fn distinct_palette() -> Palette {
    Palette::new(std::array::from_fn(|i| {
        let i = i as u8;
        Rgb {
            r: i,
            g: 255 - i,
            b: i ^ 0x5a,
        }
    }))
}

/// OBP bytes mapping every bank's local `n` to master `100 + n`.
fn obp_hundreds() -> [u8; 128] {
    std::array::from_fn(|offset| match offset % 16 {
        0 => 0,
        local => 100 + local as u8,
    })
}

/// Writes every video asset into `pong`: tile 0 solid local 3, tile 1
/// with a single pixel, all tiles on OBP0.
fn write_video_assets(fixture: &Fixture) {
    let dir = fixture.root().join("pong");
    let mut tiles = vec![0u8; 64 * 32];
    tiles[..32].fill(0x33);
    tiles[32] = 0x50;
    std::fs::write(dir.join("sprites.bin"), &tiles).unwrap();
    std::fs::write(dir.join("objattr.bin"), [0u8; 64]).unwrap();
    std::fs::write(dir.join("obp.bin"), obp_hundreds()).unwrap();
    std::fs::write(dir.join("palette.bin"), distinct_palette().to_rgb888()).unwrap();
}

pub(super) fn with_video_assets(fixture: &Fixture) -> System {
    let system = shell_with_cartridge(fixture, "");
    write_video_assets(fixture);
    enter(system, "load pong")
}

#[test]
fn the_workspace_presents_ui_colors_and_exact_cartridge_colors() {
    let fixture = Fixture::new("sys-sprites-palette");
    let rom = fake_rom();
    let system = open_sprites(with_video_assets(&fixture));
    let palette = system.frame_palette(&rom);
    let master = distinct_palette();
    assert_eq!(palette.entries()[..32], rom.palette().entries()[..32]);
    for local in 1..16u8 {
        assert_eq!(
            palette.get(31 + local),
            master.get(100 + local),
            "local {local}"
        );
    }

    let fb = render(&system);
    let canvas = workspace::tests_support::sprite_canvas(&system.workspace);
    let pixel = fb.pixel(canvas.x + 5, canvas.y + 5).unwrap();
    assert_eq!(palette.get(pixel), master.get(103));
}

#[test]
fn reopening_the_workspace_reads_the_assets_again() {
    let fixture = Fixture::new("sys-sprites-reload");
    let rom = fake_rom();
    let system = open_sprites(with_video_assets(&fixture));
    let mut changed = distinct_palette().entries().to_owned();
    changed[103] = glaux_machine::palette::Rgb::from_hex(0x123456);
    std::fs::write(
        fixture.root().join("pong/palette.bin"),
        Palette::new(changed).to_rgb888(),
    )
    .unwrap();
    let before = system.frame_palette(&rom).get(34);
    assert_eq!(before, distinct_palette().get(103));

    let system = press_break(press_break(system));
    assert_eq!(
        system.frame_palette(&rom).get(34),
        glaux_machine::palette::Rgb::from_hex(0x123456)
    );
}

#[test]
fn sprite_editor_state_survives_tab_switches_and_shell_trips() {
    let fixture = Fixture::new("sys-sprites-keep");
    let system = open_sprites(loaded(&fixture));
    assert_eq!(workspace_tab(&system), Tab::Sprites);
    let system = system.update(&[
        InputEvent::Down { select: false },
        InputEvent::Down { select: false },
        InputEvent::Right { select: false },
        InputEvent::Text('a'),
        InputEvent::Text('+'),
        InputEvent::Wheel { delta: -3 },
        InputEvent::WheelHorizontal { delta: -2 },
        InputEvent::Text('g'),
    ]);
    let state = system.workspace.sprites().clone();
    assert_eq!(state.selected(), 33);
    assert!(state.is_advanced());

    // Keys typed in other views do not reach the sprite editor.
    let system = system.update(&[
        InputEvent::NextView,
        InputEvent::Text('a'),
        InputEvent::Down { select: false },
        InputEvent::Wheel { delta: 2 },
    ]);
    assert_eq!(workspace_tab(&system), Tab::Map);
    let system = system.update(&click(tab_centre(Tab::Sprites)));
    assert_eq!(system.workspace.sprites(), &state);

    let system = press_break(enter(press_break(system), "info"));
    assert_eq!(workspace_tab(&system), Tab::Sprites);
    assert_eq!(system.workspace.sprites(), &state);
}

/// Every file under `dir` with its contents and modification time.
fn snapshot(dir: &std::path::Path) -> Vec<(std::path::PathBuf, Vec<u8>, std::time::SystemTime)> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            out.extend(snapshot(&path));
        } else {
            let modified = std::fs::metadata(&path).unwrap().modified().unwrap();
            out.push((path.clone(), std::fs::read(&path).unwrap(), modified));
        }
    }
    out.sort();
    out
}

/// Clicks tile pixel `(x, y)` on the sprite editor's canvas.
fn click_pixel(system: System, x: i32, y: i32) -> System {
    let at = workspace::tests_support::sprite_pixel(&system.workspace, x, y);
    system.update(&click(at))
}

pub(super) fn document(system: &System) -> &SpriteDocument {
    system.sprites.as_ref().expect("a sprite document")
}

/// Opens the sprite editor on assets from `write_video_assets` and
/// draws local 1, the starting color, over pixel (0, 0) of tile 0.
fn edited(fixture: &Fixture) -> System {
    let system = click_pixel(open_sprites(with_video_assets(fixture)), 0, 0);
    assert!(document(&system).is_modified());
    system
}

#[test]
fn sprite_edits_show_at_once_and_survive_tab_switches_and_shell_trips() {
    let fixture = Fixture::new("sys-sprites-edit-keep");
    let rom = fake_rom();
    let system = edited(&fixture);
    let canvas = workspace::tests_support::sprite_canvas(&system.workspace);
    let shown = |system: &System| {
        let pixel = render(system).pixel(canvas.x + 5, canvas.y + 5).unwrap();
        system.frame_palette(&rom).get(pixel)
    };
    assert_eq!(shown(&system), distinct_palette().get(101));

    let system = system.update(&[InputEvent::NextView]);
    let system = system.update(&click(tab_centre(Tab::Sprites)));
    let system = press_break(enter(press_break(system), "info"));
    assert!(is_workspace(&system));
    assert_eq!(document(&system).pixel(0, 0, 0), Some(1));
    assert_eq!(document(&system).pixel(0, 1, 0), Some(3));
    assert_eq!(shown(&system), distinct_palette().get(101));
}

#[test]
fn reopening_keeps_unsaved_edits_over_the_files() {
    let fixture = Fixture::new("sys-sprites-edit-reopen");
    let system = press_break(edited(&fixture));
    std::fs::write(fixture.root().join("pong/sprites.bin"), [0x77; 32]).unwrap();
    let system = press_break(system);
    assert!(modal(&system).is_some(), "the change is not ignored");
    assert_eq!(document(&system).pixel(0, 0, 0), Some(1));
    assert_eq!(document(&system).pixel(0, 1, 0), Some(3));
}

#[test]
fn restoring_every_edit_rereads_the_files_and_unblocks_load() {
    let fixture = Fixture::new("sys-sprites-edit-restore");
    let system = edited(&fixture);
    // Pick local 3 from a neighbour and paint it back.
    let system = system.update(&[InputEvent::Text('i')]);
    let system = click_pixel(system, 1, 0);
    let system = system.update(&[InputEvent::Text('p')]);
    let system = click_pixel(system, 0, 0);
    assert!(!document(&system).is_modified());

    let system = press_break(system);
    std::fs::write(fixture.root().join("pong/sprites.bin"), [0x77; 32]).unwrap();
    let system = press_break(system);
    assert_eq!(document(&system).pixel(0, 0, 0), Some(7));

    let system = enter(press_break(system), "load pong");
    assert_eq!(after(&system, "> load pong"), ["Loaded Pong 0.1.0"]);
}

#[test]
fn unsaved_edits_block_load_new_and_unload_without_discard() {
    let fixture = Fixture::new("sys-sprites-edit-guard");
    let system = press_break(edited(&fixture));
    let before = snapshot(&fixture.root());

    let system = enter(system, "load pong");
    assert_eq!(
        after(&system, "> load pong"),
        [
            "! Pong 0.1.0 has unsaved graphics changes; --discard drops them",
            "Try: load --discard pong",
        ]
    );
    let system = enter(system, "new other --name Other");
    assert_eq!(
        after(&system, "> new other --name Other"),
        [
            "! Pong 0.1.0 has unsaved graphics changes; --discard drops them",
            "Try: new --discard other --name Other",
        ]
    );
    let system = enter(system, "unload");
    assert_eq!(
        after(&system, "> unload"),
        [
            "! Pong 0.1.0 has unsaved graphics changes; --discard drops them",
            "Try: unload --discard",
        ]
    );
    assert_eq!(snapshot(&fixture.root()), before, "new created nothing");
    assert!(document(&system).is_modified());

    let system = enter(system, "load --discard pong");
    assert_eq!(
        after(&system, "> load --discard pong"),
        ["Loaded Pong 0.1.0"]
    );
    assert!(system.sprites.is_none());
    let system = press_break(system);
    assert_eq!(document(&system).pixel(0, 0, 0), Some(3));
}

#[test]
fn unload_and_new_with_discard_drop_the_edits() {
    let fixture = Fixture::new("sys-sprites-edit-discard");
    let system = enter(press_break(edited(&fixture)), "unload --discard");
    assert_eq!(
        after(&system, "> unload --discard"),
        ["Unloaded Pong 0.1.0"]
    );
    assert!(system.sprites.is_none());

    let fixture = Fixture::new("sys-sprites-edit-discard-new");
    let system = enter(press_break(edited(&fixture)), "new other --discard");
    assert!(system.sprites.is_none());
    assert!(fixture.root().join("other").is_dir());
}

#[test]
fn run_uses_unsaved_edits_and_they_survive_the_run() {
    let fixture = Fixture::new("sys-sprites-edit-run");
    let dir = fixture.root().join("pong");
    let system = shell_with_cartridge(&fixture, "function draw() cls(0) spr(0, 10, 10) end");
    std::fs::write(dir.join("sprites.bin"), [0x11; 32]).unwrap();
    let before = snapshot(&dir);
    let system = open_sprites(enter(system, "load pong"));
    let system = click_pixel(system.update(&[InputEvent::Text('e')]), 0, 0);

    let system = ticks(enter(press_break(system), "run"), 1);
    assert_eq!(
        after(&system, "> run"),
        ["Running Pong 0.1.0 with unsaved graphics changes"]
    );
    let fb = render(&system);
    assert_eq!(fb.pixel(10, 10), Some(0), "the erased pixel");
    assert_eq!(fb.pixel(11, 10), Some(1));

    let system = stop_cartridge(system);
    assert!(document(&system).is_modified());
    assert_eq!(document(&system).pixel(0, 0, 0), Some(0));
    assert_eq!(snapshot(&dir), before);
}

#[test]
fn a_cancelled_or_abandoned_drag_never_resumes() {
    let fixture = Fixture::new("sys-sprites-edit-cancel");
    let system = open_sprites(with_video_assets(&fixture));
    let pixel =
        |system: &System, x, y| workspace::tests_support::sprite_pixel(&system.workspace, x, y);
    let button = PointerButton::Primary;

    let at = pixel(&system, 0, 7);
    let system = system.update(&[
        InputEvent::PointerDown {
            button,
            at,
            select: false,
        },
        InputEvent::PointerCancel,
    ]);
    let to = pixel(&system, 7, 7);
    let system = system.update(&[InputEvent::PointerMove(Some(to))]);
    assert_eq!(document(&system).pixel(0, 0, 7), Some(1));
    assert_eq!(document(&system).pixel(0, 1, 7), Some(3));

    // The release of a drag left by closing the workspace never comes.
    let at = pixel(&system, 0, 6);
    let system = press_break(system.update(&[InputEvent::PointerDown {
        button,
        at,
        select: false,
    }]));
    let to = pixel(&system, 7, 6);
    let system = press_break(system).update(&[InputEvent::PointerMove(Some(to))]);
    assert_eq!(document(&system).pixel(0, 0, 6), Some(1));
    assert_eq!(document(&system).pixel(0, 1, 6), Some(3));
}

fn tiles_on_disk(fixture: &Fixture) -> Vec<u8> {
    std::fs::read(fixture.root().join("pong/sprites.bin")).unwrap()
}

fn modal(system: &System) -> Option<(String, Vec<String>)> {
    workspace::tests_support::modal(&system.workspace).map(|(title, labels)| {
        (
            title.to_owned(),
            labels.into_iter().map(str::to_owned).collect(),
        )
    })
}

fn conflict_labels() -> Vec<String> {
    ["OVERWRITE DISK", "DISCARD & RELOAD", "CANCEL"]
        .map(str::to_owned)
        .to_vec()
}

/// An edited system whose `sprites.bin` was then rewritten outside
/// Glaux and whose save attempt opened the conflict dialog.
fn in_conflict(fixture: &Fixture) -> System {
    let system = edited(fixture);
    std::fs::write(fixture.root().join("pong/sprites.bin"), [0x77; 64]).unwrap();
    let system = system.update(&[InputEvent::Save]);
    assert_eq!(
        modal(&system),
        Some(("FILE CONFLICT".to_owned(), conflict_labels()))
    );
    system
}

#[test]
fn ctrl_s_writes_the_tiles_and_undo_still_works_after() {
    let fixture = Fixture::new("sys-sprites-save");
    let system = edited(&fixture).update(&[InputEvent::Save]);
    let saved = tiles_on_disk(&fixture);
    assert_eq!(saved.len(), 64 * 32, "the file keeps its length");
    assert_eq!(saved[0], 0x13);
    assert!(!document(&system).is_modified());
    assert_eq!(
        workspace::tests_support::notice(&system.workspace),
        Some("Saved sprites.bin (2048 bytes)")
    );
    assert_eq!(modal(&system), None);

    let system = system.update(&[InputEvent::Undo]);
    assert!(document(&system).is_modified());
    let system = system.update(&[InputEvent::Save]);
    assert_eq!(tiles_on_disk(&fixture)[0], 0x33);
    let system = system.update(&[InputEvent::Save]);
    assert_eq!(
        workspace::tests_support::notice(&system.workspace),
        Some("Nothing to save")
    );
}

#[test]
fn notices_fade_after_a_few_seconds() {
    let fixture = Fixture::new("sys-sprites-notice");
    let system = edited(&fixture).update(&[InputEvent::Save]);
    let system = ticks(system, 3 * glaux_machine::machine::TICK_RATE as usize);
    assert_eq!(workspace::tests_support::notice(&system.workspace), None);
}

#[test]
fn history_survives_a_trip_to_the_shell_when_the_file_is_unchanged() {
    let fixture = Fixture::new("sys-sprites-history-shell");
    let system = edited(&fixture).update(&[InputEvent::Save]);
    let system = press_break(press_break(system));
    assert!(is_workspace(&system));
    assert_eq!(modal(&system), None);
    let system = system.update(&[InputEvent::Undo]);
    assert_eq!(document(&system).pixel(0, 0, 0), Some(3));
    assert!(document(&system).is_modified());
}

#[test]
fn a_conflict_dialog_takes_all_input_and_escape_cancels_it() {
    let fixture = Fixture::new("sys-sprites-conflict-cancel");
    let system = in_conflict(&fixture);
    let before = document(&system).clone();
    let canvas = workspace::tests_support::sprite_pixel(&system.workspace, 5, 5);
    let mut events = vec![
        InputEvent::Text('e'),
        InputEvent::NextView,
        InputEvent::Undo,
        InputEvent::Save,
    ];
    events.extend(click(canvas));
    events.extend(click(tab_centre(Tab::Code)));
    let system = system.update(&events);
    assert_eq!(document(&system), &before);
    assert_eq!(workspace_tab(&system), Tab::Sprites);
    assert!(modal(&system).is_some());

    let system = press_break(system);
    assert!(
        is_workspace(&system),
        "Escape closed the dialog, not the workspace"
    );
    assert_eq!(modal(&system), None);
    assert_eq!(document(&system), &before);
    assert_eq!(tiles_on_disk(&fixture), [0x77; 64]);

    // Editing works again, and a second Escape leaves.
    let system = click_pixel(system, 2, 0);
    assert_eq!(document(&system).pixel(0, 2, 0), Some(1));
    assert!(!is_workspace(&press_break(system)));
}

#[test]
fn overwrite_disk_writes_the_edits_over_the_outside_change() {
    let fixture = Fixture::new("sys-sprites-conflict-overwrite");
    let system = in_conflict(&fixture);
    let system = system.update(&[
        InputEvent::Up { select: false },
        InputEvent::Up { select: false },
        InputEvent::Enter,
    ]);
    assert_eq!(modal(&system), None);
    let saved = tiles_on_disk(&fixture);
    assert_eq!(saved, document(&system).tiles().as_bytes()[..2048]);
    assert_eq!(saved[0], 0x13);
    assert!(!document(&system).is_modified());
    assert!(document(&system).can_undo());
}

#[test]
fn overwrite_after_a_further_change_asks_again() {
    let fixture = Fixture::new("sys-sprites-conflict-again");
    let system = in_conflict(&fixture);
    std::fs::write(fixture.root().join("pong/sprites.bin"), [0x66; 32]).unwrap();
    let system = system.update(&[
        InputEvent::Up { select: false },
        InputEvent::Up { select: false },
        InputEvent::Enter,
    ]);
    assert_eq!(
        modal(&system),
        Some(("FILE CONFLICT".to_owned(), conflict_labels()))
    );
    assert_eq!(tiles_on_disk(&fixture), [0x66; 32]);
    assert!(document(&system).is_modified());
}

#[test]
fn discard_and_reload_is_chosen_by_pointer_and_resets_history() {
    let fixture = Fixture::new("sys-sprites-conflict-reload");
    let system = in_conflict(&fixture);
    let layout = workspace::tests_support::modal_layout(&system.workspace).unwrap();
    let reload = layout.buttons[1];
    let at = Point::new(
        (reload.x + reload.width / 2) as u16,
        (reload.y + reload.height / 2) as u16,
    );
    let system = system.update(&click(at));
    assert_eq!(modal(&system), None);
    assert_eq!(document(&system).pixel(0, 0, 0), Some(7));
    assert_eq!(document(&system).pixel(1, 0, 0), Some(7));
    assert!(!document(&system).is_modified());
    assert!(!document(&system).can_undo());
    assert_eq!(tiles_on_disk(&fixture), [0x77; 64]);
}

#[test]
fn reopening_over_a_changed_file_asks_before_touching_edits() {
    let fixture = Fixture::new("sys-sprites-reopen-conflict");
    let system = press_break(edited(&fixture));
    std::fs::remove_file(fixture.root().join("pong/sprites.bin")).unwrap();
    let system = press_break(system);
    assert_eq!(
        modal(&system),
        Some(("FILE CONFLICT".to_owned(), conflict_labels()))
    );
    assert!(document(&system).is_modified());
    // Enter on the focused CANCEL keeps the edits.
    let system = system.update(&[InputEvent::Enter]);
    assert_eq!(modal(&system), None);
    assert_eq!(document(&system).pixel(0, 0, 0), Some(1));
}

#[test]
fn a_failed_save_explains_itself_and_keeps_the_edits() {
    let fixture = Fixture::new("sys-sprites-save-fail");
    let system = edited(&fixture);
    let path = fixture.root().join("pong/sprites.bin");
    let mut permissions = std::fs::metadata(&path).unwrap().permissions();
    permissions.set_readonly(true);
    std::fs::set_permissions(&path, permissions.clone()).unwrap();

    let system = system.update(&[InputEvent::Save]);
    assert_eq!(
        modal(&system),
        Some(("SAVE FAILED".to_owned(), vec!["OK".to_owned()]))
    );
    let fb = render(&system);
    let layout = workspace::tests_support::modal_layout(&system.workspace).unwrap();
    assert_eq!(
        fb.pixel(layout.title_bar.x, layout.title_bar.y),
        Some(BACKGROUND),
        "the dialog is drawn over the editor"
    );
    let system = system.update(&[InputEvent::Enter]);
    assert_eq!(modal(&system), None);
    assert!(document(&system).is_modified());
    assert_eq!(tiles_on_disk(&fixture)[0], 0x33);

    #[expect(
        clippy::permissions_set_readonly_false,
        reason = "restores the test file so the fixture can be removed"
    )]
    permissions.set_readonly(false);
    std::fs::set_permissions(&path, permissions).unwrap();
}

#[test]
fn editing_without_saving_never_writes_the_cartridge() {
    let fixture = Fixture::new("sys-sprites-readonly");
    let system = with_video_assets(&fixture);
    let before = snapshot(&fixture.root().join("pong"));

    let mut system = open_sprites(system);
    let keys = [
        InputEvent::Text('a'),
        InputEvent::Text('+'),
        InputEvent::Text('g'),
        InputEvent::Text('-'),
        InputEvent::Down { select: false },
        InputEvent::Right { select: false },
        InputEvent::Wheel { delta: -2 },
        InputEvent::WheelHorizontal { delta: -1 },
        InputEvent::Enter,
        InputEvent::Backspace,
        InputEvent::Delete,
    ];
    for round in 0..2 {
        system = system.update(&keys);
        // Click everywhere below the tab bar but the Save button:
        // canvas, sheet, tools, swatches, bank cells, options buttons and
        // the palette button, and then whatever the palette picker they
        // open shows.
        let save = workspace::tests_support::sprite_save_button(&system.workspace);
        for y in (16..240).step_by(3) {
            let row: Vec<InputEvent> = (0..320)
                .step_by(5)
                .filter(|&x| !save.contains_xy(x, y))
                .flat_map(|x| click(Point::new(x as u16, y as u16)))
                .collect();
            system = system.update(&row);
        }
        system.render(
            &mut Framebuffer::new(),
            &fake_rom(),
            Some(Point::new(40, 60)),
        );
        assert_eq!(workspace_tab(&system), Tab::Sprites, "round {round}");
    }
    if system.workspace.has_modal() {
        system = press_break(system);
        assert!(is_workspace(&system), "Escape closed the picker first");
    }
    let system = press_break(system);
    assert!(!is_workspace(&system));
    assert_eq!(snapshot(&fixture.root().join("pong")), before);
}

#[test]
fn malformed_video_assets_leave_the_workspace_usable() {
    let fixture = Fixture::new("sys-sprites-malformed");
    let rom = fake_rom();
    let system = with_video_assets(&fixture);
    std::fs::write(fixture.root().join("pong/obp.bin"), [1u8, 2, 3]).unwrap();
    let system = open_sprites(system);
    assert_eq!(system.frame_palette(&rom), rom.palette());
    let fb = render(&system);
    assert!(fb.as_bytes().contains(&ERROR), "the error is shown");

    let system = system.update(&[InputEvent::Text('a'), InputEvent::Wheel { delta: -1 }]);
    let system = system.update(&click(tab_centre(Tab::Map)));
    assert_eq!(workspace_tab(&system), Tab::Map);
    render(&system);
    assert!(!is_workspace(&press_break(system)));
}

// Palette changes.

const RED: glaux_machine::palette::Rgb = glaux_machine::palette::Rgb::from_hex(0xe6482e);

/// Sets master 101, which local 1 of every bank resolves to under
/// `obp_hundreds`, to red in the document, as a palette editor would.
fn recolor(mut system: System) -> System {
    let document = system.sprites.as_mut().expect("a sprite document");
    assert!(document.set_master_color(101, RED));
    system
}

/// The frame palette entry the workspace shows master 101 with.
const SLOT_101: u8 = 32;

#[test]
fn palette_changes_show_at_once_and_undo_restores_them() {
    let fixture = Fixture::new("sys-palette-show");
    let rom = fake_rom();
    let system = recolor(open_sprites(with_video_assets(&fixture))).update(&[]);
    assert_eq!(system.frame_palette(&rom).get(SLOT_101), RED);

    let system = system.update(&[InputEvent::Undo]);
    assert_eq!(
        system.frame_palette(&rom).get(SLOT_101),
        distinct_palette().get(101)
    );
    assert!(!document(&system).is_modified());
}

#[test]
fn palette_changes_block_load_survive_trips_and_reach_a_run() {
    let fixture = Fixture::new("sys-palette-run");
    let rom = fake_rom();
    let dir = fixture.root().join("pong");
    let system = recolor(open_sprites(with_video_assets(&fixture)));
    let before = snapshot(&dir);

    let system = enter(press_break(system), "load pong");
    assert_eq!(
        after(&system, "> load pong"),
        [
            "! Pong 0.1.0 has unsaved graphics changes; --discard drops them",
            "Try: load --discard pong",
        ]
    );
    let system = press_break(system);
    assert!(is_workspace(&system));
    assert!(modal(&system).is_none());
    assert_eq!(document(&system).video().palette.get(101), RED);

    let system = ticks(enter(press_break(system), "run"), 1);
    assert_eq!(
        after(&system, "> run"),
        ["Running Pong 0.1.0 with unsaved graphics changes"]
    );
    assert_eq!(system.frame_palette(&rom).get(101), RED);
    assert_eq!(
        system.frame_palette(&rom).get(102),
        distinct_palette().get(102),
        "unmodified colors come from the files"
    );

    let system = stop_cartridge(system);
    assert_eq!(document(&system).video().palette.get(101), RED);
    assert_eq!(snapshot(&dir), before, "nothing was written");
}

#[test]
fn an_outside_palette_change_reloads_beside_unsaved_tile_edits() {
    let fixture = Fixture::new("sys-palette-outside");
    let rom = fake_rom();
    let system = press_break(edited(&fixture));
    let mut outside = distinct_palette();
    outside.set(101, RED);
    std::fs::write(fixture.root().join("pong/palette.bin"), outside.to_rgb888()).unwrap();

    let system = press_break(system);
    assert!(
        modal(&system).is_none(),
        "the palette had no unsaved changes"
    );
    assert_eq!(system.frame_palette(&rom).get(SLOT_101), RED);
    assert_eq!(document(&system).pixel(0, 0, 0), Some(1));
    let system = system.update(&[InputEvent::Undo]);
    assert_eq!(document(&system).pixel(0, 0, 0), Some(3), "tile undo kept");
}

#[test]
fn discard_and_reload_of_a_palette_conflict_keeps_tile_edits() {
    let fixture = Fixture::new("sys-palette-conflict");
    let system = recolor(edited(&fixture));
    let system = press_break(system);
    std::fs::write(fixture.root().join("pong/palette.bin"), [0; 768]).unwrap();

    let system = press_break(system);
    assert_eq!(
        modal(&system),
        Some(("FILE CONFLICT".to_owned(), conflict_labels()))
    );
    let layout = workspace::tests_support::modal_layout(&system.workspace).unwrap();
    let reload = layout.buttons[1];
    let at = Point::new(
        (reload.x + reload.width / 2) as u16,
        (reload.y + reload.height / 2) as u16,
    );
    let system = system.update(&click(at));
    assert!(modal(&system).is_none());
    assert_eq!(
        workspace::tests_support::notice(&system.workspace),
        Some("Reloaded palette.bin from disk")
    );
    assert_eq!(
        document(&system).video().palette.get(101),
        glaux_machine::palette::Rgb::from_hex(0)
    );
    assert_eq!(document(&system).pixel(0, 0, 0), Some(1));
    assert!(document(&system).is_modified(), "the tile edit stays");
}

/// The open dialog's message, its wrapped lines joined by spaces.
fn modal_text(system: &System) -> String {
    let layout = workspace::tests_support::modal_layout(&system.workspace).unwrap();
    let lines: Vec<String> = layout
        .lines
        .into_iter()
        .map(|(line, _)| line)
        .filter(|line| !line.is_empty())
        .collect();
    lines.join(" ")
}

#[test]
fn a_save_that_stops_partway_says_what_was_saved() {
    let fixture = Fixture::new("sys-palette-partial");
    let obp = fixture.root().join("pong/obp.bin");
    let mut system = recolor(open_sprites(with_video_assets(&fixture)));
    let bank = glaux_machine::obj_palette::ObjBank::new(2).unwrap();
    assert!(system.sprites.as_mut().unwrap().set_obj_color(bank, 1, 7));
    let writable = std::fs::metadata(&obp).unwrap().permissions();
    let mut readonly = writable.clone();
    readonly.set_readonly(true);
    std::fs::set_permissions(&obp, readonly).unwrap();

    let system = system.update(&[InputEvent::Save]);
    assert_eq!(
        modal(&system),
        Some(("SAVE FAILED".to_owned(), vec!["OK".to_owned()]))
    );
    assert_eq!(
        modal_text(&system),
        "Saved palette.bin before stopping. obp.bin is read-only. Make it writable, then save \
         again. Your edits are still in memory."
    );

    std::fs::set_permissions(&obp, writable).unwrap();
    let system = system.update(&[InputEvent::Enter, InputEvent::Save]);
    assert_eq!(
        workspace::tests_support::notice(&system.workspace),
        Some("Saved obp.bin (128 bytes)")
    );
    assert!(!document(&system).is_modified());
}

// Choosing a tile's palette.

/// Opens the sprite editor on assets from `write_video_assets` and clicks
/// the palette button.
fn picking(fixture: &Fixture) -> System {
    let system = open_sprites(with_video_assets(fixture));
    let button = workspace::tests_support::sprite_palette_button(&system.workspace);
    let at = Point::new(
        (button.x + button.width / 2) as u16,
        (button.y + button.height / 2) as u16,
    );
    let system = system.update(&click(at));
    assert!(system.workspace.has_modal());
    system
}

fn tile_bank(system: &System, tile: usize) -> u8 {
    document(system).obj().attributes().bank(tile).get()
}

#[test]
fn the_picker_shows_the_tile_in_the_cartridge_colors() {
    let fixture = Fixture::new("sys-picker-colors");
    let rom = fake_rom();
    let system = picking(&fixture);
    let fb = render(&system);
    // Tile 0 is solid local 3, which every bank maps to master 103.
    for bank in 0..8 {
        let preview = workspace::tests_support::picker_preview(&system.workspace, bank);
        let shown = fb.pixel(preview.x + 7, preview.y + 7).unwrap();
        assert_eq!(
            system.frame_palette(&rom).get(shown),
            distinct_palette().get(103),
            "bank {bank}"
        );
    }
}

#[test]
fn escape_and_tab_keys_stay_inside_the_picker() {
    let fixture = Fixture::new("sys-picker-escape");
    let system = picking(&fixture).update(&[
        InputEvent::NextView,
        InputEvent::PreviousView,
        InputEvent::Down { select: false },
    ]);
    assert_eq!(workspace_tab(&system), Tab::Sprites);
    assert!(system.workspace.has_modal());

    let system = press_break(system);
    assert!(is_workspace(&system), "Escape closed the picker only");
    assert!(!system.workspace.has_modal());
    assert_eq!(tile_bank(&system, 0), 0);
    assert!(!document(&system).is_modified());
    assert!(!is_workspace(&press_break(system)));
}

#[test]
fn an_applied_palette_saves_undoes_and_redoes() {
    let fixture = Fixture::new("sys-picker-save");
    let system = picking(&fixture).update(&[InputEvent::Text('4'), InputEvent::Enter]);
    assert!(!system.workspace.has_modal());
    assert_eq!(tile_bank(&system, 0), 4);

    let system = system.update(&[InputEvent::Save]);
    assert_eq!(
        workspace::tests_support::notice(&system.workspace),
        Some("Saved objattr.bin (64 bytes)")
    );
    let attributes = std::fs::read(fixture.root().join("pong/objattr.bin")).unwrap();
    assert_eq!(attributes[..2], [4, 0]);
    assert!(attributes[2..].iter().all(|&b| b == 0));

    let system = system.update(&[InputEvent::Undo]);
    assert_eq!(tile_bank(&system, 0), 0);
    assert!(document(&system).is_modified(), "the file has bank 4");
    let system = system.update(&[InputEvent::Redo]);
    assert_eq!(tile_bank(&system, 0), 4);
    assert!(!document(&system).is_modified());
}

// Editing a color.

/// The frame palette entry EDIT COLOR shows its proposed color in.
const NEW_SLOT: u8 = 255;
/// The frame palette entry the workspace shows OBP0 local 3 in.
const SLOT_LOCAL_3: u8 = 34;

fn centre(rect: workspace::Rect) -> Point {
    Point::new(
        (rect.x + rect.width / 2) as u16,
        (rect.y + rect.height / 2) as u16,
    )
}

/// Opens the sprite editor on the cartridge's assets and double-clicks
/// swatch 3, the color tile 0 is drawn in.
fn editing_color(system: System) -> System {
    let system = open_sprites(system);
    let at = centre(workspace::tests_support::sprite_swatch(
        &system.workspace,
        3,
    ));
    let system = system.update(&click(at)).update(&click(at));
    assert!(system.workspace.has_modal());
    system
}

/// Types `hex` into the open EDIT COLOR dialog's HEX field.
fn type_hex(system: System, hex: &str) -> System {
    let (_, field, _) = workspace::tests_support::color_editor_parts(&system.workspace);
    let typed: Vec<InputEvent> = hex.chars().map(InputEvent::Text).collect();
    system.update(&click(centre(field))).update(&typed)
}

#[test]
fn edit_color_previews_through_the_frame_palette_only() {
    let fixture = Fixture::new("sys-color-preview");
    let rom = fake_rom();
    let system = type_hex(editing_color(with_video_assets(&fixture)), "E6482E");
    let palette = system.frame_palette(&rom);
    assert_eq!(palette.get(NEW_SLOT), RED);
    assert_eq!(palette.get(SLOT_LOCAL_3), distinct_palette().get(103));
    assert_eq!(palette.entries()[..32], rom.palette().entries()[..32]);
    assert!(!document(&system).is_modified());

    // Tile 0 is solid local 3: the preview draws it in the new color.
    let fb = render(&system);
    let (preview, _, _) = workspace::tests_support::color_editor_parts(&system.workspace);
    assert_eq!(fb.pixel(preview.x + 3, preview.y + 3), Some(NEW_SLOT));
}

#[test]
fn an_applied_color_saves_undoes_and_keeps_the_ui_colors() {
    let fixture = Fixture::new("sys-color-apply");
    let rom = fake_rom();
    // Every bank maps local 3 to master 103: choose to change them all.
    let system =
        type_hex(editing_color(with_video_assets(&fixture)), "e6482e").update(&[InputEvent::Enter]);
    let [all, _] = workspace::tests_support::color_editor_scope_rows(&system.workspace);
    let system = system
        .update(&click(centre(all)))
        .update(&[InputEvent::Enter]);
    assert!(!system.workspace.has_modal());
    assert_eq!(document(&system).video().palette.get(103), RED);
    assert_eq!(system.frame_palette(&rom).get(SLOT_LOCAL_3), RED);

    let system = system.update(&[InputEvent::Save]);
    assert_eq!(
        workspace::tests_support::notice(&system.workspace),
        Some("Saved palette.bin (768 bytes)")
    );
    let saved = std::fs::read(fixture.root().join("pong/palette.bin")).unwrap();
    assert_eq!(saved[103 * 3..104 * 3], [RED.r, RED.g, RED.b]);

    let system = system.update(&[InputEvent::Undo]);
    assert_eq!(
        system.frame_palette(&rom).get(SLOT_LOCAL_3),
        distinct_palette().get(103)
    );
    assert!(
        document(&system).is_modified(),
        "the file has the new color"
    );
}

#[test]
fn escape_reverts_typing_then_cancels_without_leaving_the_workspace() {
    let fixture = Fixture::new("sys-color-escape");
    let rom = fake_rom();
    let system = with_video_assets(&fixture);
    let system = type_hex(editing_color(system), "123456");
    let system = press_break(system);
    assert!(
        system.workspace.has_modal(),
        "the first Escape reverts typing"
    );
    assert_eq!(
        workspace::tests_support::color_editor_color(&system.workspace),
        Some(distinct_palette().get(103))
    );
    let system = press_break(system);
    assert!(is_workspace(&system));
    assert!(!system.workspace.has_modal());
    assert!(!document(&system).is_modified() && !document(&system).can_undo());
    assert_eq!(
        system.frame_palette(&rom).get(NEW_SLOT),
        rom.palette().get(NEW_SLOT),
        "the dialog's colors are gone"
    );
}

#[test]
fn editing_a_low_master_never_touches_the_ui_entry_of_that_index() {
    let fixture = Fixture::new("sys-color-low-master");
    let rom = fake_rom();
    let system = shell_with_cartridge(&fixture, "");
    write_video_assets(&fixture);
    let mut obp = obp_hundreds();
    // OBP0 local 3, tile 0's color, resolves to master 5.
    obp[3] = 5;
    std::fs::write(fixture.root().join("pong/obp.bin"), obp).unwrap();
    let system = enter(system, "load pong");

    let system =
        type_hex(editing_color(system), "E6482E").update(&[InputEvent::Enter, InputEvent::Enter]);
    assert_eq!(document(&system).video().palette.get(5), RED);
    let palette = system.frame_palette(&rom);
    assert_eq!(palette.entries()[..32], rom.palette().entries()[..32]);
    assert_eq!(palette.get(SLOT_LOCAL_3), RED);
}

#[test]
fn only_this_palette_keeps_the_other_palettes_and_saves_both_files() {
    let fixture = Fixture::new("sys-color-only-palette");
    let rom = fake_rom();
    // Every bank maps local 3 to master 103; tile 0 is on OBP0.
    let system =
        type_hex(editing_color(with_video_assets(&fixture)), "010203").update(&[InputEvent::Enter]);
    let fb = render(&system);
    let (preview, _, _) = workspace::tests_support::color_editor_parts(&system.workspace);
    assert_eq!(fb.pixel(preview.x + 3, preview.y + 3), Some(NEW_SLOT));
    let system = system.update(&[InputEvent::Enter]);
    assert!(!system.workspace.has_modal());

    let doc = document(&system);
    let palettes = doc.obj().palettes();
    let moved = palettes.resolve(ObjBank::ZERO, 3).unwrap();
    assert_ne!(moved, 103);
    assert_eq!(doc.video().palette.get(moved), Rgb::from_hex(0x010203));
    for bank in 1..8 {
        assert_eq!(palettes.resolve(ObjBank::new(bank).unwrap(), 3), Some(103));
    }
    assert_eq!(doc.video().palette.get(103), distinct_palette().get(103));
    assert_eq!(
        system.frame_palette(&rom).get(SLOT_LOCAL_3),
        Rgb::from_hex(0x010203),
        "the workspace shows tile 0's palette with the new color"
    );

    let system = system.update(&[InputEvent::Save]);
    assert_eq!(
        workspace::tests_support::notice(&system.workspace),
        Some("Saved palette.bin, obp.bin")
    );
    let obp = std::fs::read(fixture.root().join("pong/obp.bin")).unwrap();
    assert_eq!(obp[3], moved);
    assert_eq!(obp[16 + 3], 103);

    let system = system.update(&[InputEvent::Undo]);
    let doc = document(&system);
    assert_eq!(doc.obj().palettes().resolve(ObjBank::ZERO, 3), Some(103));
    assert_eq!(
        system.frame_palette(&rom).get(SLOT_LOCAL_3),
        distinct_palette().get(103),
        "one undo puts both files' changes back"
    );
}
