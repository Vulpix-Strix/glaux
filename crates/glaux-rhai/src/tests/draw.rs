//! `cls`, `pset`, `rectfill`, `spr` and `text`, and the draw budget.

use super::*;
use glaux_machine::framebuffer::Framebuffer;
use glaux_machine::obj_palette::ObjBank;
use glaux_script::{DrawCommand, MAX_DRAW_COMMANDS, SPRITE_TILE_COST, TEXT_CHAR_COST};
use pretty_assertions::assert_eq;

/// The framebuffer of the test hardware after one tick of a script whose
/// `draw()` body is `body`.
fn drawn(body: &str) -> Framebuffer {
    let mut runtime = started(&format!("fn draw() {{\n{body}\n}}"));
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

/// The error message of a `draw()` whose body is `body`, raised on line 2.
fn draw_error(body: &str) -> String {
    tick_message(&format!("fn draw() {{\n{body}\n}}"))
}

fn sprite(tile: u16, x: i32, y: i32, width: u8, height: u8) -> DrawCommand {
    DrawCommand::Sprite {
        tile,
        x,
        y,
        width,
        height,
        flip_x: false,
        flip_y: false,
        palette: None,
    }
}

#[test]
fn each_function_draws_in_call_order() {
    assert_drew(
        r#"cls(1); pset(2, 3, 4); rectfill(5, 6, 7, 8, 9); spr(10, 11, 12); text("hi", 13, 14, 15);"#,
        &[
            DrawCommand::Clear(1),
            DrawCommand::Pixel {
                x: 2,
                y: 3,
                color: 4,
            },
            DrawCommand::Rect {
                x: 5,
                y: 6,
                width: 7,
                height: 8,
                color: 9,
            },
            sprite(10, 11, 12, 1, 1),
            DrawCommand::Text {
                text: "hi".into(),
                x: 13,
                y: 14,
                color: 15,
            },
        ],
    );
    // The last writer of a pixel wins, and a later `cls` erases everything.
    assert_eq!(drawn("pset(1, 1, 2); pset(1, 1, 3);").pixel(1, 1), Some(3));
    assert_eq!(drawn("pset(1, 1, 3); pset(1, 1, 2);").pixel(1, 1), Some(2));
    assert!(drawn("rectfill(0, 0, 9, 9, 5); cls(0);") == Framebuffer::new());
}

#[test]
fn float_coordinates_are_floored_and_integral_floats_are_accepted() {
    assert_drew(
        "pset(1.9, 2.5, 7.0); pset(5.0, 6.0, 8); spr(2, 3.9, 4.1); spr(2, -0.5, 1.5);",
        &[
            DrawCommand::Pixel {
                x: 1,
                y: 2,
                color: 7,
            },
            DrawCommand::Pixel {
                x: 5,
                y: 6,
                color: 8,
            },
            sprite(2, 3, 4, 1, 1),
            sprite(2, -1, 1, 1, 1),
        ],
    );
}

#[test]
fn spr_takes_optional_arguments_left_out_or_given_as_unit() {
    let flipped = |palette| DrawCommand::Sprite {
        tile: 0,
        x: 0,
        y: 0,
        width: 2,
        height: 3,
        flip_x: true,
        flip_y: false,
        palette,
    };
    assert_drew(
        "spr(0, 0, 0, 2, 3, true); spr(0, 0, 0, 2, 3, true, (), 5); spr(0, 0, 0, (), ());",
        &[
            flipped(None),
            flipped(ObjBank::new(5)),
            sprite(0, 0, 0, 1, 1),
        ],
    );
}

#[test]
fn text_converts_any_value() {
    let text = |text: &str, x: i32| DrawCommand::Text {
        text: text.into(),
        x,
        y: 0,
        color: 1,
    };
    assert_drew(
        "text(42, 0, 0, 1); text(1.5, 20, 0, 1); text((), 40, 0, 1); text(Button::B, 60, 0, 1);",
        &[
            text("42", 0),
            text("1.5", 20),
            text("()", 40),
            text("Button::B", 60),
        ],
    );
}

#[test]
fn invalid_arguments_name_the_function_and_position() {
    let cases = [
        (
            "cls(256);",
            "bad argument #1 to 'cls' (palette index 0-255 expected, got 256)",
        ),
        (
            "cls(1.5);",
            "bad argument #1 to 'cls' (palette index 0-255 expected, got 1.5)",
        ),
        (
            "pset(\"1\", 0, 0);",
            "bad argument #1 to 'pset' (number expected, got string)",
        ),
        (
            "rectfill(0, 0, 0, 1, 1);",
            "bad argument #3 to 'rectfill' (positive width expected, got 0)",
        ),
        (
            "spr(1024, 0, 0);",
            "bad argument #1 to 'spr' (object tile index 0-1023 expected, got 1024)",
        ),
        (
            "spr(15, 0, 0, 2);",
            "bad argument #4 to 'spr' (width 2 from tile column 15 exceeds the 16-column object-tile grid)",
        ),
        (
            "spr(0, 0, 0, 1, 1, 1);",
            "bad argument #6 to 'spr' (bool expected, got i64)",
        ),
        (
            "spr(0, 0, 0, 1, 1, false, false, 8);",
            "bad argument #8 to 'spr' (object palette bank 0-7 expected, got 8)",
        ),
        (
            "text(\"x\", 0, 0, -1);",
            "bad argument #4 to 'text' (palette index 0-255 expected, got -1)",
        ),
    ];
    for (body, message) in cases {
        assert_eq!(
            draw_error(body),
            format!("main.rhai:2: {message}"),
            "{body}"
        );
    }
}

#[test]
fn drawing_outside_draw_fails_before_checking_arguments() {
    for (call, function) in [
        ("cls(999)", "cls"),
        ("pset()", "pset"),
        ("text(1, 2, 3, 4)", "text"),
        ("spr(0, 0, 0)", "spr"),
        ("rectfill(0, 0, 1, 1, 1)", "rectfill"),
    ] {
        let source = format!("fn tick() {{ {call}; }}");
        // `pset()` has no overload for no arguments: Rhai reports that
        // instead, which is still an error raised in tick.
        if call == "pset()" {
            assert!(started(&source).tick(NO_INPUT).is_err());
            continue;
        }
        assert_eq!(
            tick_message(&source),
            format!("main.rhai:1: '{function}' can only be called during draw()"),
        );
    }
    assert_eq!(
        start_message("cls(0);"),
        "main.rhai:1: 'cls' can only be called during draw()"
    );
}

#[test]
fn a_frame_may_spend_exactly_the_draw_budget() {
    let budget = Budget {
        operations: 10_000_000,
    };
    let source = format!(
        "fn boot() {{ this.extra = 0; }}\nfn draw() {{ for i in 0..{} {{ pset(0, 0, 1); }} for i in 0..this.extra {{ pset(0, 0, 2); }} }}",
        MAX_DRAW_COMMANDS
    );
    let mut runtime = Runtime::start(
        "main.rhai",
        source.as_bytes(),
        Files::default(),
        budget,
        font(),
    )
    .unwrap();
    runtime.tick(NO_INPUT).unwrap();
    assert_eq!(runtime.scratch().framebuffer.pixel(0, 0), Some(1));

    let over = source.replace("this.extra = 0", "this.extra = 1");
    let mut runtime = Runtime::start(
        "main.rhai",
        over.as_bytes(),
        Files::default(),
        budget,
        font(),
    )
    .unwrap();
    assert_eq!(
        runtime.tick(NO_INPUT),
        Err(runtime_error(
            Phase::Draw,
            &format!(
                "main.rhai:2: too many draw commands in one frame (limit {MAX_DRAW_COMMANDS})"
            )
        ))
    );
    assert_eq!(
        runtime.scratch().framebuffer.pixel(0, 0),
        Some(1),
        "the rejected command was not drawn"
    );
}

#[test]
fn sprites_and_text_cost_what_they_may_draw() {
    // One 4x4 block costs as much as 16 tiles; text costs per character.
    // One block fewer than would fill the budget leaves room for the text.
    let tiles = MAX_DRAW_COMMANDS / SPRITE_TILE_COST / 16 - 1;
    let chars = (MAX_DRAW_COMMANDS - tiles * 16 * SPRITE_TILE_COST) / TEXT_CHAR_COST;
    let body = format!(
        "for i in 0..{tiles} {{ spr(0, 0, 0, 4, 4); }} text(\"{}\", 0, 0, 1);",
        "x".repeat(chars)
    );
    assert!(drawn(&body) != Framebuffer::new());
    assert_eq!(
        draw_error(&format!("{body} pset(0, 0, 1);")),
        format!("main.rhai:2: too many draw commands in one frame (limit {MAX_DRAW_COMMANDS})")
    );
}
