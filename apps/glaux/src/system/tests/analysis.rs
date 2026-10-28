//! The Luau analyzer wired to a real system.
//!
//! These tests drive a real `luau-lsp`. Set `LUAU_LSP` to its path to run
//! them, otherwise they return early, as `mise run luau-lsp-check` does.

use std::path::Path;
use std::time::{Duration, Instant};

use super::*;
use crate::lsp::Analysis;
use crate::scratch_dir::ScratchDir;
use crate::system::EditorDiagnostic;
use crate::system::Severity;
use crate::system::editing::history::EditKind;
use pretty_assertions::assert_eq;

fn analyzer() -> Option<&'static str> {
    let available = std::env::var_os("LUAU_LSP").is_some();
    if !available {
        eprintln!("skipped: set LUAU_LSP to a luau-lsp executable");
    }
    available.then_some("")
}

fn code_text(system: &System) -> String {
    let code = system.code.as_ref().unwrap();
    code.active_document().unwrap().text().to_owned()
}

fn write(fixture: &Fixture, path: &str, text: &str) {
    let file = fixture.root().join("pong").join(path);
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(file, text).unwrap();
}

/// The Code view of `pong` after `prepare` added files to its cartridge.
fn code_view(fixture: &Fixture, main: &str, prepare: impl FnOnce(&Fixture)) -> System {
    let system = shell_with_cartridge(fixture, main);
    prepare(fixture);
    press_break(enter(system, "load pong"))
}

/// Runs the analyzer's frame work until `done` holds.
fn settle(mut system: System, analysis: &mut Analysis, done: impl Fn(&System) -> bool) -> System {
    let deadline = Instant::now() + Duration::from_secs(20);
    while !done(&system) {
        assert!(Instant::now() < deadline, "the analyzer never answered");
        analysis.sync(&mut system);
        analysis.poll(&mut system);
        system = system.update(&[]);
        std::thread::sleep(Duration::from_millis(5));
    }
    system
}

fn active_findings(system: &System) -> Vec<(usize, Severity, String)> {
    let code = system.code.as_ref().unwrap();
    let document = code.active_document().unwrap();
    code.diagnostics(document.id())
        .iter()
        .map(|d: &EditorDiagnostic| {
            (
                document.line_of(d.range.start) + 1,
                d.severity,
                format!("{}: {}", d.code.as_deref().unwrap_or("-"), d.message),
            )
        })
        .collect()
}

fn has_findings(system: &System) -> bool {
    !active_findings(system).is_empty()
}

fn errors(system: &System) -> Vec<(usize, String)> {
    active_findings(system)
        .into_iter()
        .filter(|(_, severity, _)| *severity == Severity::Error)
        .map(|(line, _, message)| (line, message))
        .collect()
}

