//! Lending the cartridge hardware to the `tick()` and `draw()` phases.

use glaux_machine::bg::BgVram;
use glaux_machine::hardware::CartridgeHardware;
use glaux_machine::palette::Palette;
use glaux_machine::sprite::ObjVram;
use pretty_assertions::assert_eq;

use glaux_script::{Phase, RuntimeError, ScriptError};

use super::*;

fn hardware() -> Option<Box<CartridgeHardware>> {
    Some(Box::new(CartridgeHardware::new(
        Palette::default(),
        ObjVram::default(),
        BgVram::default(),
    )))
}

fn address(home: &Option<Box<CartridgeHardware>>) -> *const CartridgeHardware {
    &**home.as_ref().expect("the hardware is back")
}

fn work_ram(home: &Option<Box<CartridgeHardware>>) -> u8 {
    home.as_ref().expect("the hardware is back").work_ram[0]
}

fn top_left(home: &Option<Box<CartridgeHardware>>) -> u8 {
    let hardware = home.as_ref().expect("the hardware is back");
    hardware.framebuffer.pixel(0, 0).unwrap()
}

fn internal_error(error: &RuntimeError, phase: Phase) -> bool {
    error.phase == phase
        && matches!(&error.error, ScriptError::Runtime(message) if message.starts_with("internal error"))
}

/// Installs `probe()`, which adds one to the first byte of Work RAM through
/// the runtime's slot and tells the script whether hardware was lent.
fn install_probe(runtime: &Runtime) {
    let slot = runtime.hardware_slot();
    let probe = runtime
        .lua()
        .create_function(move |_, ()| Ok(slot.with(|hardware| hardware.work_ram[0] += 1).is_some()))
        .unwrap();
    runtime.lua().globals().set("probe", probe).unwrap();
}

#[test]
fn each_phase_gets_the_hardware_and_returns_the_same_allocation() {
    let mut runtime =
        started("function tick() assert(probe()) end function draw() assert(probe()) end");
    install_probe(&runtime);
    let mut home = hardware();
    let before = address(&home);

    runtime.run_tick(NO_INPUT, &mut home).unwrap();
    assert_eq!((address(&home), work_ram(&home)), (before, 1));
    runtime.run_draw(&mut home).unwrap();
    assert_eq!((address(&home), work_ram(&home)), (before, 2));
    assert_eq!(runtime.hardware_slot().with(|_| ()), None);
}

