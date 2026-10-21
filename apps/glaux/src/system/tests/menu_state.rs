//! What the menu bar offers, for each surface that can receive its
//! commands. Where a command is offered, the event it sends does
//! something there, and where it is not, the event leaves the surface
//! alone.

use super::sprite_editor::{edited, open_sprites, with_video_assets};
use super::*;
use crate::system::clipboard::MemoryClipboard;
use pretty_assertions::assert_eq;

fn none() -> MenuState {
    MenuState::default()
}

fn only_documentation() -> MenuState {
    MenuState {
        documentation: true,
        ..none()
    }
}

/// The workspace on its Code view with `text` in `code/main.luau`, the
/// caret at its start.
fn in_code(fixture: &Fixture, text: &str) -> System {
    press_break(enter(shell_with_cartridge(fixture, text), "load pong"))
}

fn typed(system: System, text: &str) -> System {
    let events: Vec<InputEvent> = text.chars().map(InputEvent::Text).collect();
    system.update(&events)
}

fn docs_open(system: &System) -> bool {
    workspace::tests_support::docs_open(&system.workspace)
}

fn docs_searching(system: &System) -> bool {
    workspace::tests_support::docs_searching(&system.workspace)
}

fn docs_pages(system: &System) -> (usize, Option<String>) {
    workspace::tests_support::docs_pages(&system.workspace)
}

fn code_text(system: &System) -> String {
    let code = system.code.as_ref().unwrap();
    code.active_document().unwrap().text().to_owned()
}

fn code_is_modified(system: &System) -> bool {
    system.code.as_ref().unwrap().is_modified()
}

fn centre(rect: workspace::Rect) -> Point {
    Point::new(
        (rect.x + rect.width / 2) as u16,
        (rect.y + rect.height / 2) as u16,
    )
}

fn docs_button() -> Point {
    centre(workspace::tests_support::docs_button(fake_rom().font()))
}

/// What the Code view offers with a document open and nothing edited.
fn code_idle() -> MenuState {
    MenuState {
        new_file: true,
        open_file: true,
        close_document: true,
        paste: true,
        select_all: true,
        find: true,
        replace: true,
        go_to_line: true,
        documentation: true,
        ..none()
    }
}

#[test]
fn the_shell_offers_only_the_documentation() {
    assert_eq!(shell().menu_state(), only_documentation());
}

#[test]
fn boot_credits_and_cartridges_offer_nothing() {
    assert_eq!(boot_at(BootStage::Intro).menu_state(), none());
    assert_eq!(enter(shell(), "credits").menu_state(), none());

    let fixture = Fixture::new("sys-menu-cartridge");
    let running = started(&fixture, "-- main\n");
    assert!(is_running(&running));
    assert_eq!(running.menu_state(), none());
    let paused = press_break(running);
    assert!(is_paused_game(&paused));
    assert_eq!(paused.menu_state(), none());
    let paused_shell = open_shell(paused);
    assert!(is_paused_shell(&paused_shell));
    assert_eq!(paused_shell.menu_state(), none());
}

#[test]
fn the_browser_shown_apart_offers_what_it_acts_on() {
    let system = enter(shell(), "docs");
    assert_eq!(
        system.menu_state(),
        MenuState {
            find: true,
            close_document: true,
            toggle_sidebar: true,
            ..none()
        }
    );
}

#[test]
fn the_code_view_offers_each_command_when_it_has_something_to_act_on() {
    let fixture = Fixture::new("sys-menu-code");
    let system = in_code(&fixture, "-- main\n");
    assert_eq!(system.menu_state(), code_idle());

    // Typing modifies the document and gives undo something.
    let system = typed(system, "a");
    assert_eq!(
        system.menu_state(),
        MenuState {
            save: true,
            undo: true,
            ..code_idle()
        }
    );

    let system = system.update(&[InputEvent::Undo]);
    assert_eq!(
        system.menu_state(),
        MenuState {
            redo: true,
            ..code_idle()
        }
    );

    // A selection can be cut and copied.
    let system = system.update(&[InputEvent::SelectAll]);
    assert_eq!(
        system.menu_state(),
        MenuState {
            redo: true,
            cut: true,
            copy: true,
            ..code_idle()
        }
    );
}

