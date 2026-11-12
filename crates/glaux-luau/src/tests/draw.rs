//! Drawing functions and frame publication.

use super::*;
use glaux_machine::framebuffer::Framebuffer;
use glaux_machine::obj_palette::ObjBank;
use pretty_assertions::assert_eq;

fn pixel(x: i32, y: i32, color: u8) -> DrawCommand {
    DrawCommand::Pixel { x, y, color }
}

fn rect(x: i32, y: i32, width: i32, height: i32, color: u8) -> DrawCommand {
    DrawCommand::Rect {
        x,
        y,
        width,
        height,
        color,
    }
}

fn sprite(tile: u16, x: i32, y: i32) -> DrawCommand {
    sprite_block(tile, x, y, 1, 1)
}

fn sprite_block(tile: u16, x: i32, y: i32, width: u8, height: u8) -> DrawCommand {
    sprite_flipped(tile, x, y, width, height, false, false)
}

fn sprite_flipped(
    tile: u16,
    x: i32,
    y: i32,
    width: u8,
    height: u8,
    flip_x: bool,
    flip_y: bool,
) -> DrawCommand {
    DrawCommand::Sprite {
        tile,
        x,
        y,
        width,
        height,
        flip_x,
        flip_y,
        palette: None,
    }
}

fn sprite_with_palette(tile: u16, width: u8, palette: u8) -> DrawCommand {
    DrawCommand::Sprite {
        tile,
        x: 0,
        y: 0,
        width,
        height: 1,
        flip_x: false,
        flip_y: false,
        palette: ObjBank::new(palette),
    }
}

/// The framebuffer of the test hardware after one tick of a script whose
/// `draw()` body is `body`.
fn drawn(body: &str) -> Framebuffer {
    let mut runtime = started(&format!("function draw() {body} end"));
    runtime.tick(NO_INPUT).unwrap();
    runtime.scratch().framebuffer.clone()
}

/// The framebuffer that executing `commands` on fresh test hardware gives.
fn after(commands: &[DrawCommand]) -> Framebuffer {
    let mut hardware = hardware();
    for command in commands {
        command.execute(&mut hardware, &font());
    }
    hardware.framebuffer
}

/// Asserts that `body` draws exactly what `commands` do. What they draw must
/// show on screen, so that two drawings cannot match by both being blank.
#[track_caller]
fn assert_drew(body: &str, commands: &[DrawCommand]) {
    let expected = after(commands);
    assert!(expected != Framebuffer::new(), "{commands:?} draw nothing");
    assert!(
        drawn(body) == expected,
        "`{body}` did not draw {commands:?}"
    );
}

/// The error of the first tick of `draw()` `body`, and what it had drawn
/// by then.
fn draw_failure(body: &str) -> (String, Framebuffer) {
    let mut runtime = started(&format!("function draw() {body} end"));
    match runtime.tick(NO_INPUT) {
        Err(RuntimeError {
            phase: Phase::Draw,
            error: ScriptError::Runtime(message),
        }) => (message, runtime.scratch().framebuffer.clone()),
        other => panic!("expected a runtime error in draw, got {other:?}"),
    }
}

/// The runtime error raised by the first tick of `draw()` `body`.
fn draw_error(body: &str) -> String {
    draw_failure(body).0
}

#[test]
fn the_framebuffer_starts_blank() {
    assert!(started("").scratch().framebuffer == Framebuffer::new());
}

#[test]
fn draw_functions_draw_in_call_order() {
    assert_drew(
        r#"cls(1) rectfill(1, 2, 3, 4, 9) spr(3, 7, 8) pset(5, 5, 2) rectfill(0, 0, 8, 8, 6) print("HI", 10, 20, 16)"#,
        &[
            DrawCommand::Clear(1),
            rect(1, 2, 3, 4, 9),
            sprite(3, 7, 8),
            pixel(5, 5, 2),
            rect(0, 0, 8, 8, 6),
            DrawCommand::Text {
                text: "HI".into(),
                x: 10,
                y: 20,
                color: 16,
            },
        ],
    );
    // The last writer of a pixel wins, and a later `cls` erases everything.
    assert_eq!(drawn("pset(1, 1, 2) pset(1, 1, 3)").pixel(1, 1), Some(3));
    assert_eq!(drawn("pset(1, 1, 3) pset(1, 1, 2)").pixel(1, 1), Some(2));
    assert!(drawn("rectfill(0, 0, 9, 9, 5) cls(0)") == Framebuffer::new());
}

