use super::*;
use pretty_assertions::assert_eq;

fn is_credits(system: &System) -> bool {
    matches!(system.state(), SystemState::Credits { .. })
}

#[test]
fn the_credits_command_opens_the_view_and_escape_returns_to_the_shell() {
    let system = enter(shell(), "credits");
    assert!(is_credits(&system));
    assert_eq!(system.input_mode(), InputMode::Workspace);

    let system = press_break(system);
    assert_eq!(transcript(&system).last().unwrap(), "> credits");
}

#[test]
fn the_view_keeps_its_page_across_frames_and_leaves_the_loaded_cartridge_alone() {
    let fixture = Fixture::new("sys-credits");
    let system = shell_with_cartridge(&fixture, COUNTER);
    let name = loaded_name(&system).map(str::to_owned);
    let system = enter(system, "credits");
    let system = system.update(&[InputEvent::Function(3)]);
    let system = system.update(&[]);
    assert!(is_credits(&system));
    let system = press_break(system);
    assert_eq!(loaded_name(&system).map(str::to_owned), name);
}

#[test]
fn the_view_draws_with_the_system_palette() {
    let system = enter(shell(), "credits");
    assert_eq!(system.frame_palette(&fake_rom()), fake_rom().palette());
    let frame = render(&system);
    assert!((0..HEIGHT).any(|y| row_has_ink(&frame, y as i32)));
}

#[test]
fn a_paused_cartridge_keeps_the_shell_to_itself() {
    let fixture = Fixture::new("sys-credits-paused");
    let system = open_shell(press_break(started(&fixture, COUNTER)));
    let system = enter(system, "credits");
    assert!(is_paused_shell(&system));
    assert_eq!(
        transcript(&system).last().unwrap(),
        "! Cartridge paused: resume or stop it first"
    );
}
