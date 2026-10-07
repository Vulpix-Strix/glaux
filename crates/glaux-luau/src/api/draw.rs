//! `cls`, `pset`, `rectfill`, `spr` and `print`, which record
//! [`DrawCommand`]s during `draw()`.

use std::cell::RefCell;
use std::rc::Rc;

use glaux_machine::sprite::{OBJ_GRID_COLUMNS, OBJ_GRID_ROWS, OBJ_TILE_HEIGHT, OBJ_TILE_WIDTH};
use mlua::{Function, Lua, Value};

use crate::args::{color_arg, coordinate_arg, located, size_arg, tile_arg, tile_span_arg, to_text};

/// Draw cost one `draw()` call may spend: each `cls`, `pset` or `rectfill`
/// costs one whatever area it covers, each `spr` costs the 64 pixels per tile
/// it may draw, and each `print` costs [`TEXT_CHAR_COST`] per character.
///
/// The completed frame is replayed on every host presentation frame, which
/// may run faster than the 60 Hz tick, so this bounds per-frame host work as
/// well as memory. Without it, the interrupt budget alone would let one
/// `draw()` record millions of commands.
pub const MAX_DRAW_COMMANDS: usize = 65_536;

/// Draw cost of one object tile drawn by `spr`: the 64 pixels it may set.
pub const SPRITE_TILE_COST: usize = OBJ_TILE_WIDTH * OBJ_TILE_HEIGHT;

/// Draw cost of one character of `print` text: about the number of pixels
/// a system font glyph sets, so that text and `pset` cost alike per pixel.
pub const TEXT_CHAR_COST: usize = 16;

/// One drawing operation recorded during `draw()`.
///
/// Colors are palette indices. Coordinates are unclipped: whoever replays
/// the commands clips them to the screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DrawCommand {
    /// `cls(color)`: fill the whole screen.
    Clear(u8),
    /// `pset(x, y, color)`: write one pixel.
    Pixel { x: i32, y: i32, color: u8 },
    /// `rectfill(x, y, width, height, color)`: fill the `width`x`height`
    /// rectangle whose top-left corner is `(x, y)`. Both sizes are at
    /// least 1.
    Rect {
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        color: u8,
    },
    /// `spr(tile, x, y, w, h)`: draw a `width`x`height` block of object
    /// tiles from the object-tile grid, `tile` at its top-left, with `(x, y)`
    /// as its top-left corner. The block is validated to fit the grid.
    Sprite {
        tile: u16,
        x: i32,
        y: i32,
        width: u8,
        height: u8,
    },
    /// `print(text, x, y, color)`: draw text with `(x, y)` as the top-left
    /// corner of its first character cell.
    Text {
        text: String,
        x: i32,
        y: i32,
        color: u8,
    },
}

/// Commands recorded by the `draw()` call in progress.
#[derive(Debug, Default)]
pub(crate) struct Recording {
    pub(crate) commands: Vec<DrawCommand>,
    /// Draw cost spent so far, charged against [`MAX_DRAW_COMMANDS`].
    cost: usize,
}

/// The recording of the `draw()` call in progress. `None` outside `draw()`,
/// which is how drawing functions reject other phases.
pub(crate) type PendingFrame = Rc<RefCell<Option<Recording>>>;

