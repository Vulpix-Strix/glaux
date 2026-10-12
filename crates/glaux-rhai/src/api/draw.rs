//! `cls`, `pset`, `rectfill`, `spr` and `text`, which record
//! [`DrawCommand`]s during `draw()`.

use std::cell::RefCell;
use std::rc::Rc;

use glaux_machine::sprite::{OBJ_GRID_COLUMNS, OBJ_GRID_ROWS};
use glaux_script::{DrawCommand, Recording};
use rhai::{Dynamic, Engine, NativeCallContext};

use crate::args::{
    Result, color_arg, coordinate_arg, flag_arg, located, obj_bank_arg, size_arg, tile_arg,
    tile_span_arg, to_text,
};

/// The recording of the `draw()` call in progress. `None` outside `draw()`,
/// which is how drawing functions reject other phases.
pub(crate) type PendingFrame = Rc<RefCell<Option<Recording>>>;

/// Registers `cls`, `pset`, `rectfill`, `spr` and `text`.
pub(crate) fn install(engine: &mut Engine, pending_frame: &PendingFrame) {
    let pending = Rc::clone(pending_frame);
    engine.register_fn(
        "cls",
        move |ctx: NativeCallContext, color: Dynamic| -> Result<()> {
            expect_draw(&ctx, &pending, "cls")?;
            let color = color_arg(&ctx, "cls", 1, &color)?;
            record(&ctx, &pending, "cls", DrawCommand::Clear(color))
        },
    );

    let pending = Rc::clone(pending_frame);
    engine.register_fn(
        "pset",
        move |ctx: NativeCallContext, x: Dynamic, y: Dynamic, color: Dynamic| -> Result<()> {
            expect_draw(&ctx, &pending, "pset")?;
            let command = DrawCommand::Pixel {
                x: coordinate_arg(&ctx, "pset", 1, &x)?,
                y: coordinate_arg(&ctx, "pset", 2, &y)?,
                color: color_arg(&ctx, "pset", 3, &color)?,
            };
            record(&ctx, &pending, "pset", command)
        },
    );

    let pending = Rc::clone(pending_frame);
    engine.register_fn(
        "rectfill",
        move |ctx: NativeCallContext,
              x: Dynamic,
              y: Dynamic,
              width: Dynamic,
              height: Dynamic,
              color: Dynamic|
              -> Result<()> {
            expect_draw(&ctx, &pending, "rectfill")?;
            let command = DrawCommand::Rect {
                x: coordinate_arg(&ctx, "rectfill", 1, &x)?,
                y: coordinate_arg(&ctx, "rectfill", 2, &y)?,
                width: size_arg(&ctx, "rectfill", 3, &width, "width")?,
                height: size_arg(&ctx, "rectfill", 4, &height, "height")?,
                color: color_arg(&ctx, "rectfill", 5, &color)?,
            };
            record(&ctx, &pending, "rectfill", command)
        },
    );

    install_spr(engine, pending_frame);

    let pending = Rc::clone(pending_frame);
    engine.register_fn(
        "text",
        move |ctx: NativeCallContext,
              text: Dynamic,
              x: Dynamic,
              y: Dynamic,
              color: Dynamic|
              -> Result<()> {
            expect_draw(&ctx, &pending, "text")?;
            let command = DrawCommand::Text {
                x: coordinate_arg(&ctx, "text", 2, &x)?,
                y: coordinate_arg(&ctx, "text", 3, &y)?,
                color: color_arg(&ctx, "text", 4, &color)?,
                text: to_text(&text),
            };
            record(&ctx, &pending, "text", command)
        },
    );
}

