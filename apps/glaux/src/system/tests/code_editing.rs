use super::*;
use pretty_assertions::assert_eq;

fn active_text(system: &System) -> Option<String> {
    let code = system.code.as_ref()?;
    Some(code.active_document()?.text().to_owned())
}

fn typed(system: System, text: &str) -> System {
    let events: Vec<InputEvent> = text
        .chars()
        .map(|ch| match ch {
            '\n' => InputEvent::Enter,
            ch => InputEvent::Text(ch),
        })
        .collect();
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

#[test]
fn suggestions_follow_unsaved_text_and_accepting_is_undoable() {
    for (args, dir, entry, local, member) in [
        (
            "my-game",
            "my-game",
            "code/main.luau",
            "local speed = 1\n",
            "audio.",
        ),
        (
            "owl --lang rhai",
            "owl",
            "code/main.rhai",
            "let speed = 1;\n",
            "audio::",
        ),
    ] {
        let fixture = Fixture::new(&format!("sys-code-complete-{dir}"));
        let system = starter_in_code(&fixture, args);
        let on_disk = std::fs::read_to_string(fixture.root().join(dir).join(entry)).unwrap();

        // A local that exists only in memory is offered, and so are members.
        let system = system.update(&[InputEvent::DocumentEnd { select: false }]);
        let system = typed(system, &format!("\n{local}sp"));
        let system = system.update(&[InputEvent::Tab { reverse: false }]);
        let text = active_text(&system).unwrap();
        assert!(text.ends_with("speed"), "{entry}: {text:?}");

        let system = typed(system, &format!("\n{member}to"));
        let before = active_text(&system).unwrap();
        let system = system.update(&[InputEvent::Tab { reverse: false }]);
        assert_eq!(
            active_text(&system).unwrap(),
            format!("{before}ne"),
            "{entry}"
        );
        let system = system.update(&[InputEvent::Undo]);
        assert_eq!(active_text(&system), Some(before), "{entry}");

        let after = std::fs::read_to_string(fixture.root().join(dir).join(entry)).unwrap();
        assert_eq!(after, on_disk, "{entry} was written");
    }
}

fn modal(system: &System) -> Option<(&str, Vec<&str>)> {
    workspace::tests_support::modal(&system.workspace)
}

fn notice(system: &System) -> Option<&str> {
    workspace::tests_support::notice(&system.workspace)
}

/// The starter `new my-game` makes, its entry script edited by typing
/// `typed` at its start, and the entry's host path.
fn edited_starter(fixture: &Fixture, typed_text: &str) -> (System, std::path::PathBuf) {
    let system = typed(starter_in_code(fixture, "my-game"), typed_text);
    (system, fixture.root().join("my-game/code/main.luau"))
}

#[test]
fn ctrl_s_writes_the_script_and_clears_its_unsaved_state() {
    let fixture = Fixture::new("sys-code-save");
    let (system, file) = edited_starter(&fixture, "-- hi\n");
    let before = std::fs::read_to_string(&file).unwrap();
    let text = active_text(&system).unwrap();

    let system = system.update(&[InputEvent::Save]);
    assert_eq!(std::fs::read_to_string(&file).unwrap(), text);
    assert_ne!(text, before);
    assert!(!system.code.as_ref().unwrap().is_modified());
    assert_eq!(
        notice(&system),
        Some(format!("Saved code/main.luau ({} bytes)", text.len()).as_str())
    );

    let system = system.update(&[InputEvent::Save]);
    assert_eq!(notice(&system), Some("Nothing to save"));
}

#[test]
fn an_outside_change_asks_before_overwriting_and_cancel_keeps_both() {
    let fixture = Fixture::new("sys-code-conflict-cancel");
    let (system, file) = edited_starter(&fixture, "x");
    let mine = active_text(&system).unwrap();
    std::fs::write(&file, "theirs").unwrap();

    let system = system.update(&[InputEvent::Save]);
    assert_eq!(
        modal(&system),
        Some(("FILE CONFLICT", vec!["OVERWRITE DISK", "CANCEL"]))
    );
    // Cancel is focused, so a stray Enter writes nothing.
    let system = system.update(&[InputEvent::Enter]);
    assert_eq!(modal(&system), None);
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "theirs");
    assert_eq!(active_text(&system), Some(mine));
    assert!(system.code.as_ref().unwrap().is_modified());
}

#[test]
fn overwrite_disk_writes_the_edits_unless_the_file_changed_again() {
    let fixture = Fixture::new("sys-code-conflict-overwrite");
    let (system, file) = edited_starter(&fixture, "x");
    let mine = active_text(&system).unwrap();
    std::fs::write(&file, "theirs").unwrap();
    let system = system.update(&[InputEvent::Save]);

    // Changed again while the dialog was up: another conflict, nothing
    // written over what the user has not seen.
    std::fs::write(&file, "theirs, again").unwrap();
    let overwrite = [InputEvent::Up { select: false }, InputEvent::Enter];
    let system = system.update(&overwrite);
    assert_eq!(
        modal(&system).map(|(title, _)| title),
        Some("FILE CONFLICT")
    );
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "theirs, again");

    let system = system.update(&overwrite);
    assert_eq!(modal(&system), None);
    assert_eq!(std::fs::read_to_string(&file).unwrap(), mine);
    assert!(!system.code.as_ref().unwrap().is_modified());
}

#[test]
fn a_deleted_script_is_written_again_only_once_confirmed() {
    let fixture = Fixture::new("sys-code-conflict-deleted");
    let (system, file) = edited_starter(&fixture, "x");
    let mine = active_text(&system).unwrap();
    std::fs::remove_file(&file).unwrap();

    let system = system.update(&[InputEvent::Save]);
    assert_eq!(
        modal(&system).map(|(title, _)| title),
        Some("FILE CONFLICT")
    );
    assert!(!file.exists());

    let system = system.update(&[InputEvent::Up { select: false }, InputEvent::Enter]);
    assert_eq!(std::fs::read_to_string(&file).unwrap(), mine);
    assert!(!system.code.as_ref().unwrap().is_modified());
}

#[test]
fn a_failed_save_is_reported_and_keeps_the_edits() {
    let fixture = Fixture::new("sys-code-save-failed");
    let (system, file) = edited_starter(&fixture, "x");
    let before = std::fs::read(&file).unwrap();
    let writable = std::fs::metadata(&file).unwrap().permissions();
    let mut read_only = writable.clone();
    read_only.set_readonly(true);
    std::fs::set_permissions(&file, read_only).unwrap();

    let system = system.update(&[InputEvent::Save]);
    std::fs::set_permissions(&file, writable).unwrap();
    assert_eq!(modal(&system), Some(("SAVE FAILED", vec!["OK"])));
    assert_eq!(std::fs::read(&file).unwrap(), before);
    assert!(system.code.as_ref().unwrap().is_modified());
}