#[test]
fn a_command_the_code_view_offers_does_what_the_shortcut_does() {
    let fixture = Fixture::new("sys-menu-code-effects");
    let system = typed(in_code(&fixture, "-- main\n"), "ab");
    let state = system.menu_state();
    assert!(state.save && state.undo && !state.copy);

    // Copy needs a selection, and with one it reaches the clipboard.
    let mut clipboard = MemoryClipboard::default();
    let system = system.update_with(&[InputEvent::SelectAll], &mut clipboard);
    assert!(system.menu_state().copy);
    let system = system.update_with(&[InputEvent::Copy], &mut clipboard);
    assert_eq!(clipboard.text.as_deref(), Some("ab-- main\n"));

    // Save writes the document, and nothing is left to save.
    let system = system.update(&[InputEvent::Save]);
    assert!(!code_is_modified(&system));
    assert!(!system.menu_state().save);
}

#[test]
fn the_find_bar_takes_the_editing_commands_but_not_the_document_ones() {
    let fixture = Fixture::new("sys-menu-find");
    let system = typed(in_code(&fixture, "-- main\n"), "a");
    let system = system.update(&[InputEvent::Find]);
    // The empty query takes a paste. Undo and Redo would reach no
    // document, so they are not offered, and the commands that act on the
    // document still are.
    assert_eq!(
        system.menu_state(),
        MenuState {
            save: true,
            paste: true,
            select_all: false,
            ..code_idle()
        }
    );

    let before = code_text(&system);
    let system = system.update(&[InputEvent::Undo]);
    assert_eq!(code_text(&system), before);

    let system = typed(system, "ma");
    let state = system.menu_state();
    assert!(state.select_all && !state.copy);
    let system = system.update(&[InputEvent::SelectAll]);
    let state = system.menu_state();
    assert!(state.copy && state.cut);
    let mut clipboard = MemoryClipboard::default();
    system.update_with(&[InputEvent::Copy], &mut clipboard);
    assert_eq!(clipboard.text.as_deref(), Some("ma"));
}

#[test]
fn a_prompt_takes_the_editing_commands_of_its_field_and_nothing_else() {
    let fixture = Fixture::new("sys-menu-go-to");
    let system = typed(in_code(&fixture, "-- main\n"), "a");
    let system = system.update(&[InputEvent::GoToLine]);
    // The prompt opens with the current line number selected.
    assert_eq!(
        system.menu_state(),
        MenuState {
            select_all: true,
            cut: true,
            copy: true,
            paste: true,
            ..none()
        }
    );

    // Emptied, only a paste can do anything.
    let system = system.update(&[InputEvent::Backspace]);
    assert_eq!(
        system.menu_state(),
        MenuState {
            paste: true,
            ..none()
        }
    );

    let system = typed(system, "1");
    assert_eq!(
        system.menu_state(),
        MenuState {
            select_all: true,
            paste: true,
            ..none()
        }
    );
    let system = system.update(&[InputEvent::SelectAll]);
    assert_eq!(
        system.menu_state(),
        MenuState {
            select_all: true,
            cut: true,
            copy: true,
            paste: true,
            ..none()
        }
    );

    // The document behind it is out of reach: Save does nothing.
    let system = system.update(&[InputEvent::Save]);
    assert!(code_is_modified(&system));
}

#[test]
fn the_file_picker_overlay_takes_nothing() {
    let fixture = Fixture::new("sys-menu-picker");
    let system = in_code(&fixture, "-- main\n").update(&[InputEvent::OpenFile]);
    assert_eq!(system.menu_state(), none());
}

#[test]
fn the_browser_over_the_code_view_leaves_the_documents_behind_it_alone() {
    let fixture = Fixture::new("sys-menu-docs-over-code");
    let system = typed(in_code(&fixture, "-- main\n"), "a");
    assert!(system.menu_state().save);

    let system = system.update(&click(docs_button()));
    assert!(docs_open(&system));
    assert_eq!(
        system.menu_state(),
        MenuState {
            find: true,
            close_document: true,
            toggle_sidebar: true,
            ..none()
        }
    );

    // What it does not offer does nothing, however modified the document.
    let before = code_text(&system);
    let system = system.update(&[InputEvent::Undo, InputEvent::Save, InputEvent::NewFile]);
    assert_eq!(code_text(&system), before);
    assert!(code_is_modified(&system));
    assert!(docs_open(&system));
}

#[test]
fn the_browsers_search_takes_the_editing_commands() {
    let fixture = Fixture::new("sys-menu-docs-search");
    let system = in_code(&fixture, "-- main\n").update(&click(docs_button()));
    let system = system.update(&[InputEvent::Find]);
    assert!(docs_searching(&system));
    assert_eq!(
        system.menu_state(),
        MenuState {
            paste: true,
            ..none()
        }
    );
    let system = typed(system, "spr").update(&[InputEvent::SelectAll]);
    let state = system.menu_state();
    assert!(state.select_all && state.copy && state.cut && state.paste);
    let mut clipboard = MemoryClipboard::default();
    system.update_with(&[InputEvent::Copy], &mut clipboard);
    assert_eq!(clipboard.text.as_deref(), Some("spr"));
}