/// Registers `spr(tile, x, y, [w, h, flip_x, flip_y, palette])` for every
/// argument count from 3 to 8. Rhai has no optional parameters; a missing
/// argument behaves as `()`, which also selects the default when given.
fn install_spr(engine: &mut Engine, pending_frame: &PendingFrame) {
    let p = Rc::clone(pending_frame);
    engine.register_fn(
        "spr",
        move |ctx: NativeCallContext, tile: Dynamic, x: Dynamic, y: Dynamic| -> Result<()> {
            spr(
                &ctx,
                &p,
                [
                    tile,
                    x,
                    y,
                    ().into(),
                    ().into(),
                    ().into(),
                    ().into(),
                    ().into(),
                ],
            )
        },
    );
    let p = Rc::clone(pending_frame);
    engine.register_fn(
        "spr",
        move |ctx: NativeCallContext,
              tile: Dynamic,
              x: Dynamic,
              y: Dynamic,
              w: Dynamic|
              -> Result<()> {
            spr(
                &ctx,
                &p,
                [tile, x, y, w, ().into(), ().into(), ().into(), ().into()],
            )
        },
    );
    let p = Rc::clone(pending_frame);
    engine.register_fn(
        "spr",
        move |ctx: NativeCallContext,
              tile: Dynamic,
              x: Dynamic,
              y: Dynamic,
              w: Dynamic,
              h: Dynamic|
              -> Result<()> {
            spr(
                &ctx,
                &p,
                [tile, x, y, w, h, ().into(), ().into(), ().into()],
            )
        },
    );
    let p = Rc::clone(pending_frame);
    engine.register_fn(
        "spr",
        move |ctx: NativeCallContext,
              tile: Dynamic,
              x: Dynamic,
              y: Dynamic,
              w: Dynamic,
              h: Dynamic,
              flip_x: Dynamic|
              -> Result<()> {
            spr(&ctx, &p, [tile, x, y, w, h, flip_x, ().into(), ().into()])
        },
    );
    let p = Rc::clone(pending_frame);
    engine.register_fn(
        "spr",
        move |ctx: NativeCallContext,
              tile: Dynamic,
              x: Dynamic,
              y: Dynamic,
              w: Dynamic,
              h: Dynamic,
              flip_x: Dynamic,
              flip_y: Dynamic|
              -> Result<()> {
            spr(&ctx, &p, [tile, x, y, w, h, flip_x, flip_y, ().into()])
        },
    );
    let p = Rc::clone(pending_frame);
    engine.register_fn(
        "spr",
        move |ctx: NativeCallContext,
              tile: Dynamic,
              x: Dynamic,
              y: Dynamic,
              w: Dynamic,
              h: Dynamic,
              flip_x: Dynamic,
              flip_y: Dynamic,
              palette: Dynamic|
              -> Result<()> { spr(&ctx, &p, [tile, x, y, w, h, flip_x, flip_y, palette]) },
    );
}

/// `spr` with all eight arguments, `()` standing for those not given.
fn spr(ctx: &NativeCallContext, pending: &PendingFrame, args: [Dynamic; 8]) -> Result<()> {
    let [tile, x, y, w, h, flip_x, flip_y, palette] = args;
    expect_draw(ctx, pending, "spr")?;
    let tile = tile_arg(ctx, "spr", 1, &tile)?;
    let x = coordinate_arg(ctx, "spr", 2, &x)?;
    let y = coordinate_arg(ctx, "spr", 3, &y)?;
    let column = usize::from(tile) % OBJ_GRID_COLUMNS;
    let row = usize::from(tile) / OBJ_GRID_COLUMNS;
    let width = tile_span_arg(ctx, 4, &w, "width", column, OBJ_GRID_COLUMNS, "column")?;
    let height = tile_span_arg(ctx, 5, &h, "height", row, OBJ_GRID_ROWS, "row")?;
    let command = DrawCommand::Sprite {
        tile,
        x,
        y,
        width,
        height,
        flip_x: flag_arg(ctx, "spr", 6, &flip_x)?,
        flip_y: flag_arg(ctx, "spr", 7, &flip_y)?,
        palette: obj_bank_arg(ctx, "spr", 8, &palette)?,
    };
    record(ctx, pending, "spr", command)
}

/// Fails unless a `draw()` call is in progress.
fn expect_draw(ctx: &NativeCallContext, pending: &PendingFrame, function: &str) -> Result<()> {
    if pending.borrow().is_some() {
        Ok(())
    } else {
        Err(outside_draw(ctx, function))
    }
}

fn outside_draw(ctx: &NativeCallContext, function: &str) -> Box<rhai::EvalAltResult> {
    located(
        ctx,
        format!("'{function}' can only be called during draw()"),
    )
}

/// Appends `command` to the frame in progress.
///
/// Callers check [`expect_draw`] before validating arguments, so a script
/// calling from the wrong phase hears about the phase first.
fn record(
    ctx: &NativeCallContext,
    pending: &PendingFrame,
    function: &str,
    command: DrawCommand,
) -> Result<()> {
    let mut pending = pending.borrow_mut();
    let Some(recording) = pending.as_mut() else {
        return Err(outside_draw(ctx, function));
    };
    recording
        .push(command)
        .map_err(|exceeded| located(ctx, exceeded.to_string()))
}
