//! The `map` functions as scripts see them. The transfer itself is tested in
//! `glaux-machine`, so these tests cover argument handling, errors, cost and
//! the phases the functions run in.

use std::sync::Arc;

use glaux_machine::map_store::{MapData, MapStore};
use glaux_machine::tilemap::{BgBank, TileEntry, TileLayer};
use pretty_assertions::assert_eq;

use super::*;

fn entry(tile: u16) -> TileEntry {
    TileEntry::new(tile, false, false, false, BgBank::new(1).unwrap()).unwrap()
}

/// The tile of cell `(x, y)` in a layer `width` cells wide. The count wraps
/// at 700 so even the biggest map stays inside the 1024 tiles.
fn tile_of(base: u16, width: usize, x: usize, y: usize) -> u16 {
    base + ((y * width + x) % 700) as u16
}

fn numbered(width: usize, height: usize, base: u16) -> TileLayer {
    let mut layer = TileLayer::empty(width, height).unwrap();
    for y in 0..height {
        for x in 0..width {
            layer.set(x, y, entry(tile_of(base, width, x, y)));
        }
    }
    layer
}

/// `town` is 6 x 4 and `big` is 100 x 70, each layer numbered from its own
/// base so a copy from the wrong layer shows.
fn store() -> Arc<MapStore> {
    let mut store = MapStore::default();
    for (id, width, height) in [("town", 6, 4), ("big", 100, 70)] {
        let layers = [0, 100, 200].map(|base| numbered(width, height, base));
        store.insert(id, MapData::new(layers).unwrap());
    }
    Arc::new(store)
}

fn start_with_budget(source: &str, budget: Budget) -> Result<Runtime, StartFailure> {
    Runtime::start_on_scratch_with_maps(
        "main.luau",
        source.as_bytes(),
        Files::default(),
        budget,
        font(),
        store(),
    )
}

fn start(source: &str) -> Runtime {
    match start_with_budget(source, Budget::default()) {
        Ok(runtime) => runtime,
        Err(failure) => panic!("expected the script to start, got {failure:?}"),
    }
}

/// Runs `body` as `tick()` and returns the runtime for inspecting VRAM.
fn ticked(body: &str) -> Runtime {
    let mut runtime = start(&format!("function tick() {body} end"));
    runtime.tick(NO_INPUT).unwrap();
    runtime
}

/// The message of the runtime error `body` raises in `tick()`, and the
/// runtime, whose windows must be unchanged.
fn tick_error(body: &str) -> (String, Runtime) {
    let mut runtime = start(&format!("function tick() {body} end"));
    match runtime.tick(NO_INPUT) {
        Err(RuntimeError {
            phase: Phase::Tick,
            error: ScriptError::Runtime(message),
        }) => (message, runtime),
        other => panic!("`{body}` should raise an error in tick, got {other:?}"),
    }
}

fn window(runtime: &Runtime, layer: usize) -> Vec<u8> {
    runtime.scratch().tilemaps[layer].as_bytes().to_vec()
}

fn tile_at(runtime: &Runtime, layer: usize, x: usize, y: usize) -> u16 {
    runtime.scratch().tilemaps[layer]
        .entry(x, y)
        .unwrap()
        .tile()
}

fn blank(runtime: &Runtime) -> bool {
    (0..3).all(|layer| window(runtime, layer).iter().all(|&byte| byte == 0))
}

#[test]
fn load_fills_the_window_from_the_origin_and_leaves_the_other_windows_alone() {
    let runtime = ticked("map.load('town', 1)");
    assert_eq!(tile_at(&runtime, 1, 0, 0), 100);
    assert_eq!(tile_at(&runtime, 1, 5, 3), 100 + 3 * 6 + 5);
    assert_eq!(tile_at(&runtime, 1, 6, 0), 0);
    assert!(window(&runtime, 0).iter().all(|&byte| byte == 0));
    assert!(window(&runtime, 2).iter().all(|&byte| byte == 0));
}

#[test]
fn the_optional_arguments_of_load_default_to_the_origin_and_the_same_layer() {
    let calls = [
        "map.load('big', 2)",
        "map.load('big', 2, 0)",
        "map.load('big', 2, 0, 0)",
        "map.load('big', 2, nil, nil, 2)",
        "map.load('big', 2, nil, 0, nil)",
        "map.load('big', 2, 0, nil, 2)",
    ];
    let reference = window(&ticked(calls[0]), 2);
    assert_eq!(tile_at(&ticked(calls[0]), 2, 7, 1), 200 + 100 + 7);
    for call in calls {
        assert_eq!(window(&ticked(call), 2), reference, "{call}");
    }
}