#[test]
fn print_converts_text_with_the_original_tostring() {
    assert_drew(
        r#"tostring = function() return "x" end print(42, 0, 0, 1)"#,
        &[DrawCommand::Text {
            text: "42".into(),
            x: 0,
            y: 0,
            color: 1,
        }],
    );
}

#[test]
fn each_tick_draws_on_a_framebuffer_cleared_to_the_backdrop() {
    let mut runtime = started(
        r#"
        local n = 0
        function draw() n += 1 for i = 1, n do pset(i, 0, n) end end
        "#,
    );
    runtime.tick(NO_INPUT).unwrap();
    assert!(runtime.scratch().framebuffer == after(&[pixel(1, 0, 1)]));
    runtime.tick(NO_INPUT).unwrap();
    assert!(runtime.scratch().framebuffer == after(&[pixel(1, 0, 2), pixel(2, 0, 2)]));
}

#[test]
fn a_draw_that_draws_nothing_leaves_the_backdrop() {
    let mut runtime =
        started("local first = true function draw() if first then cls(1) end first = false end");
    runtime.tick(NO_INPUT).unwrap();
    assert!(runtime.scratch().framebuffer == after(&[DrawCommand::Clear(1)]));
    runtime.tick(NO_INPUT).unwrap();
    assert!(runtime.scratch().framebuffer == Framebuffer::new());
}

#[test]
fn a_failing_draw_leaves_what_it_drew_before_failing() {
    // A runtime error, the interrupt budget and the draw-cost cap. The
    // command that exceeds the cap is not drawn.
    for failure in [
        r#"error("boom")"#,
        "while true do end",
        "spr(0, 0, 0, 16, 64)",
    ] {
        let mut runtime = started(&format!(
            "function draw() cls(2) pset(1, 1, 1) {failure} end"
        ));
        assert!(runtime.tick(NO_INPUT).is_err(), "{failure}");
        assert!(
            runtime.scratch().framebuffer == after(&[DrawCommand::Clear(2), pixel(1, 1, 1)]),
            "{failure}"
        );
    }
}

#[test]
fn a_failing_tick_does_not_reach_draw() {
    let mut runtime = started(
        r#"
        local n = 0
        function tick() n += 1 if n == 2 then error("x") end end
        function draw() cls(n) end
        "#,
    );
    runtime.tick(NO_INPUT).unwrap();
    assert!(runtime.tick(NO_INPUT).is_err());
    assert!(runtime.scratch().framebuffer == after(&[DrawCommand::Clear(1)]));
}

#[test]
fn removing_draw_leaves_only_the_backdrop() {
    let mut runtime = started("function draw() cls(4) draw = nil end");
    runtime.tick(NO_INPUT).unwrap();
    assert!(runtime.scratch().framebuffer == after(&[DrawCommand::Clear(4)]));
    runtime.tick(NO_INPUT).unwrap();
    assert!(runtime.scratch().framebuffer == Framebuffer::new());
}

#[test]
fn draw_functions_fail_outside_draw() {
    for call in [
        "cls(1)",
        "pset(0, 0, 1)",
        "rectfill(0, 0, 1, 1, 1)",
        "spr(0, 0, 0)",
        r#"print("x", 0, 0, 1)"#,
    ] {
        let name = &call[..call.find('(').unwrap()];
        let expected = format!("'{name}' can only be called during draw()");

        let message = runtime_message(call);
        assert_eq!(message, format!("main.luau:1: {expected}"));
        let failure = start_failure(&format!("function boot() {call} end"));
        assert_eq!(failure.error.phase, Phase::Boot);

        let mut runtime = started(&format!("function tick() {call} end"));
        let err = runtime.tick(NO_INPUT).unwrap_err();
        assert_eq!(err.phase, Phase::Tick);
        assert!(
            matches!(&err.error, ScriptError::Runtime(m) if m.ends_with(&expected)),
            "{err:?}"
        );
    }
}

