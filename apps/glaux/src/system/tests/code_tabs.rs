use super::*;
use pretty_assertions::assert_eq;

/// The open scripts' paths, in tab order.
fn open_scripts(system: &System) -> Vec<String> {
    let Some(code) = &system.code else {
        return Vec::new();
    };
    code.tabs()
        .ids()
        .map(|id| code.document(id).unwrap().path().to_string())
        .collect()
}

fn active_script(system: &System) -> Option<String> {
    let code = system.code.as_ref()?;
    Some(code.active_document()?.path().to_string())
}

fn notice(system: &System) -> Option<&str> {
    workspace::tests_support::notice(&system.workspace)
}

/// Opens `filter`'s best match through OPEN FILE.
pub(super) fn open_file(system: System, filter: &str) -> System {
    let mut input = vec![InputEvent::OpenFile];
    input.extend(filter.chars().map(InputEvent::Text));
    input.push(InputEvent::Enter);
    system.update(&input)
}

/// The `Pong` cartridge loaded with `files` written beside its entry.
pub(super) fn loaded_with(fixture: &Fixture, files: &[(&str, &str)]) -> System {
    let system = shell_with_cartridge(fixture, "-- main\n");
    let dir = fixture.root().join("pong");
    for (path, text) in files {
        let host = dir.join(path);
        std::fs::create_dir_all(host.parent().unwrap()).unwrap();
        std::fs::write(host, text).unwrap();
    }
    enter(system, "load pong")
}

#[test]
fn the_entry_opens_on_the_first_visit_to_code_only() {
    let fixture = Fixture::new("sys-code-entry");
    let system = press_break(loaded(&fixture));
    assert_eq!(workspace_tab(&system), Tab::Code);
    assert_eq!(open_scripts(&system), ["code/main.luau"]);

    // Closed tabs stay closed across visits.
    let system = system.update(&[InputEvent::CloseDocument]);
    let system = press_break(press_break(system));
    assert!(is_workspace(&system));
    assert_eq!(open_scripts(&system), Vec::<String>::new());
}

#[test]
fn the_entry_waits_until_the_code_view_first_shows() {
    let fixture = Fixture::new("sys-code-entry-later");
    write_cartridge(&fixture.root().join("other"), "Other");
    // Leave the workspace on Sprites, then switch cartridges.
    let system = press_break(loaded(&fixture)).update(&[InputEvent::NextView]);
    let system = enter(press_break(system), "load other");
    let system = press_break(system);
    assert_eq!(workspace_tab(&system), Tab::Sprites);
    assert_eq!(open_scripts(&system), Vec::<String>::new());

    let system = system.update(&[InputEvent::PreviousView]);
    assert_eq!(open_scripts(&system), ["code/main.luau"]);
}

#[test]
fn tabs_survive_trips_to_sprites_and_the_shell() {
    let fixture = Fixture::new("sys-code-keep");
    let system = loaded_with(
        &fixture,
        &[
            ("code/enemies/bat.luau", "-- bat\n"),
            ("code/ui/hud.rhai", ""),
        ],
    );
    let system = open_file(press_break(system), "bat");
    let system = open_file(system, "hud");
    let system = system.update(&[InputEvent::PreviousDocument]);
    let tabs = [
        "code/main.luau",
        "code/enemies/bat.luau",
        "code/ui/hud.rhai",
    ];
    assert_eq!(open_scripts(&system), tabs);
    assert_eq!(
        active_script(&system).as_deref(),
        Some("code/enemies/bat.luau")
    );

    let system = system.update(&[InputEvent::NextView]);
    let system = press_break(press_break(system));
    let system = system.update(&[InputEvent::PreviousView]);
    assert_eq!(workspace_tab(&system), Tab::Code);
    assert_eq!(open_scripts(&system), tabs);
    assert_eq!(
        active_script(&system).as_deref(),
        Some("code/enemies/bat.luau")
    );

    // Running and stopping the cartridge keeps them too.
    let system = stop_cartridge(enter(press_break(system), "run"));
    let system = press_break(system);
    assert_eq!(open_scripts(&system), tabs);
}

#[test]
fn opening_an_open_file_again_shows_its_tab() {
    let fixture = Fixture::new("sys-code-dup");
    let system = loaded_with(&fixture, &[("code/a.luau", "")]);
    let system = open_file(press_break(system), "a.luau");
    let system = open_file(system, "main");
    assert_eq!(open_scripts(&system), ["code/main.luau", "code/a.luau"]);
    assert_eq!(active_script(&system).as_deref(), Some("code/main.luau"));
}

#[test]
fn a_failed_load_keeps_the_open_scripts() {
    let fixture = Fixture::new("sys-code-failed-load");
    std::fs::create_dir_all(fixture.root().join("broken")).unwrap();
    std::fs::write(fixture.root().join("broken/cartridge.ron"), "(").unwrap();
    let system = loaded_with(&fixture, &[("code/a.luau", "")]);
    let system = open_file(press_break(system), "a.luau");
    let tabs = open_scripts(&system);

    let system = enter(press_break(system), "load broken");
    let system = enter(system, "load missing");
    assert_eq!(loaded_name(&system), Some("Pong"));
    let system = press_break(system);
    assert_eq!(open_scripts(&system), tabs);
    assert_eq!(active_script(&system).as_deref(), Some("code/a.luau"));
}

