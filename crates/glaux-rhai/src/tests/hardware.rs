//! Lending the cartridge hardware to the `tick()` and `draw()` phases.

use glaux_machine::bg::BgVram;
use glaux_machine::hardware::CartridgeHardware;
use glaux_machine::palette::Palette;
use glaux_machine::sprite::ObjVram;
use pretty_assertions::assert_eq;

use glaux_script::DrawCommand;

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

fn internal_error(error: &RuntimeError, phase: Phase) -> bool {
    error.phase == phase
        && matches!(&error.error, ScriptError::Runtime(message) if message.starts_with("internal error"))
}

/// Registers `probe()`, which adds one to the first byte of Work RAM through
/// the runtime's slot and tells the script whether hardware was lent.
fn install_probe(runtime: &mut Runtime) {
    let slot = runtime.hardware_slot();
    runtime.engine_mut().register_fn("probe", move || {
        slot.with(|hardware| hardware.work_ram[0] += 1).is_some()
    });
}

#[test]
fn each_phase_gets_the_hardware_and_returns_the_same_allocation() {
    let mut runtime = started(
        r#"
        fn tick() { if !probe() { throw "no hardware"; } }
        fn draw() { if !probe() { throw "no hardware"; } }
        "#,
    );
    install_probe(&mut runtime);
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
    let mut runtime = started(
        r#"
        fn tick() { probe(); throw "boom"; }
        fn draw() { probe(); }
        "#,
    );
    install_probe(&mut runtime);
    let mut home = hardware();
    let before = address(&home);

    let error = runtime.run_tick(NO_INPUT, &mut home).unwrap_err();
    assert_eq!(error.phase, Phase::Tick);
    assert_eq!((address(&home), work_ram(&home)), (before, 1));
    assert_eq!(runtime.hardware_slot().with(|_| ()), None);
}

#[test]
fn a_failing_draw_returns_the_hardware() {
    let mut runtime = started(r#"fn draw() { probe(); throw "boom"; }"#);
    install_probe(&mut runtime);
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
    let mut runtime = started(r#"fn tick() { print("tick"); } fn draw() { print("draw"); }"#);
    let mut home = None;

    let error = runtime.run_tick(NO_INPUT, &mut home).unwrap_err();
    assert!(internal_error(&error, Phase::Tick), "{error}");
    let error = runtime.run_draw(&mut home).unwrap_err();
    assert!(internal_error(&error, Phase::Draw), "{error}");
    assert!(home.is_none());
    assert_eq!(texts(runtime.take_output()), Vec::<String>::new());
    assert_eq!(runtime.frame(), []);
}

#[test]
fn a_tick_completes_in_draw_and_both_callbacks_see_the_same_time() {
    let mut runtime =
        started("fn tick() { print(`${frame()}`); } fn draw() { cls(3); print(`${frame()}`); }");
    let mut home = hardware();

    runtime.run_tick(NO_INPUT, &mut home).unwrap();
    assert_eq!(texts(runtime.take_output()), ["0"]);
    assert_eq!(runtime.frame(), []);

    runtime.run_draw(&mut home).unwrap();
    assert_eq!(texts(runtime.take_output()), ["0"]);
    assert_eq!(runtime.frame(), [DrawCommand::Clear(3)]);

    runtime.run_tick(NO_INPUT, &mut home).unwrap();
    assert_eq!(texts(runtime.take_output()), ["1"]);
}

#[test]
fn a_failing_draw_keeps_the_frame_and_does_not_advance_time() {
    let mut runtime = started(
        r#"
        fn boot() { this.draws = 0; }
        fn tick() { print(`${frame()}`); }
        fn draw() {
            this.draws += 1;
            if this.draws == 2 { throw "boom"; }
            cls(1);
        }
        "#,
    );
    let mut home = hardware();
    runtime.run_tick(NO_INPUT, &mut home).unwrap();
    runtime.run_draw(&mut home).unwrap();
    runtime.take_output();

    runtime.run_tick(NO_INPUT, &mut home).unwrap();
    runtime.run_draw(&mut home).unwrap_err();
    assert_eq!(runtime.frame(), [DrawCommand::Clear(1)]);
    assert_eq!(texts(runtime.take_output()), ["1"]);

    runtime.run_tick(NO_INPUT, &mut home).unwrap();
    assert_eq!(texts(runtime.take_output()), ["1"]);
}
