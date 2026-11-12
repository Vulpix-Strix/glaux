//! `peek`, `poke`, `peek16`, `poke16`, `peek32`, `poke32`, `memcpy` and
//! `memset` on the lent hardware.
//!
//! A call validates its arguments, checks the ranges with the bus's own rule,
//! charges the budget and only then touches the bus. A rejected call costs
//! nothing, and one the budget cannot cover writes nothing.

use std::rc::Rc;

use glaux_machine::bus::{BusError, check_range};
use glaux_machine::hardware::CartridgeHardware;
use glaux_script::{HardwareSlot, LoanError};
use mlua::{Lua, Value};

use crate::args::{bounded_arg, located};
use crate::runtime::Meter;

/// Budget units for each `peek` and `poke`, on top of the interrupt check
/// the call itself costs.
const ACCESS_COST: u64 = 1;

/// Fixed budget units for each `memcpy` and `memset`.
const BLOCK_COST: u64 = 1;

/// Bytes `memcpy` moves per budget unit. The rates of copy and fill are set
/// so that a call spending its whole budget on memory takes no longer than
/// one spent on an empty loop. A copy reads and writes every byte, so it
/// costs about twice a fill.
const COPY_BYTES_PER_UNIT: u64 = 1;

/// Bytes `memset` writes per budget unit.
const FILL_BYTES_PER_UNIT: u64 = 2;

/// The slot and the meter every memory function uses.
#[derive(Clone)]
struct Memory {
    hardware: HardwareSlot,
    meter: Rc<Meter>,
}

impl Memory {
    /// Checks each of `ranges`, charges `cost` and runs `access` on the lent
    /// hardware.
    fn run<T>(
        &self,
        lua: &Lua,
        function: &str,
        ranges: &[(u32, u32)],
        cost: u64,
        access: impl FnOnce(&mut CartridgeHardware) -> Result<T, BusError>,
    ) -> mlua::Result<T> {
        for &(start, length) in ranges {
            check_range(start, length).map_err(|error| bus_error(lua, function, error))?;
        }
        if !self.meter.charge(cost) {
            return Err(located(lua, "execution limit exceeded".to_owned()));
        }
        self.hardware
            .with(access)
            .ok_or_else(|| located(lua, format!("internal error: {}", LoanError::Unavailable)))?
            .map_err(|error| bus_error(lua, function, error))
    }
}

fn bus_error(lua: &Lua, function: &str, error: BusError) -> mlua::Error {
    located(lua, format!("'{function}': {error}"))
}

/// Budget units for a block of `length` bytes.
fn block_cost(length: u32, bytes_per_unit: u64) -> u64 {
    BLOCK_COST.saturating_add(u64::from(length).div_ceil(bytes_per_unit))
}

/// Installs the eight memory functions.
pub(crate) fn install(lua: &Lua, hardware: &HardwareSlot, meter: &Rc<Meter>) -> mlua::Result<()> {
    let globals = lua.globals();
    let memory = Memory {
        hardware: hardware.clone(),
        meter: Rc::clone(meter),
    };

    let shared = memory.clone();
    let peek = lua.create_function(move |lua, address: Value| {
        let address = bounded_arg(lua, "peek", 1, &address, "address", u32::MAX)?;
        shared.run(lua, "peek", &[(address, 1)], ACCESS_COST, |hw| {
            hw.read(address)
        })
    })?;
    globals.raw_set("peek", peek)?;

    let shared = memory.clone();
    let poke = lua.create_function(move |lua, (address, value): (Value, Value)| {
        let address = bounded_arg(lua, "poke", 1, &address, "address", u32::MAX)?;
        let value = bounded_arg(lua, "poke", 2, &value, "value", u32::from(u8::MAX))? as u8;
        shared.run(lua, "poke", &[(address, 1)], ACCESS_COST, |hw| {
            hw.write(address, value)
        })
    })?;
    globals.raw_set("poke", poke)?;

    let shared = memory.clone();
    let peek16 = lua.create_function(move |lua, address: Value| {
        let address = bounded_arg(lua, "peek16", 1, &address, "address", u32::MAX)?;
        shared.run(lua, "peek16", &[(address, 2)], ACCESS_COST, |hw| {
            hw.read16(address)
        })
    })?;
    globals.raw_set("peek16", peek16)?;

    let shared = memory.clone();
    let poke16 = lua.create_function(move |lua, (address, value): (Value, Value)| {
        let address = bounded_arg(lua, "poke16", 1, &address, "address", u32::MAX)?;
        let value = bounded_arg(lua, "poke16", 2, &value, "value", u32::from(u16::MAX))? as u16;
        shared.run(lua, "poke16", &[(address, 2)], ACCESS_COST, |hw| {
            hw.write16(address, value)
        })
    })?;
    globals.raw_set("poke16", poke16)?;

    let shared = memory.clone();
    let peek32 = lua.create_function(move |lua, address: Value| {
        let address = bounded_arg(lua, "peek32", 1, &address, "address", u32::MAX)?;
        shared.run(lua, "peek32", &[(address, 4)], ACCESS_COST, |hw| {
            hw.read32(address)
        })
    })?;
    globals.raw_set("peek32", peek32)?;

    let shared = memory.clone();
    let poke32 = lua.create_function(move |lua, (address, value): (Value, Value)| {
        let address = bounded_arg(lua, "poke32", 1, &address, "address", u32::MAX)?;
        let value = bounded_arg(lua, "poke32", 2, &value, "value", u32::MAX)?;
        shared.run(lua, "poke32", &[(address, 4)], ACCESS_COST, |hw| {
            hw.write32(address, value)
        })
    })?;
    globals.raw_set("poke32", poke32)?;

    let shared = memory.clone();
    let memcpy = lua.create_function(
        move |lua, (destination, source, length): (Value, Value, Value)| {
            let destination = bounded_arg(lua, "memcpy", 1, &destination, "address", u32::MAX)?;
            let source = bounded_arg(lua, "memcpy", 2, &source, "address", u32::MAX)?;
            let length = bounded_arg(lua, "memcpy", 3, &length, "length", u32::MAX)?;
            shared.run(
                lua,
                "memcpy",
                &[(destination, length), (source, length)],
                block_cost(length, COPY_BYTES_PER_UNIT),
                |hw| hw.copy(destination, source, length),
            )
        },
    )?;
    globals.raw_set("memcpy", memcpy)?;

    let memset = lua.create_function(
        move |lua, (destination, value, length): (Value, Value, Value)| {
            let destination = bounded_arg(lua, "memset", 1, &destination, "address", u32::MAX)?;
            let value = bounded_arg(lua, "memset", 2, &value, "value", u32::from(u8::MAX))? as u8;
            let length = bounded_arg(lua, "memset", 3, &length, "length", u32::MAX)?;
            memory.run(
                lua,
                "memset",
                &[(destination, length)],
                block_cost(length, FILL_BYTES_PER_UNIT),
                |hw| hw.fill(destination, value, length),
            )
        },
    )?;
    globals.raw_set("memset", memset)?;
    Ok(())
}
