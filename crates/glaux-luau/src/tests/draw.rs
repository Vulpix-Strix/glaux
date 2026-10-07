//! Drawing functions and frame publication.

use super::*;
use pretty_assertions::assert_eq;

fn pixel(x: i32, y: i32, color: u8) -> DrawCommand {
    DrawCommand::Pixel { x, y, color }
}

fn sprite(tile: u16, x: i32, y: i32) -> DrawCommand {
    sprite_block(tile, x, y, 1, 1)
}

fn sprite_block(tile: u16, x: i32, y: i32, width: u8, height: u8) -> DrawCommand {
    DrawCommand::Sprite {
        tile,
        x,
        y,
        width,
        height,
    }
}

/// The frame published by one tick of a script whose `draw()` body is
/// `body`.
fn drawn(body: &str) -> Vec<DrawCommand> {
    let mut runtime = started(&format!("function draw() {body} end"));
    runtime.tick(NO_INPUT).unwrap();
    runtime.frame().to_vec()
}

/// The runtime error raised by the first tick of `draw()` `body`.
fn draw_error(body: &str) -> String {
    let mut runtime = started(&format!("function draw() {body} end"));
    match runtime.tick(NO_INPUT) {
        Err(RuntimeError {
            phase: Phase::Draw,
            error: ScriptError::Runtime(message),
        }) => message,
        other => panic!("expected a runtime error in draw, got {other:?}"),
    }
}

#[test]
fn the_frame_starts_empty() {
    assert_eq!(started("").frame(), []);
}

