use super::*;
use pretty_assertions::assert_eq;

fn is_docs(system: &System) -> bool {
    matches!(system.state(), SystemState::Docs { .. })
}

fn pages(system: &System) -> (usize, Option<String>) {
    workspace::tests_support::docs_pages(&system.workspace)
}

#[test]
fn docs_opens_the_front_page_and_escape_returns_to_the_shell() {
    let system = enter(shell(), "docs");
    assert!(is_docs(&system));
    assert_eq!(system.input_mode(), InputMode::Workspace);
    assert_eq!(pages(&system), (1, Some("guide/index".to_owned())));
    assert!(!workspace::tests_support::docs_open(&system.workspace));

    let system = press_break(system);
    assert!(!is_docs(&system));
    assert_eq!(transcript(&system).last().unwrap(), "> docs");
}

#[test]
fn docs_opens_the_page_a_topic_names_and_keeps_its_tabs_between_openings() {
    let system = enter(shell(), "docs spr");
    assert_eq!(pages(&system), (1, Some("api/graphics/spr".to_owned())));
    let system = press_break(system);

    let system = enter(system, "docs guide/getting-started");
    assert_eq!(
        pages(&system),
        (2, Some("guide/getting-started".to_owned()))
    );
    let system = press_break(system);

    let system = enter(system, "docs");
    assert_eq!(
        pages(&system),
        (2, Some("guide/getting-started".to_owned()))
    );
}

#[test]
fn a_topic_that_could_mean_several_pages_opens_the_search() {
    let system = enter(shell(), "docs index");
    assert!(is_docs(&system));
    assert!(workspace::tests_support::docs_searching(&system.workspace));

    // Escape closes the search first, then the browser.
    let system = press_break(system);
    assert!(is_docs(&system));
    assert!(!workspace::tests_support::docs_searching(&system.workspace));
    assert!(!is_docs(&press_break(system)));
}

#[test]
fn an_unknown_topic_stays_in_the_shell() {
    let system = enter(shell(), "docs zzzzqq");
    assert!(!is_docs(&system));
    let lines = transcript(&system);
    assert_eq!(
        lines[lines.len() - 2..],
        ["! No documentation for zzzzqq", "Try: docs"]
    );
}

#[test]
fn keys_that_belong_to_the_workspace_do_nothing_here() {
    let system = enter(shell(), "docs spr");
    let before = pages(&system);
    let system = system.update(&[
        InputEvent::Save,
        InputEvent::OpenFile,
        InputEvent::Function(2),
        InputEvent::Function(3),
    ]);
    assert!(is_docs(&system));
    assert_eq!(pages(&system), before);
}

#[test]
fn the_browser_fills_the_screen_from_the_first_row() {
    let system = enter(shell(), "docs");
    assert_eq!(system.frame_palette(&fake_rom()), fake_rom().palette());
    let frame = render(&system);
    assert!(row_has_ink(&frame, 0));
}

#[test]
fn docs_work_without_a_cartridge_and_leave_a_loaded_one_alone() {
    let fixture = Fixture::new("sys-docs-shell");
    let system = shell_with_cartridge(&fixture, COUNTER);
    let name = loaded_name(&system).map(str::to_owned);
    let system = press_break(enter(system, "docs"));
    assert_eq!(loaded_name(&system).map(str::to_owned), name);
}

#[test]
fn a_paused_cartridge_keeps_the_shell_to_itself() {
    let fixture = Fixture::new("sys-docs-paused");
    let system = open_shell(press_break(started(&fixture, COUNTER)));
    let system = enter(system, "docs");
    assert!(is_paused_shell(&system));
    assert_eq!(
        transcript(&system).last().unwrap(),
        "! Cartridge paused: resume or stop it first"
    );
}
