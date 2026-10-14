//! The clipboard and NEW FILE through the whole system.

use super::*;
use crate::system::clipboard::MemoryClipboard;
use pretty_assertions::assert_eq;

fn active_text(system: &System) -> Option<String> {
    let code = system.code.as_ref()?;
    Some(code.active_document()?.text().to_owned())
}

fn active_path(system: &System) -> Option<String> {
    let code = system.code.as_ref()?;
    Some(code.active_document()?.path().to_string())
}

fn text(text: &str) -> Vec<InputEvent> {
    text.chars().map(InputEvent::Text).collect()
}

/// The `Pong` cartridge, its entry holding `script`, in the Code view.
fn in_code(fixture: &Fixture, script: &str) -> System {
    let system = press_break(enter(shell_with_cartridge(fixture, script), "load pong"));
    assert_eq!(workspace_tab(&system), Tab::Code);
    system
}

#[test]
fn copy_and_cut_reach_the_hosts_clipboard_and_cut_undoes() {
    let fixture = Fixture::new("sys-clip-copy");
    let mut clipboard = MemoryClipboard::default();
    let system = in_code(&fixture, "speed = 1");
    let system = system.update_with(
        &[InputEvent::WordRight { select: true }, InputEvent::Copy],
        &mut clipboard,
    );
    assert_eq!(clipboard.text.as_deref(), Some("speed"));
    let system = system.update_with(
        &[
            InputEvent::Right { select: false },
            InputEvent::End { select: true },
            InputEvent::Cut,
        ],
        &mut clipboard,
    );
    assert_eq!(clipboard.text.as_deref(), Some(" = 1"));
    assert_eq!(active_text(&system).as_deref(), Some("speed"));
    let system = system.update_with(&[InputEvent::Undo], &mut clipboard);
    assert_eq!(active_text(&system).as_deref(), Some("speed = 1"));
}

#[test]
fn a_paste_lands_before_the_keys_typed_after_it() {
    let fixture = Fixture::new("sys-clip-order");
    let mut clipboard = MemoryClipboard {
        text: Some("local ".into()),
    };
    let system = in_code(&fixture, "");
    let mut input = vec![InputEvent::Paste];
    input.extend(text("x"));
    let system = system.update_with(&input, &mut clipboard);
    assert_eq!(active_text(&system).as_deref(), Some("local x"));
    assert!(system.code.as_ref().unwrap().is_modified());
}

#[test]
fn the_find_field_copies_and_pastes_without_touching_the_script() {
    let fixture = Fixture::new("sys-clip-find");
    let mut clipboard = MemoryClipboard {
        text: Some("two".into()),
    };
    let system = in_code(&fixture, "one two");
    let system = system.update_with(&[InputEvent::Find, InputEvent::Paste], &mut clipboard);
    let mut input = text("s");
    input.extend([InputEvent::SelectAll, InputEvent::Cut]);
    let system = system.update_with(&input, &mut clipboard);
    assert_eq!(clipboard.text.as_deref(), Some("twos"));
    assert_eq!(active_text(&system).as_deref(), Some("one two"));
}

#[test]
fn new_file_creates_the_script_in_the_manifests_language_and_opens_it() {
    for (args, extension) in [("game", "luau"), ("owl --lang rhai", "rhai")] {
        let fixture = Fixture::new(&format!("sys-new-file-{extension}"));
        let mut clipboard = MemoryClipboard::default();
        let dir = args.split(' ').next().unwrap();
        let system = press_break(enter(
            shell_with_cartridge(&fixture, ""),
            &format!("new {args}"),
        ));
        let mut input = vec![InputEvent::NewFile];
        input.extend(text("enemies/bat"));
        input.push(InputEvent::Enter);
        let system = system.update_with(&input, &mut clipboard);

        let path = format!("code/enemies/bat.{extension}");
        let host = fixture.root().join(dir).join(&path);
        assert_eq!(std::fs::read(&host).unwrap(), b"", "{path}");
        assert_eq!(active_path(&system).as_deref(), Some(path.as_str()));
        assert!(!system.code.as_ref().unwrap().is_modified());
        assert!(!system.workspace.has_modal(), "NEW FILE closed");
    }
}

#[test]
fn new_file_over_an_existing_script_stays_open_and_changes_nothing() {
    let fixture = Fixture::new("sys-new-file-exists");
    let system = in_code(&fixture, "-- keep\n");
    let mut input = vec![InputEvent::NewFile];
    input.extend(text("main"));
    input.push(InputEvent::Enter);
    let system = system.update(&input);
    assert!(system.workspace.has_modal(), "NEW FILE stays open");
    let main = fixture.root().join("pong/code/main.luau");
    assert_eq!(std::fs::read_to_string(main).unwrap(), "-- keep\n");
    // Escape closes NEW FILE and stays in the workspace.
    let system = press_break(system);
    assert!(is_workspace(&system) && !system.workspace.has_modal());
}
