//! [`DrawCommand`]s and the draw cost one `draw()` call may spend on them.

use glaux_machine::obj_palette::ObjBank;
use glaux_machine::sprite::{OBJ_TILE_HEIGHT, OBJ_TILE_WIDTH};

/// Draw cost one `draw()` call may spend: each clear, pixel or filled
/// rectangle costs one whatever area it covers, each sprite costs the 64
/// pixels per tile it may draw, and each text costs [`TEXT_CHAR_COST`] per
/// character.
///
/// The completed frame is replayed on every host presentation frame, which
/// may run faster than the 60 Hz tick, so this bounds per-frame host work as
/// well as memory. Without it, a runtime's execution budget alone would let
/// one `draw()` record millions of commands.
pub const MAX_DRAW_COMMANDS: usize = 65_536;

/// Draw cost of one object tile drawn by a sprite: the 64 pixels it may set.
pub const SPRITE_TILE_COST: usize = OBJ_TILE_WIDTH * OBJ_TILE_HEIGHT;

/// Draw cost of one character of text: about the number of pixels a system
/// font glyph sets, so that text and single pixels cost alike per pixel.
pub const TEXT_CHAR_COST: usize = 16;

/// One drawing operation recorded during `draw()`.
///
/// Colors are palette indices. Coordinates are unclipped: whoever replays
/// the commands clips them to the screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DrawCommand {
    /// Fill the whole screen.
    Clear(u8),
    /// Write one pixel.
    Pixel { x: i32, y: i32, color: u8 },
    /// Fill the `width`x`height` rectangle whose top-left corner is
    /// `(x, y)`. Both sizes are at least 1.
    Rect {
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        color: u8,
    },
    /// Draw a `width`x`height` block of object tiles from the object-tile
    /// grid, `tile` at its top-left, with `(x, y)` as its top-left corner.
    /// The block is validated to fit the grid. `flip_x` and `flip_y` mirror
    /// the whole composed block, not each tile in place. `palette` is the
    /// object palette bank for every tile of the block, or `None` to let each
    /// tile use its default bank.
    Sprite {
        tile: u16,
        x: i32,
        y: i32,
        width: u8,
        height: u8,
        flip_x: bool,
        flip_y: bool,
        palette: Option<ObjBank>,
    },
    /// Draw text with `(x, y)` as the top-left corner of its first
    /// character cell.
    Text {
        text: String,
        x: i32,
        y: i32,
        color: u8,
    },
}

impl DrawCommand {
    /// What this command charges against [`MAX_DRAW_COMMANDS`].
    pub fn cost(&self) -> usize {
        match self {
            Self::Text { text, .. } => text.chars().count().max(1).saturating_mul(TEXT_CHAR_COST),
            // Grid-sized blocks cannot overflow. If one did, its cost must
            // still exceed the limit rather than wrap below it.
            Self::Sprite { width, height, .. } => usize::from(*width)
                .checked_mul(usize::from(*height))
                .and_then(|tiles| tiles.checked_mul(SPRITE_TILE_COST))
                .unwrap_or(usize::MAX),
            Self::Clear(_) | Self::Pixel { .. } | Self::Rect { .. } => 1,
        }
    }
}

/// A command that would have taken a frame past [`MAX_DRAW_COMMANDS`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("too many draw commands in one frame (limit {MAX_DRAW_COMMANDS})")]
pub struct DrawBudgetExceeded;

/// Commands recorded by one `draw()` call, in call order.
#[derive(Debug, Default)]
pub struct Recording {
    commands: Vec<DrawCommand>,
    /// Draw cost spent so far, charged against [`MAX_DRAW_COMMANDS`].
    cost: usize,
}

impl Recording {
    /// Appends `command` unless its cost takes the frame past
    /// [`MAX_DRAW_COMMANDS`]. A rejected command is not recorded, but its
    /// cost stays spent, so every later command is rejected too.
    pub fn push(&mut self, command: DrawCommand) -> Result<(), DrawBudgetExceeded> {
        self.cost = self.cost.saturating_add(command.cost());
        if self.cost > MAX_DRAW_COMMANDS {
            return Err(DrawBudgetExceeded);
        }
        self.commands.push(command);
        Ok(())
    }

    pub fn into_commands(self) -> Vec<DrawCommand> {
        self.commands
    }
}
