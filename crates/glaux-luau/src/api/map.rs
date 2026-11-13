//! The `map` table, which reads the cartridge's maps and copies regions of
//! them into the background tilemap windows.
//!
//! The maps are a read-only copy kept outside the hardware, so writes to VRAM
//! never change them. Like the memory functions, a call checks its arguments
//! before it charges the budget and charges before it writes. A rejected call
//! costs nothing, and one the budget cannot cover writes nothing.

use std::rc::Rc;
use std::sync::Arc;

use glaux_machine::map_store::{MapData, MapStore};
use glaux_machine::tilemap::{BgLayer, TILE_ENTRY_BYTES, TILE_WINDOW_SIDE, TileEntry};
use glaux_script::{HardwareSlot, LoanError};
use mlua::{Lua, MultiValue, Value};

use crate::args::{
    bad_argument, bounded_arg, layer_arg, located, script_type_name, world_cell_arg,
};
use crate::runtime::Meter;

/// Budget units for `map.size` and `map.get`.
const LOOKUP_COST: u64 = 1;

/// Fixed budget units for each `map.load` and `map.copy`.
const TRANSFER_COST: u64 = 1;

/// Budget units per byte written, the same rate as `memcpy`. A full window
/// is 8 KiB, so reloading it costs about as much as copying 8 KiB.
const BYTE_COST: u64 = 1;

#[derive(Clone)]
struct Maps {
    hardware: HardwareSlot,
    meter: Rc<Meter>,
    store: Arc<MapStore>,
}

impl Maps {
    fn map(
        &self,
        lua: &Lua,
        function: &str,
        position: usize,
        id: &Value,
    ) -> mlua::Result<&MapData> {
        let Value::String(name) = id else {
            return Err(bad_argument(
                lua,
                function,
                position,
                &format!("string expected, got {}", script_type_name(id)),
            ));
        };
        let name = name.to_string_lossy();
        self.store
            .get(&name)
            .ok_or_else(|| bad_argument(lua, function, position, &format!("no map named '{name}'")))
    }

    fn charge(&self, lua: &Lua, cost: u64) -> mlua::Result<()> {
        if self.meter.charge(cost) {
            Ok(())
        } else {
            Err(located(lua, "execution limit exceeded".to_owned()))
        }
    }

    fn transfer(&self, lua: &Lua, function: &str, arguments: Transfer<'_>) -> mlua::Result<()> {
        let map = self.map(lua, function, 1, arguments.id)?;
        let window = layer_arg(lua, function, 2, arguments.window, "background layer")?;
        let source = match arguments.source {
            Value::Nil => window,
            other => layer_arg(lua, function, arguments.source_position, other, "map layer")?,
        };
        let (x, y) = (
            arguments.x.get(lua, function)?,
            arguments.y.get(lua, function)?,
        );
        let (width, height) = arguments.size(lua, function)?;
        let cells = (width * height) as u64;
        self.charge(
            lua,
            TRANSFER_COST + cells * TILE_ENTRY_BYTES as u64 * BYTE_COST,
        )?;
        self.hardware
            .with(|hardware| {
                hardware.tilemaps[window.index()].transfer(
                    map.layer(source),
                    i64::from(x),
                    i64::from(y),
                    width,
                    height,
                );
            })
            .ok_or_else(|| located(lua, format!("internal error: {}", LoanError::Unavailable)))
    }
}

/// The arguments of `map.load` and `map.copy`, still unchecked.
struct Transfer<'a> {
    id: &'a Value,
    window: &'a Value,
    x: Coordinate<'a>,
    y: Coordinate<'a>,
    /// `None` for a full window, the width and height otherwise.
    extent: Option<(&'a Value, &'a Value)>,
    source: &'a Value,
    source_position: usize,
}