#[test]
fn draw_functions_record_commands_in_call_order() {
    assert_eq!(drawn("cls(3)"), [DrawCommand::Clear(3)]);
    assert_eq!(
        drawn(r#"cls(1) spr(3, 7, 8) pset(5, 5, 2) print("HI", 10, 20, 16) cls(0)"#),
        [
            DrawCommand::Clear(1),
            sprite(3, 7, 8),
            pixel(5, 5, 2),
            DrawCommand::Text {
                text: "HI".into(),
                x: 10,
                y: 20,
                color: 16,
            },
            DrawCommand::Clear(0),
        ]
    );
}

#[test]
fn print_converts_text_with_the_original_tostring() {
    assert_eq!(
        drawn(r#"tostring = function() return "x" end print(42, 0, 0, 1)"#),
        [DrawCommand::Text {
            text: "42".into(),
            x: 0,
            y: 0,
            color: 1,
        }]
    );
}

#[test]
fn each_successful_draw_replaces_the_frame() {
    let mut runtime = started(
        r#"
        local n = 0
        function draw() n += 1 for i = 1, n do pset(i, 0, n) end end
        "#,
    );
    runtime.tick(NO_INPUT).unwrap();
    assert_eq!(runtime.frame(), [pixel(1, 0, 1)]);
    runtime.tick(NO_INPUT).unwrap();
    assert_eq!(runtime.frame(), [pixel(1, 0, 2), pixel(2, 0, 2)]);
}

#[test]
fn a_draw_that_records_nothing_publishes_an_empty_frame() {
    let mut runtime =
        started("local first = true function draw() if first then cls(1) end first = false end");
    runtime.tick(NO_INPUT).unwrap();
    assert_eq!(runtime.frame(), [DrawCommand::Clear(1)]);
    runtime.tick(NO_INPUT).unwrap();
    assert_eq!(runtime.frame(), []);
}

#[test]
fn a_failing_draw_keeps_the_previous_frame() {
    for failure in [r#"error("boom")"#, "while true do end"] {
        let mut runtime = started(&format!(
            r#"
            local n = 0
            function draw()
                n += 1
                cls(n)
                if n == 2 then pset(1, 1, 1) {failure} end
            end
            "#
        ));
        runtime.tick(NO_INPUT).unwrap();
        assert!(runtime.tick(NO_INPUT).is_err(), "{failure}");
        assert_eq!(runtime.frame(), [DrawCommand::Clear(1)], "{failure}");
    }
}

#[test]
fn a_failing_draw_does_not_publish_sprite_commands() {
    let mut runtime = started(
        r#"
        local n = 0
        function draw()
            n += 1
            cls(n)
            if n == 2 then spr(7, 10, 20) error("boom") end
        end
        "#,
    );
    runtime.tick(NO_INPUT).unwrap();
    assert!(runtime.tick(NO_INPUT).is_err());
    assert_eq!(runtime.frame(), [DrawCommand::Clear(1)]);
}

#[test]
fn a_failing_tick_keeps_the_previous_frame() {
    let mut runtime = started(
        r#"
        local n = 0
        function tick() n += 1 if n == 2 then error("x") end end
        function draw() cls(n) end
        "#,
    );
    runtime.tick(NO_INPUT).unwrap();
    assert!(runtime.tick(NO_INPUT).is_err());
    assert_eq!(runtime.frame(), [DrawCommand::Clear(1)]);
}

#[test]
fn removing_draw_keeps_the_last_frame() {
    let mut runtime = started("function draw() cls(4) draw = nil end");
    runtime.tick(NO_INPUT).unwrap();
    runtime.tick(NO_INPUT).unwrap();
    assert_eq!(runtime.frame(), [DrawCommand::Clear(4)]);
}

#[test]
fn draw_functions_fail_outside_draw() {
    for call in [
        "cls(1)",
        "pset(0, 0, 1)",
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
    assert_eq!(
        drawn("cls(0) cls(255)"),
        [DrawCommand::Clear(0), DrawCommand::Clear(255)]
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
    assert_eq!(
        drawn("pset(1.9, 2.5, 1) pset(-0.5, -1.5, 1) spr(2, 3.9, -0.5)"),
        [pixel(1, 2, 1), pixel(-1, -2, 1), sprite(2, 3, -1)]
    );
}

#[test]
fn sprite_tile_indices_must_be_integers_in_range() {
    assert_eq!(
        drawn("spr(0, 1, 2) spr(1023, 3, 4)"),
        [sprite(0, 1, 2), sprite(1023, 3, 4)]
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
fn sprite_blocks_record_one_command_per_call() {
    assert_eq!(
        drawn("cls(1) spr(24, 7, 8, 2, 2) pset(0, 0, 2) spr(5, 0.5, -1.5, 3, 2) spr(6, 0, 0)"),
        [
            DrawCommand::Clear(1),
            sprite_block(24, 7, 8, 2, 2),
            pixel(0, 0, 2),
            sprite_block(5, 0, -2, 3, 2),
            sprite(6, 0, 0),
        ]
    );
}

#[test]
fn a_missing_sprite_dimension_defaults_to_one_tile() {
    assert_eq!(
        drawn("spr(0, 0, 0, nil, nil) spr(0, 0, 0, 3) spr(0, 0, 0, nil, 4)"),
        [
            sprite(0, 0, 0),
            sprite_block(0, 0, 0, 3, 1),
            sprite_block(0, 0, 0, 1, 4),
        ]
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
    assert_eq!(
        drawn("spr(14, 0, 0, 2, 1) spr(1008, 0, 0, 16, 1)"),
        [
            sprite_block(14, 0, 0, 2, 1),
            sprite_block(1008, 0, 0, 16, 1),
        ]
    );
    assert_eq!(
        drawn("spr(0, 0, 0, 16, 64)"),
        [sprite_block(0, 0, 0, 16, 64)]
    );
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
fn huge_coordinates_saturate() {
    assert_eq!(
        drawn("pset(1e300, -1e300, 1) pset(math.huge, -math.huge, 2) pset(400, -3, 3)"),
        [
            pixel(i32::MAX, i32::MIN, 1),
            pixel(i32::MAX, i32::MIN, 2),
            pixel(400, -3, 3),
        ]
    );
}

#[test]
fn draw_commands_are_capped_per_frame() {
    let budget = Budget {
        interrupts: 4 * MAX_DRAW_COMMANDS as u64,
    };
    let source = format!(
        r#"
        local extra = 0
        function draw() for i = 1, {MAX_DRAW_COMMANDS} + extra do pset(0, 0, 1) end extra = 1 end
        "#
    );
    let mut runtime =
        Runtime::start("main.luau", source.as_bytes(), Files::default(), budget).unwrap();
    runtime.tick(NO_INPUT).unwrap();
    assert_eq!(runtime.frame().len(), MAX_DRAW_COMMANDS);

    let err = runtime.tick(NO_INPUT).unwrap_err();
    assert_eq!(err.phase, Phase::Draw);
    assert!(
        matches!(&err.error, ScriptError::Runtime(m) if m.contains("too many draw commands")),
        "{err:?}"
    );
    assert_eq!(runtime.frame().len(), MAX_DRAW_COMMANDS);
}

#[test]
fn text_characters_count_toward_the_cap() {
    const CHARS: usize = MAX_DRAW_COMMANDS / TEXT_CHAR_COST;
    let fits = format!(
        r#"print(string.rep("x", {CHARS} - 1), 0, 0, 1) for i = 1, {TEXT_CHAR_COST} do pset(0, 0, 1) end"#
    );
    assert_eq!(drawn(&fits).len(), 1 + TEXT_CHAR_COST);
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
    let mut runtime =
        Runtime::start("main.luau", fits.as_bytes(), Files::default(), budget).unwrap();
    runtime.tick(NO_INPUT).unwrap();
    assert_eq!(runtime.frame().len(), SPRITES);

    let over = format!(
        "function draw() for i = 1, {} do spr(0, 0, 0) end end",
        SPRITES + 1
    );
    let mut runtime =
        Runtime::start("main.luau", over.as_bytes(), Files::default(), budget).unwrap();
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
    assert_eq!(drawn(&fits).len(), BLOCKS);
    let over = format!("for i = 1, {BLOCKS} do spr(0, 0, 0, 2, 2) end pset(0, 0, 1)");
    assert!(draw_error(&over).contains("too many draw commands"));

    assert_eq!(drawn("spr(0, 0, 0, 16, 64)").len(), 1);
    assert!(draw_error("spr(0, 0, 0, 16, 64) cls(0)").contains("too many draw commands"));
}

#[test]
fn a_failing_draw_does_not_publish_sprite_blocks() {
    let mut runtime = started(
        r#"
        local n = 0
        function draw()
            n += 1
            spr(0, 0, 0, 2, 2)
            if n == 2 then spr(0, 0, 0, 16, 64) end
        end
        "#,
    );
    runtime.tick(NO_INPUT).unwrap();
    assert!(runtime.tick(NO_INPUT).is_err());
    assert_eq!(runtime.frame(), [sprite_block(0, 0, 0, 2, 2)]);
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
    let sprite_tiles = |runtime: &Runtime| {
        runtime
            .frame()
            .iter()
            .filter_map(|command| match command {
                DrawCommand::Sprite { tile, .. } => Some(*tile),
                _ => None,
            })
            .collect::<Vec<_>>()
    };

    runtime.tick(NO_INPUT).unwrap();
    assert_eq!(sprite_tiles(&runtime), (0..40).collect::<Vec<_>>());

    runtime.tick(buttons(&[], &[Button::Right])).unwrap();
    assert_eq!(sprite_tiles(&runtime), (1..41).collect::<Vec<_>>());

    runtime.tick(buttons(&[], &[Button::Down])).unwrap();
    assert_eq!(sprite_tiles(&runtime), (17..57).collect::<Vec<_>>());

    for _ in 0..130 {
        runtime.tick(buttons(&[], &[Button::Down])).unwrap();
    }
    assert_eq!(sprite_tiles(&runtime), vec![1023]);
    assert!(runtime.frame().iter().any(|command| matches!(
        command,
        DrawCommand::Text { text, .. } if text == "SELECTED TILE 1023"
    )));
    assert!(
        runtime
            .frame()
            .iter()
            .any(|command| matches!(command, DrawCommand::Pixel { .. }))
    );
}