#[test]
fn draw_functions_fail_after_draw_returns() {
    let mut runtime = started(
        r#"
        local later
        function draw() later = function() cls(1) end end
        function tick() if later then later() end end
        "#,
    );
    runtime.tick(NO_INPUT).unwrap();
    assert_eq!(runtime.tick(NO_INPUT).unwrap_err().phase, Phase::Tick);
}

#[test]
fn colors_must_be_palette_indices() {
    assert_drew(
        "cls(0) cls(255)",
        &[DrawCommand::Clear(0), DrawCommand::Clear(255)],
    );
    for (call, detail) in [
        (
            "cls(256)",
            "#1 to 'cls' (palette index 0-255 expected, got 256)",
        ),
        (
            "cls(-1)",
            "#1 to 'cls' (palette index 0-255 expected, got -1)",
        ),
        (
            "cls(1.5)",
            "#1 to 'cls' (palette index 0-255 expected, got 1.5)",
        ),
        (
            "cls(0/0)",
            "#1 to 'cls' (palette index 0-255 expected, got NaN)",
        ),
        (
            "cls(math.huge)",
            "#1 to 'cls' (palette index 0-255 expected, got inf)",
        ),
        (r#"cls("1")"#, "#1 to 'cls' (number expected, got string)"),
        ("cls()", "#1 to 'cls' (number expected, got nil)"),
        (
            "pset(0, 0, 300)",
            "#3 to 'pset' (palette index 0-255 expected, got 300)",
        ),
        (
            r#"print("x", 0, 0, true)"#,
            "#4 to 'print' (number expected, got boolean)",
        ),
    ] {
        assert_eq!(
            draw_error(call),
            format!("main.luau:1: bad argument {detail}"),
            "{call}"
        );
    }
}

#[test]
fn coordinates_must_be_numbers() {
    for (call, detail) in [
        ("pset(0/0, 0, 1)", "#1 to 'pset' (number is NaN)"),
        (
            r#"pset("1", 0, 1)"#,
            "#1 to 'pset' (number expected, got string)",
        ),
        ("pset(0, nil, 1)", "#2 to 'pset' (number expected, got nil)"),
        (
            r#"print("x", 0, {}, 1)"#,
            "#3 to 'print' (number expected, got table)",
        ),
        (
            r#"spr(0, "1", 0)"#,
            "#2 to 'spr' (number expected, got string)",
        ),
    ] {
        assert_eq!(
            draw_error(call),
            format!("main.luau:1: bad argument {detail}"),
            "{call}"
        );
    }
}

#[test]
fn fractional_coordinates_are_floored() {
    assert_drew(
        "pset(1.9, 2.5, 1) pset(10.5, 20.9, 2) spr(2, 3.9, 4.1) spr(2, -0.5, 1.5)",
        &[
            pixel(1, 2, 1),
            pixel(10, 20, 2),
            sprite(2, 3, 4),
            sprite(2, -1, 1),
        ],
    );
}

#[test]
fn sprite_tile_indices_must_be_integers_in_range() {
    assert_drew(
        "spr(0, 1, 2) spr(1023, 3, 4)",
        &[sprite(0, 1, 2), sprite(1023, 3, 4)],
    );
    for (call, detail) in [
        (
            "spr(-1, 0, 0)",
            "#1 to 'spr' (object tile index 0-1023 expected, got -1)",
        ),
        (
            "spr(1024, 0, 0)",
            "#1 to 'spr' (object tile index 0-1023 expected, got 1024)",
        ),
        (
            "spr(1.5, 0, 0)",
            "#1 to 'spr' (object tile index 0-1023 expected, got 1.5)",
        ),
        (
            r#"spr("1", 0, 0)"#,
            "#1 to 'spr' (number expected, got string)",
        ),
        ("spr()", "#1 to 'spr' (number expected, got nil)"),
    ] {
        assert_eq!(
            draw_error(call),
            format!("main.luau:1: bad argument {detail}"),
            "{call}"
        );
    }
}

#[test]
fn sprite_blocks_draw_one_block_per_call() {
    assert_drew(
        "cls(1) spr(24, 7, 8, 2, 2) pset(0, 0, 2) spr(5, 0.5, -1.5, 3, 2) spr(6, 0, 0)",
        &[
            DrawCommand::Clear(1),
            sprite_block(24, 7, 8, 2, 2),
            pixel(0, 0, 2),
            sprite_block(5, 0, -2, 3, 2),
            sprite(6, 0, 0),
        ],
    );
}

#[test]
fn a_missing_sprite_dimension_defaults_to_one_tile() {
    assert_drew(
        "spr(0, 0, 0, nil, nil) spr(0, 0, 0, 3) spr(0, 0, 0, nil, 4)",
        &[
            sprite(0, 0, 0),
            sprite_block(0, 0, 0, 3, 1),
            sprite_block(0, 0, 0, 1, 4),
        ],
    );
}

#[test]
fn sprite_dimensions_must_be_positive_integers() {
    for (call, detail) in [
        (
            "spr(0, 0, 0, 0, 1)",
            "#4 to 'spr' (positive tile count expected, got 0)",
        ),
        (
            "spr(0, 0, 0, 1, -1)",
            "#5 to 'spr' (positive tile count expected, got -1)",
        ),
        (
            "spr(0, 0, 0, 1.5, 1)",
            "#4 to 'spr' (positive tile count expected, got 1.5)",
        ),
        (
            "spr(0, 0, 0, 1, 0/0)",
            "#5 to 'spr' (positive tile count expected, got NaN)",
        ),
        (
            r#"spr(0, 0, 0, "2", 1)"#,
            "#4 to 'spr' (number expected, got string)",
        ),
    ] {
        assert_eq!(
            draw_error(call),
            format!("main.luau:1: bad argument {detail}"),
            "{call}"
        );
    }
}

#[test]
fn sprite_blocks_must_fit_the_object_tile_grid() {
    assert_drew(
        "spr(14, 0, 0, 2, 1) spr(1008, 0, 0, 16, 1)",
        &[
            sprite_block(14, 0, 0, 2, 1),
            sprite_block(1008, 0, 0, 16, 1),
        ],
    );
    assert_drew("spr(0, 0, 0, 16, 64)", &[sprite_block(0, 0, 0, 16, 64)]);
    for (call, detail) in [
        (
            "spr(15, 0, 0, 2, 1)",
            "#4 to 'spr' (width 2 from tile column 15 exceeds the 16-column object-tile grid)",
        ),
        (
            "spr(1023, 0, 0, 2, 1)",
            "#4 to 'spr' (width 2 from tile column 15 exceeds the 16-column object-tile grid)",
        ),
        (
            "spr(1023, 0, 0, 1, 2)",
            "#5 to 'spr' (height 2 from tile row 63 exceeds the 64-row object-tile grid)",
        ),
        (
            "spr(0, 0, 0, 17, 1)",
            "#4 to 'spr' (width 17 from tile column 0 exceeds the 16-column object-tile grid)",
        ),
    ] {
        assert_eq!(
            draw_error(call),
            format!("main.luau:1: bad argument {detail}"),
            "{call}"
        );
    }
}

#[test]
fn omitted_or_nil_sprite_options_use_their_defaults() {
    assert_drew(
        "spr(0, 0, 0) spr(0, 0, 0, nil, nil, nil, nil, nil) \
             spr(0, 0, 0, 2, 2, false) spr(3, 0, 0, 1, 1, false, false, nil)",
        &[
            sprite(0, 0, 0),
            sprite(0, 0, 0),
            sprite_block(0, 0, 0, 2, 2),
            sprite(3, 0, 0),
        ],
    );
}

#[test]
fn sprite_flips_apply_to_single_tiles_and_blocks() {
    assert_drew(
        "spr(3, 1, 2, nil, nil, true) spr(3, 1, 2, nil, nil, false, true) \
             spr(3, 1, 2, 1, 1, true, true) spr(24, 7, 8, 2, 3, true, false) \
             spr(24, 7, 8, 2, 3, false, true) spr(24, 7, 8, 2, 3, true, true)",
        &[
            sprite_flipped(3, 1, 2, 1, 1, true, false),
            sprite_flipped(3, 1, 2, 1, 1, false, true),
            sprite_flipped(3, 1, 2, 1, 1, true, true),
            sprite_flipped(24, 7, 8, 2, 3, true, false),
            sprite_flipped(24, 7, 8, 2, 3, false, true),
            sprite_flipped(24, 7, 8, 2, 3, true, true),
        ],
    );
}

#[test]
fn sprite_flips_must_be_booleans() {
    for (call, detail) in [
        (
            "spr(0, 0, 0, 1, 1, 1)",
            "#6 to 'spr' (boolean expected, got number)",
        ),
        (
            r#"spr(0, 0, 0, 1, 1, "true")"#,
            "#6 to 'spr' (boolean expected, got string)",
        ),
        (
            "spr(0, 0, 0, 1, 1, {})",
            "#6 to 'spr' (boolean expected, got table)",
        ),
        (
            "spr(0, 0, 0, 1, 1, false, 0)",
            "#7 to 'spr' (boolean expected, got number)",
        ),
        (
            "spr(0, 0, 0, nil, nil, true, Button.A)",
            "#7 to 'spr' (boolean expected, got Button)",
        ),
    ] {
        assert_eq!(
            draw_error(call),
            format!("main.luau:1: bad argument {detail}"),
            "{call}"
        );
    }
}

#[test]
fn sprite_palettes_apply_for_every_bank() {
    assert_drew(
        "spr(3, 0, 0, 1, 1, false, false, 0) spr(4, 0, 0, 2, 1, nil, nil, 7)",
        &[sprite_with_palette(3, 1, 0), sprite_with_palette(4, 2, 7)],
    );
}

#[test]
fn sprite_palettes_must_be_banks() {
    for (call, detail) in [
        (
            "spr(0, 0, 0, 1, 1, false, false, 8)",
            "#8 to 'spr' (object palette bank 0-7 expected, got 8)",
        ),
        (
            "spr(0, 0, 0, 1, 1, false, false, -1)",
            "#8 to 'spr' (object palette bank 0-7 expected, got -1)",
        ),
        (
            "spr(0, 0, 0, 1, 1, false, false, 1.5)",
            "#8 to 'spr' (object palette bank 0-7 expected, got 1.5)",
        ),
        (
            r#"spr(0, 0, 0, 1, 1, false, false, "2")"#,
            "#8 to 'spr' (number expected, got string)",
        ),
    ] {
        assert_eq!(
            draw_error(call),
            format!("main.luau:1: bad argument {detail}"),
            "{call}"
        );
    }
}

#[test]
fn flipped_sprites_cost_the_same_as_unflipped_ones() {
    const BLOCKS: usize = MAX_DRAW_COMMANDS / (4 * SPRITE_TILE_COST);
    for flips in ["true, false", "false, true", "true, true"] {
        let fits = format!("for i = 1, {BLOCKS} do spr(0, 0, 0, 2, 2, {flips}) end");
        assert!(drawn(&fits) != Framebuffer::new(), "{flips}");
        let over =
            format!("for i = 1, {BLOCKS} do spr(0, 0, 0, 2, 2, {flips}) end pset(300, 200, 1)");
        let (message, framebuffer) = draw_failure(&over);
        assert!(message.contains("too many draw commands"), "{flips}");
        assert_eq!(framebuffer.pixel(300, 200), Some(0), "{flips}: not drawn");
    }
    assert!(
        draw_error("spr(0, 0, 0, 16, 64, true, true) cls(0)").contains("too many draw commands")
    );
}

#[test]
fn rectfill_draws_one_rectangle() {
    assert_drew("rectfill(10, 20, 30, 40, 5)", &[rect(10, 20, 30, 40, 5)]);
    // Off-screen rectangles draw nothing, and one that is partly off-screen
    // is clipped to the screen.
    assert_drew(
        "rectfill(-500, 1000, 1, 1, 7) rectfill(1.9, -0.5, 2, 3, 255)",
        &[rect(1, -1, 2, 3, 255)],
    );
    assert_drew("rectfill(0, 0, 1e300, 4, 1)", &[rect(0, 0, i32::MAX, 4, 1)]);
}

#[test]
fn rectfill_sizes_must_be_positive_integers() {
    for (call, detail) in [
        (
            "rectfill(0, 0, 0, 4, 1)",
            "#3 to 'rectfill' (positive width expected, got 0)",
        ),
        (
            "rectfill(0, 0, 4, 0, 1)",
            "#4 to 'rectfill' (positive height expected, got 0)",
        ),
        (
            "rectfill(0, 0, -3, 4, 1)",
            "#3 to 'rectfill' (positive width expected, got -3)",
        ),
        (
            "rectfill(0, 0, 4, -1, 1)",
            "#4 to 'rectfill' (positive height expected, got -1)",
        ),
        (
            "rectfill(0, 0, 1.5, 4, 1)",
            "#3 to 'rectfill' (positive width expected, got 1.5)",
        ),
        (
            "rectfill(0, 0, 0/0, 4, 1)",
            "#3 to 'rectfill' (positive width expected, got NaN)",
        ),
        (
            "rectfill(0, 0, math.huge, 4, 1)",
            "#3 to 'rectfill' (positive width expected, got inf)",
        ),
        (
            r#"rectfill(0, 0, "4", 4, 1)"#,
            "#3 to 'rectfill' (number expected, got string)",
        ),
        (
            "rectfill(0, 0, 4, nil, 1)",
            "#4 to 'rectfill' (number expected, got nil)",
        ),
    ] {
        assert_eq!(
            draw_error(call),
            format!("main.luau:1: bad argument {detail}"),
            "{call}"
        );
    }
}

#[test]
fn rectfill_validates_coordinates_and_color() {
    for (call, detail) in [
        (
            r#"rectfill("1", 0, 4, 4, 1)"#,
            "#1 to 'rectfill' (number expected, got string)",
        ),
        (
            "rectfill(0, {}, 4, 4, 1)",
            "#2 to 'rectfill' (number expected, got table)",
        ),
        (
            "rectfill(0/0, 0, 4, 4, 1)",
            "#1 to 'rectfill' (number is NaN)",
        ),
        (
            "rectfill(0, 0, 4, 4, 300)",
            "#5 to 'rectfill' (palette index 0-255 expected, got 300)",
        ),
        (
            "rectfill(0, 0, 4, 4, true)",
            "#5 to 'rectfill' (number expected, got boolean)",
        ),
        (
            "rectfill(0, 0, 4, 4)",
            "#5 to 'rectfill' (number expected, got nil)",
        ),
    ] {
        assert_eq!(
            draw_error(call),
            format!("main.luau:1: bad argument {detail}"),
            "{call}"
        );
    }
}

#[test]
fn a_rectfill_costs_one_draw_unit_whatever_its_area() {
    let budget = Budget {
        interrupts: 4 * MAX_DRAW_COMMANDS as u64,
    };
    let run = |extra: &str| {
        let source = format!(
            "function draw() for i = 1, {MAX_DRAW_COMMANDS} - 1 do pset(0, 0, 1) end \
             rectfill(-1000, -1000, 100000, 100000, 2) {extra} end"
        );
        let mut runtime = Runtime::start(
            "main.luau",
            source.as_bytes(),
            Files::default(),
            budget,
            font(),
        )
        .unwrap();
        runtime
            .tick(NO_INPUT)
            .map(|()| runtime.scratch().framebuffer.clone())
    };
    let framebuffer = run("").unwrap();
    assert_eq!(framebuffer.pixel(319, 239), Some(2), "the rectangle drew");

    let err = run("pset(0, 0, 1)").unwrap_err();
    assert_eq!(err.phase, Phase::Draw);
    assert!(
        matches!(&err.error, ScriptError::Runtime(m) if m.contains("too many draw commands")),
        "{err:?}"
    );
}

#[test]
fn huge_coordinates_saturate_off_screen() {
    // A coordinate that wrapped instead of saturating could land on screen.
    assert!(
        drawn("pset(1e300, -1e300, 1) pset(math.huge, -math.huge, 2) pset(400, -3, 3)")
            == Framebuffer::new()
    );
    assert_drew(
        "pset(1e300, 5, 1) pset(-math.huge, 6, 2) pset(7, 8, 3)",
        &[pixel(7, 8, 3)],
    );
}

#[test]
fn draw_commands_are_capped_per_frame() {
    let budget = Budget {
        interrupts: 4 * MAX_DRAW_COMMANDS as u64,
    };
    // Each command writes the same pixel with its own number, so the pixel
    // holds the color of the last command that was drawn.
    let source = format!(
        r#"
        local extra = 0
        function draw()
            for i = 1, {MAX_DRAW_COMMANDS} + extra do pset(0, 0, i % 255 + 1) end
            extra = 1
        end
        "#
    );
    let mut runtime = Runtime::start(
        "main.luau",
        source.as_bytes(),
        Files::default(),
        budget,
        font(),
    )
    .unwrap();
    let last_accepted = (MAX_DRAW_COMMANDS % 255 + 1) as u8;
    runtime.tick(NO_INPUT).unwrap();
    assert_eq!(
        runtime.scratch().framebuffer.pixel(0, 0),
        Some(last_accepted)
    );

    let err = runtime.tick(NO_INPUT).unwrap_err();
    assert_eq!(err.phase, Phase::Draw);
    assert!(
        matches!(&err.error, ScriptError::Runtime(m) if m.contains("too many draw commands")),
        "{err:?}"
    );
    assert_eq!(
        runtime.scratch().framebuffer.pixel(0, 0),
        Some(last_accepted),
        "the rejected command was not drawn"
    );
}

#[test]
fn text_characters_count_toward_the_cap() {
    const CHARS: usize = MAX_DRAW_COMMANDS / TEXT_CHAR_COST;
    let fits = format!(
        r#"print(string.rep("x", {CHARS} - 1), 0, 0, 1) for i = 1, {TEXT_CHAR_COST} do pset(0, 0, 1) end"#
    );
    assert_eq!(drawn(&fits).pixel(0, 0), Some(1));
    for over in [
        format!(r#"print(string.rep("x", {CHARS}), 0, 0, 1) cls(0)"#),
        format!(r#"print(string.rep("x", {CHARS} + 1), 0, 0, 1)"#),
    ] {
        let message = draw_error(&over);
        assert!(message.contains("too many draw commands"), "{message}");
    }
    // Empty text still costs one character.
    let empty = format!(r#"for i = 1, {CHARS} + 1 do print("", 0, 0, 1) end"#);
    assert!(draw_error(&empty).contains("too many draw commands"));
}

#[test]
fn sprites_cost_sixty_four_draw_units() {
    const SPRITES: usize = MAX_DRAW_COMMANDS / 64;
    let budget = Budget {
        interrupts: 4 * MAX_DRAW_COMMANDS as u64,
    };
    let fits = format!("function draw() for i = 1, {SPRITES} do spr(0, 0, 0) end end");
    let mut runtime = Runtime::start(
        "main.luau",
        fits.as_bytes(),
        Files::default(),
        budget,
        font(),
    )
    .unwrap();
    runtime.tick(NO_INPUT).unwrap();
    assert!(runtime.scratch().framebuffer != Framebuffer::new());

    let over = format!(
        "function draw() for i = 1, {} do spr(0, 0, 0) end end",
        SPRITES + 1
    );
    let mut runtime = Runtime::start(
        "main.luau",
        over.as_bytes(),
        Files::default(),
        budget,
        font(),
    )
    .unwrap();
    let err = runtime.tick(NO_INPUT).unwrap_err();
    assert!(matches!(
        err.error,
        ScriptError::Runtime(message) if message.contains("too many draw commands")
    ));
}

#[test]
fn sprite_blocks_cost_sixty_four_units_per_tile() {
    const BLOCKS: usize = MAX_DRAW_COMMANDS / (4 * SPRITE_TILE_COST);
    let fits = format!("for i = 1, {BLOCKS} do spr(0, 0, 0, 2, 2) end");
    assert!(drawn(&fits) != Framebuffer::new());
    let over = format!("for i = 1, {BLOCKS} do spr(0, 0, 0, 2, 2) end pset(0, 0, 1)");
    assert!(draw_error(&over).contains("too many draw commands"));

    assert!(drawn("spr(0, 0, 0, 16, 64)") != Framebuffer::new());
    assert!(draw_error("spr(0, 0, 0, 16, 64) cls(0)").contains("too many draw commands"));
}

#[test]
fn tile_viewer_example_draws_and_navigates_its_visible_range() {
    let source = include_str!("../../../../examples/tile-viewer/code/main.luau");
    let files = Files::new(&[(
        "code/grid.luau",
        include_str!("../../../../examples/tile-viewer/code/grid.luau"),
    )]);
    let mut runtime = started_with(source, files);
    assert_eq!(texts(runtime.take_output()), ["grid module loaded"]);
    // The grid starts at (4, 34) in cells of 39 by 39 pixels, and each
    // sprite sits at (14, 2) inside its cell.
    let sprite_at = |runtime: &Runtime, offset: i32, tile: usize| {
        let hardware = runtime.scratch();
        let (x, y) = (4 + 14 + offset % 8 * 39, 34 + 2 + offset / 8 * 39);
        (0..64).all(|n| {
            let (dx, dy) = (n % 8, n / 8);
            let local = hardware
                .obj
                .tiles()
                .pixel(tile, dx as usize, dy as usize)
                .unwrap();
            let bank = hardware.obj.attributes().bank(tile);
            let expected = hardware.obj.palettes().resolve(bank, local);
            let shown = hardware.framebuffer.pixel(x + dx, y + dy);
            // The selection border is around cell 0, never inside the tile.
            shown == Some(expected.unwrap_or(0))
        })
    };
    let sprite_at_blank = |runtime: &Runtime, offset: i32| {
        let (x, y) = (4 + 14 + offset % 8 * 39, 34 + 2 + offset / 8 * 39);
        (0..64).all(|n| runtime.scratch().framebuffer.pixel(x + n % 8, y + n / 8) == Some(0))
    };

    let showing = |runtime: &Runtime, first: usize, count: i32| {
        let shown_all =
            (0..count).all(|offset| sprite_at(runtime, offset, first + offset as usize));
        let blank_after = (count..40).all(|offset| sprite_at_blank(runtime, offset));
        shown_all && blank_after
    };
    runtime.tick(NO_INPUT).unwrap();
    assert!(showing(&runtime, 0, 40));
    // The selected tile has the border drawn around it.
    assert_eq!(runtime.scratch().framebuffer.pixel(16, 36), Some(15));

    runtime.tick(buttons(&[], &[Button::Right])).unwrap();
    assert!(showing(&runtime, 1, 40));

    runtime.tick(buttons(&[], &[Button::Down])).unwrap();
    assert!(showing(&runtime, 17, 40));

    for _ in 0..130 {
        runtime.tick(buttons(&[], &[Button::Down])).unwrap();
    }
    assert!(showing(&runtime, 1023, 1));
    // The header names the selected tile, drawn with the font.
    let header = after(&[DrawCommand::Text {
        text: "SELECTED TILE 1023".into(),
        x: 6,
        y: 5,
        color: 7,
    }]);
    let framebuffer = &runtime.scratch().framebuffer;
    assert!(
        (0..320 * 240).all(|n| {
            let (x, y) = (n % 320, n / 320);
            y > 12 || header.pixel(x, y) == Some(0) || framebuffer.pixel(x, y) == header.pixel(x, y)
        }),
        "the header text"
    );
}
