//! `cls`, `pset`, `rectfill`, `spr` and `text`, which run
//! [`DrawCommand`]s on the lent hardware during `draw()`.

use std::cell::RefCell;
use std::rc::Rc;

use glaux_machine::font::BitmapFont;
use glaux_machine::sprite::{OBJ_GRID_COLUMNS, OBJ_GRID_ROWS};
use glaux_script::{DrawBudget, DrawCommand, HardwareSlot, LoanError};
use rhai::{Dynamic, Engine, NativeCallContext};

use crate::args::{
    Result, color_arg, coordinate_arg, flag_arg, located, obj_bank_arg, size_arg, tile_arg,
    tile_span_arg, to_text,
};

/// The draw budget of the `draw()` call in progress. `None` outside `draw()`,
/// which is how drawing functions reject other phases.
pub(crate) type PendingFrame = Rc<RefCell<Option<DrawBudget>>>;

/// What a drawing function needs to run a command.
#[derive(Clone)]
struct Canvas {
    pending: PendingFrame,
    hardware: HardwareSlot,
    font: BitmapFont,
}

/// Registers `cls`, `pset`, `rectfill`, `spr` and `text`.
pub(crate) fn install(
    engine: &mut Engine,
    pending_frame: &PendingFrame,
    hardware: &HardwareSlot,
    font: &BitmapFont,
) {
    let shared = Canvas {
        pending: Rc::clone(pending_frame),
        hardware: hardware.clone(),
        font: font.clone(),
    };
    let canvas = shared.clone();
    engine.register_fn(
        "cls",
        move |ctx: NativeCallContext, color: Dynamic| -> Result<()> {
            expect_draw(&ctx, &canvas.pending, "cls")?;
            let color = color_arg(&ctx, "cls", 1, &color)?;
            record(&ctx, &canvas, "cls", DrawCommand::Clear(color))
        },
    );

    let canvas = shared.clone();
    engine.register_fn(
        "pset",
        move |ctx: NativeCallContext, x: Dynamic, y: Dynamic, color: Dynamic| -> Result<()> {
            expect_draw(&ctx, &canvas.pending, "pset")?;
            let command = DrawCommand::Pixel {
                x: coordinate_arg(&ctx, "pset", 1, &x)?,
                y: coordinate_arg(&ctx, "pset", 2, &y)?,
                color: color_arg(&ctx, "pset", 3, &color)?,
            };
            record(&ctx, &canvas, "pset", command)
        },
    );

    let canvas = shared.clone();
    engine.register_fn(
        "rectfill",
        move |ctx: NativeCallContext,
              x: Dynamic,
              y: Dynamic,
              width: Dynamic,
              height: Dynamic,
              color: Dynamic|
              -> Result<()> {
            expect_draw(&ctx, &canvas.pending, "rectfill")?;
            let command = DrawCommand::Rect {
                x: coordinate_arg(&ctx, "rectfill", 1, &x)?,
                y: coordinate_arg(&ctx, "rectfill", 2, &y)?,
                width: size_arg(&ctx, "rectfill", 3, &width, "width")?,
                height: size_arg(&ctx, "rectfill", 4, &height, "height")?,
                color: color_arg(&ctx, "rectfill", 5, &color)?,
            };
            record(&ctx, &canvas, "rectfill", command)
        },
    );

    install_spr(engine, &shared);

    let canvas = shared.clone();
    engine.register_fn(
        "text",
        move |ctx: NativeCallContext,
              text: Dynamic,
              x: Dynamic,
              y: Dynamic,
              color: Dynamic|
              -> Result<()> {
            expect_draw(&ctx, &canvas.pending, "text")?;
            let command = DrawCommand::Text {
                x: coordinate_arg(&ctx, "text", 2, &x)?,
                y: coordinate_arg(&ctx, "text", 3, &y)?,
                color: color_arg(&ctx, "text", 4, &color)?,
                text: to_text(&text),
            };
            record(&ctx, &canvas, "text", command)
        },
    );
}

/// Registers `spr(tile, x, y, [w, h, flip_x, flip_y, palette])` for every
/// argument count from 3 to 8. Rhai has no optional parameters. A missing
/// argument behaves as `()`, which also selects the default when given.
fn install_spr(engine: &mut Engine, shared: &Canvas) {
    let p = shared.clone();
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
    let p = shared.clone();
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
    let p = shared.clone();
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
    let p = shared.clone();
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
    let p = shared.clone();
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
    let p = shared.clone();
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
fn spr(ctx: &NativeCallContext, canvas: &Canvas, args: [Dynamic; 8]) -> Result<()> {
    let [tile, x, y, w, h, flip_x, flip_y, palette] = args;
    expect_draw(ctx, &canvas.pending, "spr")?;
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
    record(ctx, canvas, "spr", command)
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

/// Charges `command` to the `draw()` call in progress and draws it on the
/// lent hardware.
///
/// Callers check [`expect_draw`] before validating arguments, so a script
/// calling from the wrong phase hears about the phase first. A command the
/// budget rejects is not drawn.
fn record(
    ctx: &NativeCallContext,
    canvas: &Canvas,
    function: &str,
    command: DrawCommand,
) -> Result<()> {
    {
        let mut pending = canvas.pending.borrow_mut();
        let Some(budget) = pending.as_mut() else {
            return Err(outside_draw(ctx, function));
        };
        budget
            .charge(&command)
            .map_err(|exceeded| located(ctx, exceeded.to_string()))?;
    }
    canvas
        .hardware
        .with(|hardware| command.execute(hardware, &canvas.font))
        .ok_or_else(|| located(ctx, format!("internal error: {}", LoanError::Unavailable)))
}