#[test]
fn a_failing_tick_returns_the_hardware_and_skips_draw() {
    let mut runtime =
        started(r#"function tick() probe() error("boom") end function draw() probe() end"#);
    install_probe(&runtime);
    let mut home = hardware();
    let before = address(&home);

    let error = runtime.run_tick(NO_INPUT, &mut home).unwrap_err();
    assert_eq!(error.phase, Phase::Tick);
    assert_eq!((address(&home), work_ram(&home)), (before, 1));
    assert_eq!(runtime.hardware_slot().with(|_| ()), None);
}

#[test]
fn a_failing_draw_returns_the_hardware() {
    let mut runtime = started(r#"function draw() probe() error("boom") end"#);
    install_probe(&runtime);
    let mut home = hardware();
    let before = address(&home);

    runtime.run_tick(NO_INPUT, &mut home).unwrap();
    let error = runtime.run_draw(&mut home).unwrap_err();
    assert_eq!(error.phase, Phase::Draw);
    assert_eq!((address(&home), work_ram(&home)), (before, 1));
    assert_eq!(runtime.hardware_slot().with(|_| ()), None);
}

#[test]
fn a_phase_without_hardware_fails_before_running_the_script() {
    let mut runtime =
        started("function tick() log.info('tick') end function draw() log.info('draw') end");
    let mut home = None;

    let error = runtime.run_tick(NO_INPUT, &mut home).unwrap_err();
    assert!(internal_error(&error, Phase::Tick), "{error}");
    let error = runtime.run_draw(&mut home).unwrap_err();
    assert!(internal_error(&error, Phase::Draw), "{error}");
    assert!(home.is_none());
    assert_eq!(texts(runtime.take_output()), Vec::<String>::new());
}

#[test]
fn a_tick_completes_in_draw_and_both_callbacks_see_the_same_time() {
    let mut runtime = started(
        "function tick() log.info(frame()) end function draw() cls(3) log.info(frame()) end",
    );
    let mut home = hardware();

    runtime.run_tick(NO_INPUT, &mut home).unwrap();
    assert_eq!(texts(runtime.take_output()), ["0"]);
    assert_eq!(top_left(&home), 0, "tick draws nothing");

    runtime.run_draw(&mut home).unwrap();
    assert_eq!(texts(runtime.take_output()), ["0"]);
    assert_eq!(top_left(&home), 3, "draw drew on the lent hardware");

    runtime.run_tick(NO_INPUT, &mut home).unwrap();
    assert_eq!(texts(runtime.take_output()), ["1"]);
}

#[test]
fn a_failing_draw_leaves_the_hardware_as_it_was_and_does_not_advance_time() {
    let mut runtime = started(
        "fail = false function tick() log.info(frame()) end \
         function draw() if fail then error('boom') end cls(1) end",
    );
    let mut home = hardware();
    runtime.run_tick(NO_INPUT, &mut home).unwrap();
    runtime.run_draw(&mut home).unwrap();
    runtime.take_output();

    runtime.lua().globals().set("fail", true).unwrap();
    runtime.run_tick(NO_INPUT, &mut home).unwrap();
    runtime.run_draw(&mut home).unwrap_err();
    assert_eq!(top_left(&home), 1);
    assert_eq!(texts(runtime.take_output()), ["1"]);

    runtime.lua().globals().set("fail", false).unwrap();
    runtime.run_tick(NO_INPUT, &mut home).unwrap();
    assert_eq!(texts(runtime.take_output()), ["1"]);
}

/// Installs `poke(address, value)` and `peek(address)`, which go through
/// the bus of the lent hardware as an API for memory will. Writing to the
/// hardware needs no other access than the runtime's slot.
fn install_bus(runtime: &Runtime) {
    let slot = runtime.hardware_slot();
    let poke = {
        let slot = slot.clone();
        runtime
            .lua()
            .create_function(move |_, (address, value): (u32, u8)| {
                Ok(slot
                    .with(|hardware| hardware.write(address, value).is_ok())
                    .unwrap_or(false))
            })
            .unwrap()
    };
    let peek = runtime
        .lua()
        .create_function(move |_, address: u32| {
            Ok(slot.with(|hardware| hardware.read(address).unwrap_or(0)))
        })
        .unwrap();
    runtime.lua().globals().set("poke", poke).unwrap();
    runtime.lua().globals().set("peek", peek).unwrap();
}

/// The bus address of the framebuffer pixel `(x, y)`.
const fn fb(x: u32, y: u32) -> u32 {
    glaux_machine::memory_map::FRAMEBUFFER.start + y * 320 + x
}

#[test]
fn drawing_is_immediate_and_the_bus_sees_the_same_framebuffer() {
    let mut runtime = started(&format!(
        "function draw()
            pset(3, 4, 9)
            log.info(peek({}))
            poke({}, 7)
            pset(7, 4, 1)
            rectfill(0, 0, 3, 3, 2)
            log.info(peek({}))
        end",
        fb(3, 4),
        fb(5, 4),
        fb(1, 1)
    ));
    install_bus(&runtime);
    let mut home = hardware();
    runtime.run_tick(NO_INPUT, &mut home).unwrap();
    runtime.run_draw(&mut home).unwrap();

    assert_eq!(texts(runtime.take_output()), ["9", "2"]);
    let framebuffer = &home.as_ref().unwrap().framebuffer;
    assert_eq!(framebuffer.pixel(3, 4), Some(9));
    // A write through the bus is not erased by what is drawn after it.
    assert_eq!(framebuffer.pixel(5, 4), Some(7));
    assert_eq!(framebuffer.pixel(7, 4), Some(1));
}

#[test]
fn primitives_and_bus_writes_land_in_the_order_they_are_made() {
    let mut runtime = started(&format!(
        "function draw()
            poke({a}, 7)
            rectfill(0, 0, 3, 2, 2)
            rectfill(0, 0, 3, 2, 3)
            poke({b}, 8)
        end",
        a = fb(1, 0),
        b = fb(2, 0)
    ));
    install_bus(&runtime);
    let mut home = hardware();
    runtime.run_tick(NO_INPUT, &mut home).unwrap();
    runtime.run_draw(&mut home).unwrap();

    let framebuffer = &home.as_ref().unwrap().framebuffer;
    // Overdrawn by the rectangles that followed the write.
    assert_eq!(framebuffer.pixel(1, 0), Some(3));
    // The write that followed the rectangles stays.
    assert_eq!(framebuffer.pixel(2, 0), Some(8));
    assert_eq!(framebuffer.pixel(0, 1), Some(3));
}

#[test]
fn a_failing_draw_keeps_every_write_it_made_to_the_hardware() {
    let mut runtime = started(&format!(
        "function draw()
            cls(2)
            poke({ram}, 42)
            poke({palette}, 11)
            error('boom')
        end",
        ram = glaux_machine::memory_map::WORK_RAM.start + 5,
        palette = glaux_machine::memory_map::MASTER_PALETTE.start
    ));
    install_bus(&runtime);
    let mut home = hardware();
    let before = home.as_ref().unwrap().palette.get(0);
    runtime.run_tick(NO_INPUT, &mut home).unwrap();
    runtime.run_draw(&mut home).unwrap_err();

    let hardware = home.as_ref().expect("the hardware is back");
    assert_eq!(hardware.work_ram[5], 42);
    assert_eq!(hardware.palette.get(0).r, 11);
    assert_ne!(hardware.palette.get(0), before);
    assert_eq!(hardware.framebuffer.pixel(0, 0), Some(2));
}