#[test]
fn the_sprite_editor_offers_save_and_history_and_the_other_views_nothing() {
    let fixture = Fixture::new("sys-menu-sprites");
    let system = open_sprites(with_video_assets(&fixture));
    assert_eq!(workspace_tab(&system), Tab::Sprites);
    assert_eq!(system.menu_state(), only_documentation());

    let fixture = Fixture::new("sys-menu-sprites-edited");
    let system = edited(&fixture);
    assert_eq!(
        system.menu_state(),
        MenuState {
            save: true,
            undo: true,
            documentation: true,
            ..none()
        }
    );
    let system = system.update(&[InputEvent::Undo]);
    assert_eq!(
        system.menu_state(),
        MenuState {
            redo: true,
            documentation: true,
            ..none()
        }
    );

    let map = system.update(&click(tab_centre(Tab::Map)));
    assert_eq!(workspace_tab(&map), Tab::Map);
    assert_eq!(map.menu_state(), only_documentation());
}

#[test]
fn the_wait_for_the_host_file_dialog_takes_nothing() {
    let fixture = Fixture::new("sys-menu-sprite-import");
    let system = edited(&fixture);
    let at = centre(workspace::tests_support::sprite_import_button(
        &system.workspace,
    ));
    let system = system.update(&click(at));
    assert!(system.workspace.awaits_file());
    assert_eq!(system.menu_state(), none());
}

#[test]
fn closing_a_modified_document_asks_and_nothing_else_is_offered() {
    let fixture = Fixture::new("sys-menu-close-dialog");
    let system = typed(in_code(&fixture, "-- main\n"), "a");
    let system = system.update(&[InputEvent::CloseDocument]);
    assert!(workspace::tests_support::modal(&system.workspace).is_some());
    assert_eq!(system.menu_state(), none());
}

#[test]
fn the_question_about_unsaved_edits_and_quitting_take_nothing() {
    let fixture = Fixture::new("sys-menu-unsaved");
    let mut system = typed(in_code(&fixture, "-- main\n"), "a");
    system.request_exit();
    assert_eq!(system.menu_state(), none());
    let system = system.update(&[]);
    assert!(system.unsaved_flow().is_some());
    assert_eq!(system.menu_state(), none());

    let mut quitting = shell();
    quitting.request_exit();
    assert!(quitting.exit_confirmed());
    assert_eq!(quitting.menu_state(), none());
}

#[test]
fn documentation_from_the_shell_opens_the_front_page() {
    let system = shell().open_documentation();
    assert!(matches!(system.state(), SystemState::Docs { .. }));
    assert_eq!(docs_pages(&system), (1, Some("guide/index".to_owned())));
    // Open already, there is nothing more to open.
    assert!(!system.menu_state().documentation);
}

#[test]
fn documentation_from_the_workspace_ignores_the_symbol_under_the_caret() {
    let fixture = Fixture::new("sys-menu-docs-f1");
    let contextual = typed(in_code(&fixture, ""), "spr(1, ").update(&[InputEvent::Docs]);
    // F1 looks the call up.
    assert_eq!(
        workspace::tests_support::docs_location(&contextual.workspace),
        Some(("api/graphics/spr".to_owned(), Some("parameters".to_owned())))
    );

    let fixture = Fixture::new("sys-menu-docs-menu");
    let system = typed(in_code(&fixture, ""), "spr(1, ").open_documentation();
    assert!(docs_open(&system));
    assert!(!docs_searching(&system));
    assert_eq!(docs_pages(&system), (1, Some("guide/index".to_owned())));
    // The editor is left as it was.
    assert_eq!(code_text(&system), "spr(1, ");
    assert!(!system.menu_state().documentation);
}

#[test]
fn documentation_opens_only_where_it_is_offered() {
    let fixture = Fixture::new("sys-menu-docs-refused");
    let paused = open_shell(press_break(started(&fixture, "-- main\n")));
    assert!(is_paused_shell(&paused));
    assert!(is_paused_shell(&paused.open_documentation()));

    let fixture = Fixture::new("sys-menu-docs-refused-prompt");
    let system = in_code(&fixture, "-- main\n").update(&[InputEvent::GoToLine]);
    let system = system.open_documentation();
    assert!(!docs_open(&system));
}
