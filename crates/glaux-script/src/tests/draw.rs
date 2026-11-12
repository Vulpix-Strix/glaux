use pretty_assertions::assert_eq;

use crate::*;

fn text(len: usize) -> DrawCommand {
    DrawCommand::Text {
        text: "x".repeat(len),
        x: 0,
        y: 0,
        color: 1,
    }
}

#[test]
fn a_frame_may_spend_exactly_the_budget() {
    let mut budget = DrawBudget::default();
    for _ in 0..MAX_DRAW_COMMANDS {
        budget.charge(&DrawCommand::Clear(0)).unwrap();
    }
    assert_eq!(
        budget.charge(&DrawCommand::Clear(2)),
        Err(DrawBudgetExceeded)
    );
}

#[test]
fn a_rejected_command_keeps_its_cost_spent() {
    let mut budget = DrawBudget::default();
    // One text over the whole budget, then a single unit that alone would fit.
    assert_eq!(
        budget.charge(&text(MAX_DRAW_COMMANDS / TEXT_CHAR_COST + 1)),
        Err(DrawBudgetExceeded)
    );
    assert_eq!(
        budget.charge(&DrawCommand::Clear(0)),
        Err(DrawBudgetExceeded)
    );
}

#[test]
fn costs_follow_what_each_command_may_draw() {
    assert_eq!(text(0).cost(), TEXT_CHAR_COST);
    assert_eq!(text(3).cost(), 3 * TEXT_CHAR_COST);
    let sprite = DrawCommand::Sprite {
        tile: 0,
        x: 0,
        y: 0,
        width: 2,
        height: 3,
        flip_x: false,
        flip_y: false,
        palette: None,
    };
    assert_eq!(sprite.cost(), 6 * SPRITE_TILE_COST);
    let rect = DrawCommand::Rect {
        x: 0,
        y: 0,
        width: 320,
        height: 240,
        color: 1,
    };
    assert_eq!(rect.cost(), 1);
}

fn hardware() -> glaux_machine::hardware::CartridgeHardware {
    use glaux_machine::bg::BgVram;
    use glaux_machine::obj_palette::ObjPalettes;
    use glaux_machine::palette::Palette;
    use glaux_machine::sprite::{ObjTileAttributes, ObjTileBank, ObjVram};

    let mut obj = ObjVram::new(
        ObjTileBank::new(),
        ObjTileAttributes::new(),
        ObjPalettes::linear(),
    );
    // Tile 2: local index 3 at (1, 2) and local index 5 at (6, 7).
    obj.tiles_mut().set_pixel(2, 1, 2, 3);
    obj.tiles_mut().set_pixel(2, 6, 7, 5);
    glaux_machine::hardware::CartridgeHardware::new(Palette::default(), obj, BgVram::default())
}

fn font() -> glaux_machine::font::BitmapFont {
    let mut font =
        glaux_machine::font::BitmapFont::new(glaux_machine::font::FontMetrics::new(4, 5).unwrap());
    // A glyph whose only set pixel is its top-left corner.
    font.insert('x', 1, &[0b1000, 0, 0, 0, 0]).unwrap();
    font
}

#[test]
fn commands_draw_on_the_work_framebuffer_of_the_hardware() {
    let mut hardware = hardware();
    let font = font();
    DrawCommand::Clear(4).execute(&mut hardware, &font);
    DrawCommand::Pixel {
        x: 3,
        y: 4,
        color: 9,
    }
    .execute(&mut hardware, &font);
    DrawCommand::Rect {
        x: 10,
        y: 10,
        width: 2,
        height: 2,
        color: 6,
    }
    .execute(&mut hardware, &font);
    // Positions off the screen are clipped.
    DrawCommand::Pixel {
        x: -1,
        y: 0,
        color: 9,
    }
    .execute(&mut hardware, &font);

    let frame = &hardware.framebuffer;
    assert_eq!(frame.pixel(0, 0), Some(4));
    assert_eq!(frame.pixel(3, 4), Some(9));
    assert_eq!(frame.pixel(11, 11), Some(6));
    assert_eq!(frame.pixel(12, 11), Some(4));
}

#[test]
fn sprites_read_the_tiles_of_the_same_hardware_and_text_uses_the_font() {
    let mut hardware = hardware();
    let font = font();
    DrawCommand::Sprite {
        tile: 2,
        x: 20,
        y: 30,
        width: 1,
        height: 1,
        flip_x: false,
        flip_y: false,
        palette: None,
    }
    .execute(&mut hardware, &font);
    DrawCommand::Text {
        text: "x".into(),
        x: 100,
        y: 50,
        color: 7,
    }
    .execute(&mut hardware, &font);

    let frame = &hardware.framebuffer;
    // Bank 0 is the identity, so a local index is its own master index.
    assert_eq!(frame.pixel(21, 32), Some(3));
    assert_eq!(frame.pixel(26, 37), Some(5));
    assert_eq!(frame.pixel(20, 30), Some(0), "local index 0 is transparent");
    assert_eq!(frame.pixel(100, 50), Some(7));
    assert_eq!(frame.pixel(101, 50), Some(0));
}
