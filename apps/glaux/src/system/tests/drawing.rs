use super::*;
use pretty_assertions::assert_eq;

#[test]
fn a_cartridge_that_has_not_drawn_shows_index_zero() {
    let fixture = Fixture::new("sys-draw-blank");
    let system = started(&fixture, r#"function tick() log.info("x") end"#);
    assert_frame_eq!(render(&system), filled(0));
    let system = ticks(system, 2);
    assert!(is_running(&system));
    assert_frame_eq!(render(&system), filled(0));
}

#[test]
fn a_missing_sprite_asset_starts_with_an_empty_bank() {
    let fixture = Fixture::new("sys-draw-no-sprites");
    let system = ticks(
        started(&fixture, "function draw() cls(6) spr(0, 10, 10) end"),
        1,
    );
    assert!(is_running(&system));
    assert_frame_eq!(render(&system), filled(6));
}

#[test]
fn later_commands_draw_over_earlier_ones() {
    let fixture = Fixture::new("sys-draw-order");
    let system = ticks(
        started(&fixture, "function draw() cls(1) pset(5, 5, 2) end"),
        1,
    );
    let mut expected = filled(1);
    expected.set_pixel(5, 5, 2);
    assert_frame_eq!(render(&system), expected);
}

#[test]
fn sprites_render_in_draw_command_order() {
    let fixture = Fixture::new("sys-draw-sprite-order");
    let dir = fixture.root().join("pong");
    let system = shell_with_cartridge(
        &fixture,
        r#"function draw() cls(9) spr(0, 0, 0) pset(0, 0, 7) print("A", 4, 0, 8) end"#,
    );
    std::fs::write(dir.join("sprites.bin"), [0x22; 32]).unwrap();
    let system = ticks(enter(enter(system, "load pong"), "run"), 1);
    let fb = render(&system);

    assert_eq!(fb.pixel(0, 0), Some(7), "pset follows sprite");
    assert_eq!(fb.pixel(4, 0), Some(8), "print follows sprite");
    assert_eq!(fb.pixel(2, 2), Some(2));
    assert_eq!(fb.pixel(3, 0), Some(2), "sprite follows cls");
    assert_eq!(fb.pixel(7, 7), Some(2));
    assert_eq!(fb.pixel(8, 0), Some(9));
}

#[test]
fn sprite_blocks_compose_grid_tiles_in_draw_command_order() {
    let fixture = Fixture::new("sys-draw-sprite-block");
    let dir = fixture.root().join("pong");
    let system = shell_with_cartridge(
        &fixture,
        "function draw() cls(9) spr(1, 0, 0, 2, 2) pset(0, 0, 7) end",
    );
    let mut bytes = vec![0; 18 * 32];
    for (tile, fill) in [(1, 0x11), (2, 0x22), (17, 0x33), (16, 0x44)] {
        bytes[tile * 32..(tile + 1) * 32].fill(fill);
    }
    std::fs::write(dir.join("sprites.bin"), bytes).unwrap();
    let system = ticks(enter(enter(system, "load pong"), "run"), 1);
    let fb = render(&system);

    assert_eq!(fb.pixel(0, 0), Some(7), "pset follows sprite");
    assert_eq!(fb.pixel(1, 0), Some(1));
    assert_eq!(fb.pixel(8, 7), Some(2));
    assert_eq!(
        fb.pixel(0, 8),
        Some(3),
        "second row starts one grid row down"
    );
    assert_eq!(fb.pixel(15, 15), Some(9), "tile 18 is transparent");
    assert_eq!(fb.pixel(16, 0), Some(9));
}

#[test]
fn sprite_flips_and_palette_banks_reach_the_rendered_frame() {
    let fixture = Fixture::new("sys-draw-sprite-flip");
    let system = shell_with_cartridge(
        &fixture,
        "function draw() cls(9) spr(0, 0, 0) spr(0, 8, 0, 1, 1, true) \
         spr(0, 16, 0, 1, 1, false, true) spr(0, 24, 0, 1, 1, true, true) \
         spr(0, 32, 0, 1, 1, false, false, 2) end",
    );
    // Tile 0 has a single opaque pixel, local index 1 at its top-left.
    let mut tile = [0; 32];
    tile[0] = 0x10;
    std::fs::write(fixture.root().join("pong/sprites.bin"), tile).unwrap();
    let system = ticks(enter(enter(system, "load pong"), "run"), 1);

    // Linear palettes map local 1 to master 1 in bank 0 and to 33 in bank 2.
    let mut expected = filled(9);
    for (x, y, color) in [(0, 0, 1), (15, 0, 1), (16, 7, 1), (31, 7, 1), (32, 0, 33)] {
        expected.set_pixel(x, y, color);
    }
    assert_frame_eq!(render(&system), expected);
}

#[test]
fn a_fresh_run_reloads_object_tiles() {
    let fixture = Fixture::new("sys-draw-sprite-reload");
    let dir = fixture.root().join("pong");
    let system = shell_with_cartridge(&fixture, "function draw() cls(0) spr(0, 10, 10) end");
    std::fs::write(dir.join("sprites.bin"), [0x11; 32]).unwrap();
    let system = ticks(enter(enter(system, "load pong"), "run"), 1);
    assert_eq!(render(&system).pixel(10, 10), Some(1));

    let system = stop_cartridge(system);
    std::fs::write(dir.join("sprites.bin"), [0x22; 32]).unwrap();
    let system = ticks(enter(system, "run"), 1);
    assert_eq!(render(&system).pixel(10, 10), Some(2));
}

#[test]
fn screen_text_uses_the_rom_font() {
    let fixture = Fixture::new("sys-draw-text");
    let system = ticks(
        started(
            &fixture,
            r#"function draw() cls(1) print("AB", 10, 20, 7) end"#,
        ),
        1,
    );
    let mut expected = filled(1);
    draw_text(&mut expected, fake_rom().font(), "AB", 10, 20, 7);
    assert_frame_eq!(render(&system), expected);
    // The fake font draws 3×3 blocks in 4-pixel cells.
    assert_eq!(expected.pixel(12, 22), Some(7));
    assert_eq!(expected.pixel(13, 20), Some(1));
    assert_eq!(expected.pixel(14, 20), Some(7));
}

#[test]
fn the_frame_persists_until_the_next_draw_replaces_it() {
    let fixture = Fixture::new("sys-draw-persist");
    let script = "local n = 0 function tick() n += 1 end function draw() cls(n) end";
    let system = ticks(started(&fixture, script), 1);
    let rom = fake_rom();
    for _ in 0..3 {
        let mut fb = filled(200);
        system.render(&mut fb, &rom, None);
        assert_frame_eq!(fb, filled(1));
    }
    let system = ticks(system, 1);
    assert_frame_eq!(render(&system), filled(2));
}

#[test]
fn stopping_discards_the_frame_and_shows_the_shell() {
    let fixture = Fixture::new("sys-draw-stop");
    let draws = "function draw() cls(1) end";
    let stopped = [
        stop_cartridge(ticks(started(&fixture, draws), 1)),
        ticks(
            started(
                &fixture,
                "local n = 0 function draw() n += 1 cls(1) if n == 2 then error(\"x\") end end",
            ),
            2,
        ),
    ];
    for system in stopped {
        assert_eq!(loaded_name(&system), Some("Pong"));
        let fb = render(&system);
        assert_eq!(fb.pixel(0, 0), Some(BACKGROUND));
        assert!(fb.as_bytes().iter().all(|&b| b != 1));
    }
}

#[test]
fn logged_levels_reach_the_shell() {
    let fixture = Fixture::new("sys-log");
    let script = r#"function tick() log.trace("t") log.warn("w") end"#;
    let system = press_break(ticks(started(&fixture, script), 1));
    assert_eq!(
        after(&system, "> run"),
        ["Running Pong 0.1.0", "t", "w", "Paused Pong 0.1.0"]
    );
}
