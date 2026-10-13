use super::*;
use pretty_assertions::assert_eq;

fn active_text(system: &System) -> Option<String> {
    let code = system.code.as_ref()?;
    Some(code.active_document()?.text().to_owned())
}

fn typed(system: System, text: &str) -> System {
    let events: Vec<InputEvent> = text.chars().map(InputEvent::Text).collect();
    system.update(&events)
}

/// The cartridge `new` makes with `args`, shown in the workspace's Code
/// view with its entry script open.
fn starter_in_code(fixture: &Fixture, args: &str) -> System {
    let system = enter(shell_with_cartridge(fixture, ""), &format!("new {args}"));
    let system = press_break(system);
    assert_eq!(workspace_tab(&system), Tab::Code);
    system
}

#[test]
fn starter_scripts_are_edited_in_memory_only() {
    for (args, dir, entry) in [
        ("my-game", "my-game", "code/main.luau"),
        ("owl --lang rhai", "owl", "code/main.rhai"),
    ] {
        let fixture = Fixture::new(&format!("sys-code-edit-starter-{dir}"));
        let system = starter_in_code(&fixture, args);
        let on_disk = std::fs::read_to_string(fixture.root().join(dir).join(entry)).unwrap();
        assert_eq!(active_text(&system).as_deref(), Some(on_disk.as_str()));

        let system = typed(system, "x = 1");
        let system = system.update(&[InputEvent::Enter, InputEvent::Delete]);
        let expected = format!("x = 1\n{}", &on_disk[1..]);
        assert_eq!(active_text(&system), Some(expected), "{entry}");
        let after = std::fs::read_to_string(fixture.root().join(dir).join(entry)).unwrap();
        assert_eq!(after, on_disk, "{entry} was written");
        assert!(system.code.as_ref().unwrap().is_modified());
    }
}

#[test]
fn only_the_code_view_takes_typing_for_its_scripts() {
    let fixture = Fixture::new("sys-code-edit-focus");
    let system = press_break(enter(
        shell_with_cartridge(&fixture, "-- main\n"),
        "load pong",
    ));
    let system = typed(system, "a");
    assert_eq!(active_text(&system).as_deref(), Some("a-- main\n"));

    // On another view, and in the shell, typing leaves the script alone.
    let system = system.update(&[InputEvent::NextView]);
    let system = system.update(&[InputEvent::Text('b'), InputEvent::Backspace]);
    let system = system.update(&[InputEvent::PreviousView]);
    assert_eq!(active_text(&system).as_deref(), Some("a-- main\n"));
    let system = enter(press_break(system), "info");
    assert!(!is_workspace(&system));

    // Edits outlive the trip, and the caret is where it was left.
    let system = typed(press_break(system), "c");
    assert_eq!(workspace_tab(&system), Tab::Code);
    assert_eq!(active_text(&system).as_deref(), Some("ac-- main\n"));
}
