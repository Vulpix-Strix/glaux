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
    let mut recording = Recording::default();
    for _ in 0..MAX_DRAW_COMMANDS - 1 {
        recording.push(DrawCommand::Clear(0)).unwrap();
    }
    recording.push(DrawCommand::Clear(1)).unwrap();
    assert_eq!(
        recording.push(DrawCommand::Clear(2)),
        Err(DrawBudgetExceeded)
    );
    let commands = recording.into_commands();
    assert_eq!(commands.len(), MAX_DRAW_COMMANDS);
    assert_eq!(commands.last(), Some(&DrawCommand::Clear(1)));
}

#[test]
fn a_rejected_command_keeps_its_cost_spent() {
    let mut recording = Recording::default();
    // One text over the whole budget, then a single unit that alone would fit.
    assert_eq!(
        recording.push(text(MAX_DRAW_COMMANDS / TEXT_CHAR_COST + 1)),
        Err(DrawBudgetExceeded)
    );
    assert_eq!(
        recording.push(DrawCommand::Clear(0)),
        Err(DrawBudgetExceeded)
    );
    assert!(recording.into_commands().is_empty());
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