#[test]
fn loading_another_cartridge_or_unloading_drops_the_open_scripts() {
    let fixture = Fixture::new("sys-code-switch");
    write_cartridge(&fixture.root().join("other"), "Other");
    std::fs::write(fixture.root().join("other/code/extra.luau"), "").unwrap();
    let system = loaded_with(&fixture, &[("code/a.luau", "")]);
    let system = open_file(press_break(system), "a.luau");
    assert_eq!(open_scripts(&system), ["code/main.luau", "code/a.luau"]);

    let system = enter(press_break(system), "load other");
    assert!(system.code.is_none());
    // The new cartridge's entry opens as on a first visit.
    let system = press_break(system);
    assert_eq!(open_scripts(&system), ["code/main.luau"]);
    assert_eq!(
        system
            .code
            .as_ref()
            .unwrap()
            .active_document()
            .unwrap()
            .text(),
        ""
    );

    let system = enter(press_break(system), "unload");
    assert!(system.code.is_none());
}

#[test]
fn escape_closes_open_file_without_leaving_the_workspace() {
    let fixture = Fixture::new("sys-code-picker-esc");
    let system = press_break(loaded(&fixture));
    let system = system.update(&[InputEvent::OpenFile]);
    assert!(system.workspace.code().picker().is_some());
    let system = press_break(system);
    assert!(is_workspace(&system));
    assert!(system.workspace.code().picker().is_none());
    let system = press_break(system);
    assert!(!is_workspace(&system));
}

#[test]
fn a_file_gone_before_it_opens_is_reported_and_changes_nothing() {
    let fixture = Fixture::new("sys-code-gone");
    let system = loaded_with(&fixture, &[("code/gone.luau", "")]);
    let system = press_break(system).update(&[InputEvent::OpenFile]);
    std::fs::remove_file(fixture.root().join("pong/code/gone.luau")).unwrap();
    let mut input: Vec<InputEvent> = "gone".chars().map(InputEvent::Text).collect();
    input.push(InputEvent::Enter);
    let system = system.update(&input);

    assert!(is_workspace(&system));
    assert_eq!(open_scripts(&system), ["code/main.luau"]);
    assert_eq!(
        notice(&system),
        Some("Cannot open code/gone.luau: No such file: code/gone.luau")
    );
}

#[test]
fn a_missing_entry_leaves_an_empty_view_that_says_why() {
    let fixture = Fixture::new("sys-code-no-entry");
    let system = loaded(&fixture);
    std::fs::remove_file(fixture.root().join("pong/code/main.luau")).unwrap();
    let system = press_break(system);
    assert!(is_workspace(&system));
    assert_eq!(open_scripts(&system), Vec::<String>::new());
    assert_eq!(notice(&system), Some("Cannot open the entry script"));
    // The workspace still works: OPEN FILE opens.
    let system = system.update(&[InputEvent::OpenFile]);
    assert!(system.workspace.code().picker().is_some());
}

#[test]
fn the_active_tab_is_underlined_in_the_accent_color() {
    let fixture = Fixture::new("sys-code-draw");
    let system = loaded_with(&fixture, &[("code/a.luau", "")]);
    let system = open_file(press_break(system), "a.luau");
    let code = system.code.as_ref().unwrap();
    let rects = system.workspace.code().tab_rects(code);
    assert_eq!(rects.len(), 2);
    let fb = render(&system);
    let underline = |rect: workspace::Rect| {
        let bottom = rect.y + rect.height - 1;
        (rect.x..rect.x + rect.width).all(|x| fb.pixel(x, bottom) == Some(ACCENT))
    };
    // `a.luau` opened after the entry and is active.
    assert!(underline(rects[1].1));
    assert!(!underline(rects[0].1));
}

#[test]
fn changed_editor_settings_are_handed_to_the_host_to_keep() {
    let fixture = Fixture::new("sys-code-settings");
    let kept = EditorSettings {
        tab_size: 2,
        ..EditorSettings::default()
    };
    let system = press_break(loaded(&fixture).with_editor_settings(kept));
    assert_eq!(system.workspace.code().settings(), kept);

    let gear = system.workspace.code().layout().settings_button();
    let at = Point::new(
        (gear.x + gear.width / 2) as u16,
        (gear.y + gear.height / 2) as u16,
    );
    let mut system = system.update(&[
        InputEvent::PointerDown {
            button: PointerButton::Primary,
            at,
            select: false,
        },
        InputEvent::Enter,
        InputEvent::Break,
    ]);
    let wrapped = EditorSettings {
        word_wrap: true,
        ..kept
    };
    assert_eq!(
        system.take_host_request(),
        Some(HostRequest::SaveEditorSettings(wrapped))
    );
    assert_eq!(system.take_host_request(), None);
    // Escape closed the window, not the workspace.
    assert!(is_workspace(&system));
}