const NESTED_MAIN: &str = "--!strict
local enemies = require(\"game.enemies\")
local label: string = enemies.hp()
local missing = require(\"game.nope\")
print(label, 0, 0, 0)
print(missing, 0, 0, 0)
";

fn nested_modules(fixture: &Fixture) {
    write(
        fixture,
        "code/util.luau",
        "--!strict\nlocal M = {}\nfunction M.double(n: number): number\n    return n * 2\nend\nreturn M\n",
    );
    write(
        fixture,
        "code/game/enemies.luau",
        "--!strict\nlocal util = require(\"util\")\nlocal boss = require(\"game.deep.boss\")\nlocal M = {}\nfunction M.hp(): number\n    return util.double(boss.base)\nend\nreturn M\n",
    );
    write(
        fixture,
        "code/game/deep/boss.luau",
        "--!strict\nreturn { base = 10 }\n",
    );
}

#[test]
fn nested_requires_resolve_and_findings_reach_the_editor() {
    if analyzer().is_none() {
        return;
    }
    let fixture = Fixture::new("sys-lsp-nested");
    let cache = ScratchDir::new("lsp-nested-cache");
    let mut analysis = Analysis::new(Some(cache.to_path_buf()), None);
    let system = code_view(&fixture, NESTED_MAIN, nested_modules);
    let system = settle(system, &mut analysis, has_findings);
    let found = errors(&system);
    let lines: Vec<usize> = found.iter().map(|(line, _)| *line).collect();
    assert_eq!(lines, [3, 4], "{found:?}");
    assert!(
        found[0].1.contains("Expected this to be 'string'"),
        "{found:?}"
    );
    assert!(found[1].1.contains("Unknown require"), "{found:?}");
}

#[test]
fn an_edit_hides_the_old_findings_until_the_new_ones_arrive() {
    if analyzer().is_none() {
        return;
    }
    let fixture = Fixture::new("sys-lsp-edit");
    let cache = ScratchDir::new("lsp-edit-cache");
    let mut analysis = Analysis::new(Some(cache.to_path_buf()), None);
    let system = code_view(&fixture, NESTED_MAIN, nested_modules);
    let system = settle(system, &mut analysis, has_findings);

    let edited = system.update(&[InputEvent::Text('-'), InputEvent::Text('-')]);
    assert!(!has_findings(&edited), "stale findings must not be drawn");
    let code = edited.code.as_ref().unwrap();
    let summary = code
        .diagnostic_summary(code.active_document().unwrap().id())
        .unwrap();
    assert!(summary.stale && summary.errors == 2, "{summary:?}");

    let settled = settle(edited, &mut analysis, has_findings);
    let code = settled.code.as_ref().unwrap();
    let summary = code
        .diagnostic_summary(code.active_document().unwrap().id())
        .unwrap();
    assert!(!summary.stale, "{summary:?}");
}

#[test]
fn loot_rain_reports_game_run_and_not_its_own_callbacks() {
    if analyzer().is_none() {
        return;
    }
    let fixture = Fixture::new("sys-lsp-loot");
    let cache = ScratchDir::new("lsp-loot-cache");
    let mut analysis = Analysis::new(Some(cache.to_path_buf()), None);
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/loot-rain/code");
    let system = code_view(&fixture, "", |fixture| {
        for entry in std::fs::read_dir(&source).unwrap() {
            let entry = entry.unwrap();
            let name = entry.file_name();
            write(
                fixture,
                &format!("code/{}", name.to_string_lossy()),
                &std::fs::read_to_string(entry.path()).unwrap(),
            );
        }
    });
    let system = settle(system, &mut analysis, has_findings);
    let all = active_findings(&system);
    let run: Vec<_> = errors(&system)
        .into_iter()
        .filter(|(_, message)| message.contains("Key 'run' not found in table 'game'"))
        .collect();
    assert_eq!(run.len(), 1, "{all:?}");
    assert_eq!(run[0].0, 66);
    for callback in ["boot", "tick", "draw"] {
        assert!(
            all.iter()
                .all(|(_, _, message)| !message.contains(&format!("Function '{callback}'"))),
            "{callback} is reported unused: {all:?}"
        );
    }
}

#[test]
fn turning_the_setting_off_clears_the_findings_and_the_editor_still_works() {
    if analyzer().is_none() {
        return;
    }
    let fixture = Fixture::new("sys-lsp-off");
    let cache = ScratchDir::new("lsp-off-cache");
    let mut analysis = Analysis::new(Some(cache.to_path_buf()), None);
    let system = code_view(&fixture, NESTED_MAIN, nested_modules);
    let system = settle(system, &mut analysis, has_findings);
    let mut system = system.with_editor_settings(crate::system::EditorSettings {
        diagnostics: false,
        ..Default::default()
    });
    analysis.sync(&mut system);
    let typed = system.update(&[InputEvent::Text('x')]);
    assert!(code_text(&typed).starts_with('x'));
}

#[test]
fn without_an_executable_nothing_happens() {
    let fixture = Fixture::new("sys-lsp-none");
    let cache = ScratchDir::new("lsp-none-cache");
    let mut analysis = Analysis::new(Some(cache.to_path_buf()), None);
    let mut system = code_view(&fixture, NESTED_MAIN, nested_modules);
    // SAFETY: tests of this module are the only readers of these variables
    // and nextest runs each test in its own process.
    unsafe {
        std::env::set_var("GLAUX_LUAU_LSP", "/definitely/not/luau-lsp");
        std::env::remove_var("LUAU_LSP");
    }
    for _ in 0..5 {
        analysis.sync(&mut system);
        analysis.poll(&mut system);
        system = system.update(&[]);
    }
    assert!(!has_findings(&system));
    assert!(code_text(&system).starts_with("--!strict"));
}

fn findings_of(system: &System, path: &str) -> Vec<(usize, String)> {
    let code = system.code.as_ref().unwrap();
    let document = code
        .open_documents()
        .find(|document| document.path().as_str() == path)
        .unwrap();
    code.diagnostics(document.id())
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| (document.line_of(d.range.start) + 1, d.message.clone()))
        .collect()
}

#[test]
fn an_unsaved_edit_in_another_module_reaches_its_dependents_and_closing_it_undoes_that() {
    if analyzer().is_none() {
        return;
    }
    let fixture = Fixture::new("sys-lsp-dependent");
    let cache = ScratchDir::new("lsp-dependent-cache");
    let mut analysis = Analysis::new(Some(cache.to_path_buf()), None);
    let main = "--!strict\nlocal util = require(\"util\")\nlocal text: string = util.double(1)\nprint(text, 0, 0, 0)\n";
    let system = code_view(&fixture, main, nested_modules);
    let system = settle(system, &mut analysis, |s| {
        !findings_of(s, "code/main.luau").is_empty()
    });
    assert_eq!(findings_of(&system, "code/main.luau")[0].0, 3);

    // `double` now returns a string, in a tab nobody saved.
    let mut system = super::code_tabs::open_file(system, "util");
    let code = system.code.as_mut().unwrap();
    let util = code.active_document().unwrap();
    let (id, at) = (util.id(), util.text().find("): number").unwrap() + 3);
    code.edit(
        id,
        at..at + "number".len(),
        "string",
        EditKind::Typing,
        None,
    )
    .unwrap()
    .unwrap();
    let mut system = settle(system, &mut analysis, |s| {
        findings_of(s, "code/main.luau").is_empty()
    });
    assert_eq!(findings_of(&system, "code/main.luau"), []);

    // Closing the tab drops the edit, so the module is what is on disk again.
    system.code.as_mut().unwrap().close(id);
    let system = settle(system, &mut analysis, |s| {
        !findings_of(s, "code/main.luau").is_empty()
    });
    assert_eq!(findings_of(&system, "code/main.luau")[0].0, 3);
}

fn workspaces(cache: &Path) -> usize {
    std::fs::read_dir(cache.join("lsp/workspaces")).map_or(0, Iterator::count)
}

#[test]
fn changing_cartridge_closes_the_session_and_leaves_no_workspace_behind() {
    if analyzer().is_none() {
        return;
    }
    let fixture = Fixture::new("sys-lsp-swap");
    let cache = ScratchDir::new("lsp-swap-cache");
    let mut analysis = Analysis::new(Some(cache.to_path_buf()), None);
    let system = shell_with_cartridge(&fixture, NESTED_MAIN);
    nested_modules(&fixture);
    let mut system = enter(system, "load pong");
    analysis.sync(&mut system);
    assert_eq!(workspaces(&cache), 1);
    let placeholders = std::fs::read_dir(cache.join("lsp/workspaces"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let mut names: Vec<_> = std::fs::read_dir(placeholders)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    assert_eq!(
        names,
        [
            "game.deep.boss.luau",
            "game.enemies.luau",
            "main.luau",
            "util.luau"
        ]
    );

    let mut system = enter(system, "unload");
    analysis.sync(&mut system);
    assert_eq!(workspaces(&cache), 0);

    let mut system = enter(system, "load pong");
    analysis.sync(&mut system);
    assert_eq!(workspaces(&cache), 1);
}