#[test]
fn a_source_layer_other_than_the_window_is_read_when_given() {
    let runtime = ticked("map.load('town', 0, nil, nil, 2)");
    assert_eq!(tile_at(&runtime, 0, 1, 0), 201);
    let runtime = ticked("map.load('town', 2, 1, 1, 0)");
    assert_eq!(tile_at(&runtime, 2, 1, 1), 7);
    assert_eq!(tile_at(&runtime, 2, 0, 0), 0);
}

#[test]
fn load_takes_the_window_from_a_world_origin_and_wraps_the_physical_cells() {
    let runtime = ticked("map.load('big', 0, 70, 3)");
    // World cell (70, 3) is physical cell (6, 3).
    assert_eq!(tile_at(&runtime, 0, 6, 3), 3 * 100 + 70);
    // The map ends at column 99, which is physical column 35.
    assert_eq!(tile_at(&runtime, 0, 35, 3), 3 * 100 + 99);
    assert_eq!(tile_at(&runtime, 0, 36, 3), 0);
}

#[test]
fn copy_transfers_a_region_for_streaming() {
    let runtime = ticked("map.copy('big', 1, 64, 0, 1, 64) map.copy('big', 1, 0, 66, 64, 1)");
    // A column of world x = 64 lands on physical column 0.
    assert_eq!(tile_at(&runtime, 1, 0, 5), 100 + 5 * 100 + 64);
    // A row of world y = 66 lands on physical row 2.
    assert_eq!(tile_at(&runtime, 1, 9, 2), tile_of(100, 100, 9, 66));
    assert_eq!(tile_at(&runtime, 1, 1, 5), 0);
}

#[test]
fn copy_accepts_negative_and_extreme_coordinates_and_writes_empty_cells() {
    let runtime = ticked(
        "map.load('big', 0)
        map.copy('big', 0, -2, -1, 4, 2)
        map.copy('big', 2, 2147483647, -2147483648, 64, 64)
        map.copy('big', 2, -2147483648, 2147483647, 64, 64)",
    );
    assert_eq!(tile_at(&runtime, 0, 62, 63), 0);
    assert_eq!(tile_at(&runtime, 0, 0, 0), 0);
    assert_eq!(tile_at(&runtime, 0, 1, 0), 1);
    assert!(window(&runtime, 2).iter().all(|&byte| byte == 0));
}

#[test]
fn size_and_get_read_the_source_even_when_it_is_not_in_vram() {
    let mut runtime = start(
        "function tick()
            local w, h = map.size('big')
            local tile, entry = map.get('big', 1, 5, 2)
            local outside, raw = map.get('big', 1, 100, -1)
            log.info(w, h, tile, entry, outside, raw)
        end",
    );
    runtime.tick(NO_INPUT).unwrap();
    let expected = entry(100 + 2 * 100 + 5).raw();
    assert_eq!(
        texts(runtime.take_output()),
        [format!("100\t70\t{}\t{expected}\t0\t0", 100 + 2 * 100 + 5)]
    );
    assert!(blank(&runtime));
}

#[test]
fn writing_vram_does_not_change_the_map() {
    let mut runtime = start(
        "function tick()
            map.load('town', 0)
            poke16(0x34000, 0x0123)
            local tile = map.get('town', 0, 0, 0)
            log.info(tile, peek16(0x34000))
            map.load('town', 0)
            log.info(peek16(0x34000))
        end",
    );
    runtime.tick(NO_INPUT).unwrap();
    let reloaded = entry(0).raw();
    assert_eq!(
        texts(runtime.take_output()),
        ["0\t291", &format!("{reloaded}")]
    );
}

#[test]
fn the_functions_work_in_the_entry_script_and_in_boot() {
    let runtime = start("map.load('town', 0)");
    assert_eq!(tile_at(&runtime, 0, 2, 1), 8);
    let runtime = start("function boot() map.load('town', 1, nil, nil, 2) end");
    assert_eq!(tile_at(&runtime, 1, 2, 1), 208);
}

#[test]
fn a_map_is_not_loaded_unless_the_script_asks() {
    let runtime = start("function tick() end");
    assert!(blank(&runtime));
}

