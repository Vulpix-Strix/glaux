//! `cls`, `pset`, `rectfill`, `spr` and `print`, which record
//! [`DrawCommand`]s during `draw()`.

use std::cell::RefCell;
use std::rc::Rc;

use glaux_machine::sprite::{OBJ_GRID_COLUMNS, OBJ_GRID_ROWS};
use glaux_script::{DrawCommand, Recording};
use mlua::{Function, Lua, Value};

use crate::args::{
    color_arg, coordinate_arg, flag_arg, located, obj_bank_arg, size_arg, tile_arg, tile_span_arg,
    to_text,
};

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
        move |lua,
              (tile, x, y, w, h, flip_x, flip_y, palette): (
            Value,
            Value,
            Value,
            Value,
            Value,
            Value,
            Value,
            Value,
        )| {
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
                flip_x: flag_arg(lua, "spr", 6, &flip_x)?,
                flip_y: flag_arg(lua, "spr", 7, &flip_y)?,
                palette: obj_bank_arg(lua, "spr", 8, &palette)?,
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
    recording
        .push(command)
        .map_err(|exceeded| located(lua, exceeded.to_string()))
}
