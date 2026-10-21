use super::code_tabs::{loaded_with, open_file};
use super::*;
use pretty_assertions::assert_eq;

const MAIN: &str = "local util = require(\"util\")\nutil.clamp(1)\nutil.hidden(2)\n";
const UTIL: &str =
    "local M = {}\nfunction M.clamp(x) return x end\nlocal function hidden() end\nreturn M\n";

fn started(fixture: &Fixture) -> System {
    press_break(loaded_with(
        fixture,
        &[("code/main.luau", MAIN), ("code/util.luau", UTIL)],
    ))
}

/// The active script and the byte its caret is on.
fn caret(system: &System) -> (String, usize) {
    let code = system.code.as_ref().unwrap();
    let document = code.active_document().unwrap();
    let at = code.tabs().selection(document.id()).unwrap().caret().byte();
    (document.path().to_string(), at)
}

fn notice(system: &System) -> Option<&str> {
    workspace::tests_support::notice(&system.workspace)
}

fn right(times: usize) -> Vec<InputEvent> {
    vec![InputEvent::Right { select: false }; times]
}

/// Puts the caret `column` characters into the second line of `main`.
fn on_second_line(system: System, column: usize) -> System {
    let mut input = vec![InputEvent::Down { select: false }];
    input.extend(right(column));
    system.update(&input)
}

#[test]
fn f11_follows_a_required_module_and_back_and_forward_retrace_it() {
    let fixture = Fixture::new("sys-nav-require");
    let system = on_second_line(started(&fixture), 7);
    let (_, start) = caret(&system);

    let system = system.update(&[InputEvent::GoToDefinition]);
    let clamp = UTIL.find("clamp").unwrap();
    assert_eq!(caret(&system), ("code/util.luau".to_owned(), clamp));

    let system = system.update(&[InputEvent::Back]);
    assert_eq!(caret(&system), ("code/main.luau".to_owned(), start));

    let system = system.update(&[InputEvent::Forward]);
    assert_eq!(caret(&system), ("code/util.luau".to_owned(), clamp));
}

#[test]
fn a_member_the_module_does_not_export_stays_put_and_says_so() {
    let fixture = Fixture::new("sys-nav-hidden");
    let system = started(&fixture).update(&[
        InputEvent::Down { select: false },
        InputEvent::Down { select: false },
    ]);
    let system = system.update(&right(8));
    let before = caret(&system);
    let system = system.update(&[InputEvent::GoToDefinition]);
    assert_eq!(caret(&system), before);
    assert_eq!(notice(&system), Some("No definition found"));
    // Nothing was recorded, so Back has nowhere to go.
    let system = system.update(&[InputEvent::Back]);
    assert_eq!(caret(&system), before);
}

#[test]
fn navigation_does_not_touch_the_edit_history() {
    let fixture = Fixture::new("sys-nav-undo");
    let system = on_second_line(started(&fixture), 7);
    let system = system.update(&[InputEvent::Text('x'), InputEvent::GoToDefinition]);
    let system = system.update(&[InputEvent::Back, InputEvent::Undo]);
    let code = system.code.as_ref().unwrap();
    assert_eq!(code.active_document().unwrap().text(), MAIN);
}

#[test]
fn the_symbol_panel_lists_declarations_and_enter_goes_to_the_picked_one() {
    let fixture = Fixture::new("sys-nav-symbols");
    let system = open_file(started(&fixture), "util");
    assert_eq!(caret(&system).0, "code/util.luau");

    let system = system.update(&[InputEvent::GoToSymbol]);
    // M, clamp, hidden are listed in source order. The caret is at the top, so the
    // first is highlighted. Down twice reaches `hidden`.
    let system = system.update(&[
        InputEvent::Down { select: false },
        InputEvent::Down { select: false },
        InputEvent::Enter,
    ]);
    let hidden = UTIL.find("hidden").unwrap();
    assert_eq!(caret(&system), ("code/util.luau".to_owned(), hidden));

    let system = system.update(&[InputEvent::Back]);
    assert_eq!(caret(&system), ("code/util.luau".to_owned(), 0));
}

#[test]
fn typing_is_not_applied_to_the_text_while_the_symbol_list_has_the_keys() {
    let fixture = Fixture::new("sys-nav-symbols-keys");
    let system = started(&fixture).update(&[InputEvent::GoToSymbol, InputEvent::Text('z')]);
    let code = system.code.as_ref().unwrap();
    assert_eq!(code.active_document().unwrap().text(), MAIN);
    // Escape hands the keys back to the text before it would leave the editor.
    let system = system.update(&[InputEvent::Break, InputEvent::Text('z')]);
    let code = system.code.as_ref().unwrap();
    assert_eq!(code.active_document().unwrap().text(), format!("z{MAIN}"));
    assert_eq!(workspace_tab(&system), Tab::Code);
}