#[test]
fn bad_arguments_are_rejected_and_write_nothing() {
    let bad = [
        "map.load()",
        "map.load(nil, 0)",
        "map.load(1, 0)",
        "map.load('town')",
        "map.load('town', nil)",
        "map.load('town', 3)",
        "map.load('town', -1)",
        "map.load('town', 0.5)",
        "map.load('town', '0')",
        "map.load('town', 0, 1.5)",
        "map.load('town', 0, 0, 0.1)",
        "map.load('town', 0, 0/0)",
        "map.load('town', 0, math.huge)",
        "map.load('town', 0, 2147483648)",
        "map.load('town', 0, -2147483649)",
        "map.load('town', 0, false)",
        "map.load('town', 0, '1')",
        "map.load('town', 0, 0, {})",
        "map.load('town', 0, nil, nil, 3)",
        "map.load('town', 0, nil, nil, false)",
        "map.load('town', 0, nil, nil, -1)",
        "map.load('nowhere', 0)",
        "map.copy('town', 0, 0, 0, 0, 1)",
        "map.copy('town', 0, 0, 0, 1, 0)",
        "map.copy('town', 0, 0, 0, 65, 1)",
        "map.copy('town', 0, 0, 0, 1, 65)",
        "map.copy('town', 0, 0, 0, 1.5, 1)",
        "map.copy('town', 0, 0, 0, 1)",
        "map.copy('town', 0, nil, 0, 1, 1)",
        "map.copy('town', 0, 0, nil, 1, 1)",
        "map.copy('town', 0, 0, 0, 1, 1, 3)",
        "map.size('nowhere')",
        "map.size(nil)",
        "map.get('town', 3, 0, 0)",
        "map.get('town', 0, 0.5, 0)",
        "map.get('town', 0, 0)",
        "map.get('nowhere', 0, 0, 0)",
    ];
    for body in bad {
        let (message, runtime) = tick_error(body);
        assert!(message.contains("bad argument #"), "`{body}`: {message}");
        assert!(blank(&runtime), "`{body}` wrote something");
    }
}

#[test]
fn the_error_names_the_function_and_the_argument() {
    let (message, _) = tick_error("map.load('town', 3)");
    assert!(
        message.contains("bad argument #2 to 'map.load' (background layer 0-2 expected, got 3)"),
        "{message}"
    );
    let (message, _) = tick_error("map.load('nowhere', 0)");
    assert!(
        message.contains("bad argument #1 to 'map.load' (no map named 'nowhere')"),
        "{message}"
    );
    let (message, _) = tick_error("map.copy('town', 0, 0, 0, 65, 1)");
    assert!(
        message.contains("bad argument #5 to 'map.copy' (width 0-64 expected, got 65)"),
        "{message}"
    );
}

/// Whether `source` runs to completion under `interrupts`.
fn runs_within(source: &str, interrupts: u64) -> bool {
    let mut runtime = start_with_budget(source, Budget { interrupts }).unwrap();
    runtime.tick(NO_INPUT) == Ok(())
}

#[test]
fn a_transfer_costs_one_unit_plus_two_per_cell() {
    // The call itself is an interrupt check. A full window is 4096 cells, so
    // it costs 1 + 8192 on top.
    let full = "function tick() map.load('big', 0) end";
    assert!(runs_within(full, 8_195));
    assert!(!runs_within(full, 8_190));
    let cells = "function tick() map.copy('big', 0, 0, 0, 3, 2) end";
    assert!(runs_within(cells, 20));
    assert!(!runs_within(
        "function tick() for i = 1, 20 do map.copy('big', 0, 0, 0, 3, 2) end end",
        100
    ));
}

#[test]
fn lookups_cost_one_unit() {
    for call in ["map.size('town')", "map.get('town', 0, 0, 0)"] {
        let source = |count: u32| format!("function tick() for i = 1, {count} do {call} end end");
        assert!(runs_within(&source(20), 100), "{call}");
        assert!(!runs_within(&source(40), 100), "{call}");
    }
}

#[test]
fn a_rejected_call_costs_nothing() {
    let source = "function tick()
        for i = 1, 5 do
            pcall(map.load, 'nowhere', 0)
            pcall(map.copy, 'big', 0, 0, 0, 65, 65)
            pcall(map.get, 'town', 9, 0, 0)
        end
    end";
    assert!(runs_within(source, 1_000));
}

#[test]
fn a_transfer_the_budget_cannot_cover_writes_nothing() {
    let mut runtime = start_with_budget(
        "function tick() map.load('big', 0) end",
        Budget { interrupts: 8_000 },
    )
    .unwrap();
    assert_eq!(runtime.tick(NO_INPUT), Err(limit(Phase::Tick)));
    assert!(blank(&runtime));
}

#[test]
fn a_store_without_the_map_is_an_error_not_a_crash() {
    let mut runtime = Runtime::start_on_scratch(
        "main.luau",
        b"function tick() map.load('town', 0) end",
        Files::default(),
        SMALL,
        font(),
    )
    .unwrap();
    assert!(runtime.tick(NO_INPUT).is_err());
    assert!(blank(&runtime));
}