impl Transfer<'_> {
    fn size(&self, lua: &Lua, function: &str) -> mlua::Result<(usize, usize)> {
        let Some((width, height)) = self.extent else {
            return Ok((TILE_WINDOW_SIDE, TILE_WINDOW_SIDE));
        };
        let side = TILE_WINDOW_SIDE as u32;
        let width = bounded_arg(lua, function, 5, width, "width", side)?;
        let height = bounded_arg(lua, function, 6, height, "height", side)?;
        if width == 0 || height == 0 {
            return Err(bad_argument(
                lua,
                function,
                if width == 0 { 5 } else { 6 },
                &format!("size 1-{side} expected, got 0"),
            ));
        }
        Ok((width as usize, height as usize))
    }
}

/// A world coordinate argument, where `nil` may stand for 0.
struct Coordinate<'a> {
    value: &'a Value,
    position: usize,
    optional: bool,
}

impl Coordinate<'_> {
    fn get(&self, lua: &Lua, function: &str) -> mlua::Result<i32> {
        match self.value {
            Value::Nil if self.optional => Ok(0),
            value => world_cell_arg(lua, function, self.position, value),
        }
    }
}

/// Installs the `map` table.
pub(crate) fn install(
    lua: &Lua,
    hardware: &HardwareSlot,
    meter: &Rc<Meter>,
    store: &Arc<MapStore>,
) -> mlua::Result<()> {
    let table = lua.create_table()?;
    let maps = Maps {
        hardware: hardware.clone(),
        meter: Rc::clone(meter),
        store: Arc::clone(store),
    };

    let shared = maps.clone();
    let load = lua.create_function(move |lua, args: MultiValue| {
        const NAME: &str = "map.load";
        let [id, window, x, y, source] = &arguments(&args);
        shared.transfer(
            lua,
            NAME,
            Transfer {
                id,
                window,
                x: Coordinate {
                    value: x,
                    position: 3,
                    optional: true,
                },
                y: Coordinate {
                    value: y,
                    position: 4,
                    optional: true,
                },
                extent: None,
                source,
                source_position: 5,
            },
        )
    })?;
    table.raw_set("load", load)?;

    let shared = maps.clone();
    let copy = lua.create_function(move |lua, args: MultiValue| {
        const NAME: &str = "map.copy";
        let [id, window, x, y, width, height, source] = &arguments(&args);
        shared.transfer(
            lua,
            NAME,
            Transfer {
                id,
                window,
                x: Coordinate {
                    value: x,
                    position: 3,
                    optional: false,
                },
                y: Coordinate {
                    value: y,
                    position: 4,
                    optional: false,
                },
                extent: Some((width, height)),
                source,
                source_position: 7,
            },
        )
    })?;
    table.raw_set("copy", copy)?;

    let shared = maps.clone();
    let size = lua.create_function(move |lua, id: Value| {
        let map = shared.map(lua, "map.size", 1, &id)?;
        shared.charge(lua, LOOKUP_COST)?;
        Ok((map.width() as u32, map.height() as u32))
    })?;
    table.raw_set("size", size)?;

    let get = lua.create_function(
        move |lua, (id, layer, x, y): (Value, Value, Value, Value)| {
            const NAME: &str = "map.get";
            let map = maps.map(lua, NAME, 1, &id)?;
            let layer = layer_arg(lua, NAME, 2, &layer, "map layer")?;
            let x = world_cell_arg(lua, NAME, 3, &x)?;
            let y = world_cell_arg(lua, NAME, 4, &y)?;
            maps.charge(lua, LOOKUP_COST)?;
            let entry = cell(map, layer, x, y);
            Ok((u32::from(entry.tile()), u32::from(entry.raw())))
        },
    )?;
    table.raw_set("get", get)?;

    lua.globals().raw_set("map", table)
}

/// The entry of `map` at world cell `(x, y)`, which is empty outside it.
fn cell(map: &MapData, layer: BgLayer, x: i32, y: i32) -> TileEntry {
    usize::try_from(x)
        .ok()
        .zip(usize::try_from(y).ok())
        .and_then(|(x, y)| map.layer(layer).get(x, y))
        .unwrap_or_default()
}

/// The first `N` arguments of a call, with `nil` for the ones left out.
fn arguments<const N: usize>(args: &MultiValue) -> [Value; N] {
    std::array::from_fn(|index| args.get(index).cloned().unwrap_or(Value::Nil))
}