/// Installs `cls`, `pset`, `rectfill`, `spr` and `print`.
pub(crate) fn install(
    lua: &Lua,
    tostring: &Function,
    pending_frame: &PendingFrame,
) -> mlua::Result<()> {
    let globals = lua.globals();

    let pending = Rc::clone(pending_frame);
    let cls = lua.create_function(move |lua, color: Value| {
        expect_draw(lua, &pending, "cls")?;
        let color = color_arg(lua, "cls", 1, &color)?;
        record(lua, &pending, "cls", DrawCommand::Clear(color))
    })?;
    globals.raw_set("cls", cls)?;

    let pending = Rc::clone(pending_frame);
    let pset = lua.create_function(move |lua, (x, y, color): (Value, Value, Value)| {
        expect_draw(lua, &pending, "pset")?;
        let command = DrawCommand::Pixel {
            x: coordinate_arg(lua, "pset", 1, &x)?,
            y: coordinate_arg(lua, "pset", 2, &y)?,
            color: color_arg(lua, "pset", 3, &color)?,
        };
        record(lua, &pending, "pset", command)
    })?;
    globals.raw_set("pset", pset)?;

    let pending = Rc::clone(pending_frame);
    let rectfill = lua.create_function(
        move |lua, (x, y, width, height, color): (Value, Value, Value, Value, Value)| {
            expect_draw(lua, &pending, "rectfill")?;
            let command = DrawCommand::Rect {
                x: coordinate_arg(lua, "rectfill", 1, &x)?,
                y: coordinate_arg(lua, "rectfill", 2, &y)?,
                width: size_arg(lua, "rectfill", 3, &width, "width")?,
                height: size_arg(lua, "rectfill", 4, &height, "height")?,
                color: color_arg(lua, "rectfill", 5, &color)?,
            };
            record(lua, &pending, "rectfill", command)
        },
    )?;
    globals.raw_set("rectfill", rectfill)?;

    let pending = Rc::clone(pending_frame);
    let spr = lua.create_function(
        move |lua, (tile, x, y, w, h): (Value, Value, Value, Value, Value)| {
            expect_draw(lua, &pending, "spr")?;
            let tile = tile_arg(lua, "spr", 1, &tile)?;
            let x = coordinate_arg(lua, "spr", 2, &x)?;
            let y = coordinate_arg(lua, "spr", 3, &y)?;
            let column = usize::from(tile) % OBJ_GRID_COLUMNS;
            let row = usize::from(tile) / OBJ_GRID_COLUMNS;
            let width = tile_span_arg(lua, 4, &w, "width", column, OBJ_GRID_COLUMNS, "column")?;
            let height = tile_span_arg(lua, 5, &h, "height", row, OBJ_GRID_ROWS, "row")?;
            let command = DrawCommand::Sprite {
                tile,
                x,
                y,
                width,
                height,
            };
            record(lua, &pending, "spr", command)
        },
    )?;
    globals.raw_set("spr", spr)?;

    let pending = Rc::clone(pending_frame);
    let tostring = tostring.clone();
    let print = lua.create_function(
        move |lua, (text, x, y, color): (Value, Value, Value, Value)| {
            expect_draw(lua, &pending, "print")?;
            let command = DrawCommand::Text {
                x: coordinate_arg(lua, "print", 2, &x)?,
                y: coordinate_arg(lua, "print", 3, &y)?,
                color: color_arg(lua, "print", 4, &color)?,
                // Converted last: a `__tostring` metamethod runs script
                // code, which must not see a half-validated call.
                text: to_text(lua, &tostring, text, "print")?,
            };
            record(lua, &pending, "print", command)
        },
    )?;
    globals.raw_set("print", print)
}

/// Fails unless a `draw()` call is in progress.
fn expect_draw(lua: &Lua, pending: &PendingFrame, function: &str) -> mlua::Result<()> {
    if pending.borrow().is_some() {
        Ok(())
    } else {
        Err(outside_draw(lua, function))
    }
}

fn outside_draw(lua: &Lua, function: &str) -> mlua::Error {
    located(
        lua,
        format!("'{function}' can only be called during draw()"),
    )
}

/// Appends `command` to the frame in progress.
///
/// Callers check [`expect_draw`] before validating arguments, so a script
/// calling from the wrong phase hears about the phase first.
fn record(
    lua: &Lua,
    pending: &PendingFrame,
    function: &str,
    command: DrawCommand,
) -> mlua::Result<()> {
    let mut pending = pending.borrow_mut();
    let Some(recording) = pending.as_mut() else {
        return Err(outside_draw(lua, function));
    };
    let cost = match &command {
        DrawCommand::Text { text, .. } => {
            text.chars().count().max(1).saturating_mul(TEXT_CHAR_COST)
        }
        // Grid-sized blocks cannot overflow; if one did, its cost must still
        // exceed the limit rather than wrap below it.
        DrawCommand::Sprite { width, height, .. } => usize::from(*width)
            .checked_mul(usize::from(*height))
            .and_then(|tiles| tiles.checked_mul(SPRITE_TILE_COST))
            .unwrap_or(usize::MAX),
        DrawCommand::Clear(_) | DrawCommand::Pixel { .. } | DrawCommand::Rect { .. } => 1,
    };
    recording.cost = recording.cost.saturating_add(cost);
    if recording.cost > MAX_DRAW_COMMANDS {
        return Err(located(
            lua,
            format!("too many draw commands in one frame (limit {MAX_DRAW_COMMANDS})"),
        ));
    }
    recording.commands.push(command);
    Ok(())
}
