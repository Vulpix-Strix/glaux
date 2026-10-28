//! The Luau analyzer wired to a real system.
//!
//! These tests drive a real `luau-lsp`. Set `LUAU_LSP` to its path to run
//! them, otherwise they return early, as `mise run luau-lsp-check` does.

use std::path::Path;
use std::time::{Duration, Instant};

use super::*;
use crate::cartridge::source::SourcePath;
use crate::lsp::Analysis;
use crate::scratch_dir::ScratchDir;
use crate::system::EditorDiagnostic;
use crate::system::editing::history::EditKind;
use crate::system::{AnalyzerStatus, Severity};
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
    analysis.poll(&mut system);
    assert_eq!(analysis.status(), &AnalyzerStatus::Off);
    let typed = system.update(&[InputEvent::Text('x')]);
    assert!(code_text(&typed).starts_with('x'));
}

/// Frames of the analyzer's work and one tick of the system.
fn frames(mut system: System, analysis: &mut Analysis, count: usize) -> System {
    for _ in 0..count {
        analysis.sync(&mut system);
        analysis.poll(&mut system);
        system = system.update(&[]);
    }
    system
}

/// Runs frames until `done`, which looks at the analyzer, holds.
fn until(mut system: System, analysis: &mut Analysis, done: impl Fn(&Analysis) -> bool) -> System {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !done(analysis) {
        assert!(
            Instant::now() < deadline,
            "never got there: {:?}",
            analysis.status()
        );
        system = frames(system, analysis, 1);
        std::thread::sleep(Duration::from_millis(2));
    }
    system
}

fn unavailable(analysis: &Analysis) -> bool {
    matches!(analysis.status(), AnalyzerStatus::Unavailable(_))
}

#[test]
fn without_an_executable_the_status_says_why_and_the_user_is_told_once() {
    let fixture = Fixture::new("sys-lsp-none");
    let cache = ScratchDir::new("lsp-none-cache");
    let mut analysis = Analysis::new(Some(cache.to_path_buf()), None);
    // SAFETY: each test runs in its own process under nextest, and no other
    // thread reads the environment while it is changed.
    unsafe {
        std::env::set_var("GLAUX_LUAU_LSP", "/definitely/not/luau-lsp");
        std::env::remove_var("LUAU_LSP");
    }
    let system = shell_with_cartridge(&fixture, NESTED_MAIN);
    nested_modules(&fixture);
    let system = enter(system, "load pong");
    let system = until(system, &mut analysis, unavailable);
    let system = frames(system, &mut analysis, 1);

    match analysis.status() {
        AnalyzerStatus::Unavailable(reason) => {
            assert!(reason.starts_with("GLAUX_LUAU_LSP is set to"), "{reason}")
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(
        system.notice_text(),
        Some("luau-lsp not found, see the log (F12)")
    );
    assert!(
        !cache.join("lsp").exists(),
        "nothing was started or written"
    );

    // The notice runs out while the workspace ticks, and loading the
    // cartridge again looks again but does not repeat it.
    let system = frames(press_break(system), &mut analysis, 200);
    assert_eq!(system.notice_text(), None);
    let system = enter(press_break(system), "unload");
    let system = frames(system, &mut analysis, 2);
    let system = until(enter(system, "load pong"), &mut analysis, |a| {
        matches!(a.status(), AnalyzerStatus::Starting)
    });
    let system = until(system, &mut analysis, unavailable);
    assert!(matches!(analysis.status(), AnalyzerStatus::Unavailable(_)));
    assert_eq!(system.notice_text(), None);
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

/// The files of the only workspace under `cache`, if there is exactly one.
fn placeholders_in(cache: &Path) -> Option<Vec<std::path::PathBuf>> {
    let mut workspaces = std::fs::read_dir(cache.join("lsp/workspaces")).ok()?;
    let workspace = workspaces.next()?.ok()?.path();
    if workspaces.next().is_some() {
        return None;
    }
    std::fs::read_dir(workspace)
        .ok()?
        .map(|entry| entry.ok().map(|entry| entry.path()))
        .collect()
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
    let system = enter(system, "load pong");
    let system = until(system, &mut analysis, |_| {
        placeholders_in(&cache).is_some_and(|names| names.len() == 4)
    });
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

    let system = enter(system, "unload");
    let system = until(system, &mut analysis, |_| workspaces(&cache) == 0);

    let system = enter(system, "load pong");
    let _system = until(system, &mut analysis, |_| workspaces(&cache) == 1);
}

/// Servers that misbehave, played by `crates/glaux-lsp/tests/fake_server.py`
/// behind a one-line shell script named like luau-lsp.
#[cfg(unix)]
mod failing_servers {
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;

    use pretty_assertions::assert_eq;

    use super::*;

    /// Starting python can take a while on a busy machine.
    const PATIENT: Duration = Duration::from_secs(5);

    fn python_available() -> bool {
        std::process::Command::new("python3")
            .arg("--version")
            .output()
            .is_ok_and(|output| output.status.success())
    }

    /// Makes `GLAUX_LUAU_LSP` name a script that runs the fake server with
    /// `mode`, and returns the cache the analyzer should use.
    fn serving(mode: &str, name: &str) -> ScratchDir {
        let scratch = ScratchDir::new(name);
        let script = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../crates/glaux-lsp/tests/fake_server.py");
        let exe: PathBuf = scratch.join("luau-lsp");
        std::fs::write(
            &exe,
            format!("#!/bin/sh\nexec python3 \"{}\" {mode}\n", script.display()),
        )
        .unwrap();
        std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755)).unwrap();
        // SAFETY: each test runs in its own process under nextest, and no
        // other thread reads the environment while it is changed.
        unsafe {
            std::env::set_var("GLAUX_LUAU_LSP", &exe);
            std::env::remove_var("LUAU_LSP");
        }
        scratch
    }

    /// Runs frames until `done` and returns each status the analyzer went
    /// through, without repeats.
    fn trace(
        mut system: System,
        analysis: &mut Analysis,
        done: impl Fn(&System, &[AnalyzerStatus]) -> bool,
    ) -> (System, Vec<AnalyzerStatus>) {
        let mut seen: Vec<AnalyzerStatus> = Vec::new();
        let deadline = Instant::now() + Duration::from_secs(10);
        while !done(&system, &seen) {
            assert!(Instant::now() < deadline, "never got there: {seen:?}");
            system = frames(system, analysis, 1);
            if seen.last() != Some(analysis.status()) {
                seen.push(analysis.status().clone());
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        (system, seen)
    }

    fn disabled(_: &System, seen: &[AnalyzerStatus]) -> bool {
        matches!(seen.last(), Some(AnalyzerStatus::Disabled(_)))
    }

    fn start(
        name: &str,
        mode: &str,
        handshake: Duration,
    ) -> Option<(ScratchDir, Analysis, System)> {
        if !python_available() {
            eprintln!("skipped: python3 is needed to play the fake server");
            return None;
        }
        let fixture = Fixture::new(name);
        let cache = serving(mode, &format!("{name}-cache"));
        let analysis =
            Analysis::new(Some(cache.join("cache")), None).with_quick_failures(handshake);
        let system = code_view(&fixture, "local x = 1\n", |_| {});
        // The fixture lives as long as the process; the test is over by then.
        std::mem::forget(fixture);
        Some((cache, analysis, system))
    }

    #[test]
    fn a_server_that_exits_is_failed_then_disabled_and_editing_goes_on() {
        let Some((_cache, mut analysis, system)) = start("sys-lsp-exits", "exit", PATIENT) else {
            return;
        };
        let (system, seen) = trace(system, &mut analysis, disabled);
        assert_eq!(seen[0], AnalyzerStatus::Starting);
        let Some(AnalyzerStatus::Failed { reason, .. }) = seen
            .iter()
            .find(|s| matches!(s, AnalyzerStatus::Failed { .. }))
        else {
            panic!("{seen:?}");
        };
        assert!(reason.contains("exit status: 3"), "{reason}");
        assert!(reason.contains("fake server: cannot start"), "{reason}");
        assert!(matches!(seen.last(), Some(AnalyzerStatus::Disabled(_))));
        assert_eq!(
            system.notice_text(),
            Some("Diagnostics stopped, see the log (F12)")
        );

        let typed = system.update(&[InputEvent::Text('x')]);
        assert!(code_text(&typed).starts_with('x'));
        assert!(!has_findings(&typed));
    }

    #[test]
    fn a_server_that_never_answers_is_given_up_on() {
        let Some((_cache, mut analysis, system)) =
            start("sys-lsp-silent", "silent", Duration::from_millis(300))
        else {
            return;
        };
        let (_, seen) = trace(system, &mut analysis, disabled);
        let Some(AnalyzerStatus::Failed { reason, .. }) = seen
            .iter()
            .find(|s| matches!(s, AnalyzerStatus::Failed { .. }))
        else {
            panic!("{seen:?}");
        };
        assert!(
            reason.contains("did not answer initialize in time"),
            "{reason}"
        );
    }

    #[test]
    fn a_server_that_dies_after_starting_is_restarted_and_the_tab_gets_its_findings() {
        let marker = std::env::temp_dir().join(format!("glaux-app-crash-{}", std::process::id()));
        let _ = std::fs::remove_file(&marker);
        let mode = format!("crash-once {}", marker.display());
        let Some((_cache, mut analysis, system)) = start("sys-lsp-crash", &mode, PATIENT) else {
            return;
        };
        let (system, seen) = trace(system, &mut analysis, |system, seen| {
            seen.iter()
                .any(|s| matches!(s, AnalyzerStatus::Failed { .. }))
                && seen.last() == Some(&AnalyzerStatus::Ready)
                && has_findings(system)
        });
        let _ = std::fs::remove_file(&marker);
        assert_eq!(seen.first(), Some(&AnalyzerStatus::Starting), "{seen:?}");
        assert_eq!(errors(&system).len(), 1);
    }
}

/// Replaces `from` by `to` in the open tab of `path`, as a person would type.
fn retype(system: &mut System, path: &str, from: &str, to: &str) {
    let code = system.code.as_mut().unwrap();
    let path = SourcePath::parse(path).unwrap();
    let document = code.open_documents().find(|d| d.path() == &path).unwrap();
    let at = document.text().find(from).unwrap();
    let id = document.id();
    code.edit(id, at..at + from.len(), to, EditKind::Typing, None)
        .unwrap()
        .unwrap();
}

/// `pong` with `modules` chained modules `m0` to `m{n-1}` and a `main` that
/// requires the last. Returns the shell, not yet loaded.
fn chain_cartridge(fixture: &Fixture, modules: usize) -> System {
    let main = format!(
        "--!strict\nlocal last = require(\"m{}\")\nlocal n: number = last.value()\nprint(n, 0, 0, 0)\n",
        modules - 1
    );
    let system = shell_with_cartridge(fixture, &main);
    write(
        fixture,
        "code/m0.luau",
        "--!strict\nlocal M = {}\nfunction M.value() return 1 end\nreturn M\n",
    );
    for k in 1..modules {
        let text = format!(
            "--!strict\nlocal prev = require(\"m{}\")\nlocal M = {{}}\nfunction M.value() return prev.value() end\nreturn M\n",
            k - 1
        );
        write(fixture, &format!("code/m{k}.luau"), &text);
    }
    system
}

#[test]
fn an_edit_reaches_the_far_end_of_a_cartridge_with_hundreds_of_modules() {
    if analyzer().is_none() {
        return;
    }
    // Modules are handed to the server in name order, so `m100` arrives
    // before `m99` that it requires. A server asked about it too early would
    // remember the failed lookup and never connect the two, so the edit made
    // at `m0` would not reach `main`.
    const MODULES: usize = 300;
    let fixture = Fixture::new("sys-lsp-large");
    let cache = ScratchDir::new("lsp-large-cache");
    let mut analysis = Analysis::new(Some(cache.to_path_buf()), None);
    let system = chain_cartridge(&fixture, MODULES);
    let mut system = press_break(enter(system, "load pong"));
    for path in ["code/main.luau", "code/m0.luau"] {
        open_tab(&mut system, path);
    }
    let analyzed = |system: &System| {
        system
            .code
            .as_ref()
            .and_then(|code| {
                let document = code
                    .open_documents()
                    .find(|d| d.path().as_str() == "code/main.luau")?;
                code.diagnostic_summary(document.id())
            })
            .is_some_and(|summary| !summary.stale)
    };
    let system = settle(system, &mut analysis, analyzed);
    assert_eq!(findings_of(&system, "code/main.luau"), []);

    let mut system = system;
    retype(&mut system, "code/m0.luau", "return 1", "return \"a\"");
    let system = settle(system, &mut analysis, |s| {
        !findings_of(s, "code/main.luau").is_empty()
    });
    let found = findings_of(&system, "code/main.luau");
    assert!(
        found[0].1.contains("Expected this to be 'number'"),
        "{found:?}"
    );
}

/// Opens the script at `path` in a tab of the Code view.
pub(super) fn open_tab(system: &mut System, path: &str) {
    let cartridge = crate::system::state::cartridge_of(&system.state)
        .unwrap()
        .clone();
    let path = SourcePath::parse(path).unwrap();
    system
        .code
        .as_mut()
        .unwrap()
        .open(&cartridge, &path)
        .unwrap();
}

/// Waits, without running frames, until the actor has made a workspace.
fn wait_for_workspace(cache: &Path) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while workspaces(cache) == 0 {
        assert!(Instant::now() < deadline, "no workspace was made");
        std::thread::sleep(Duration::from_micros(100));
    }
}

#[test]
fn unloading_while_the_workspace_is_being_prepared_leaves_nothing_behind() {
    if analyzer().is_none() {
        return;
    }
    let fixture = Fixture::new("sys-lsp-cancel");
    let cache = ScratchDir::new("lsp-cancel-cache");
    let mut analysis = Analysis::new(Some(cache.to_path_buf()), None);
    let system = chain_cartridge(&fixture, 400);

    // Unload the moment the workspace starts to exist, as when a cartridge
    // is replaced right after being loaded. The disk is slow, so the
    // preparation is certainly half done and has to stop and clean up.
    crate::lsp::SLOW_MODULES.store(2000, std::sync::atomic::Ordering::Relaxed);
    let mut system = enter(system, "load pong");
    analysis.sync(&mut system);
    wait_for_workspace(&cache);
    let mut system = enter(system, "unload");
    analysis.sync(&mut system);
    analysis.poll(&mut system);

    // Wherever the preparation had got to, it ends with no workspace and no
    // analyzer running for a cartridge that is gone.
    std::thread::sleep(Duration::from_millis(300));
    let system = frames(system, &mut analysis, 3);
    assert_eq!(workspaces(&cache), 0, "{:?}", placeholders_in(&cache));
    assert_ne!(analysis.status(), &AnalyzerStatus::Ready);
    drop(system);
}

#[test]
fn replacing_a_cartridge_never_shows_the_old_ones_findings_on_the_new() {
    if analyzer().is_none() {
        return;
    }
    let fixture = Fixture::new("sys-lsp-replace");
    let cache = ScratchDir::new("lsp-replace-cache");
    let mut analysis = Analysis::new(Some(cache.to_path_buf()), None);
    // `pong` has an error in `main`. `clean` has none.
    let system = shell_with_cartridge(
        &fixture,
        "--!strict\nlocal n: number = \"a\"\nprint(n, 0, 0, 0)\n",
    );
    let clean = fixture.root().join("clean");
    crate::cartridge::fixture::write_cartridge(&clean, "Clean");
    std::fs::write(
        clean.join("code/main.luau"),
        "--!strict\nlocal n: number = 1\nprint(n, 0, 0, 0)\n",
    )
    .unwrap();

    let system = press_break(enter(system, "load pong"));
    let system = settle(system, &mut analysis, has_findings);
    assert_eq!(errors(&system).len(), 1);

    // Replace it and keep going until the new one has its own answer.
    let system = enter(press_break(system), "load clean");
    let mut system = press_break(system);
    let began = Instant::now();
    let mut answered = false;
    while began.elapsed() < Duration::from_secs(5) && !answered {
        system = frames(system, &mut analysis, 1);
        let errors = errors(&system);
        assert_eq!(errors, [], "the old cartridge's error was shown");
        answered = system
            .code
            .as_ref()
            .and_then(|code| code.diagnostic_summary(code.active_document()?.id()))
            .is_some_and(|summary| !summary.stale);
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(answered, "the new cartridge never got its answer");
}

#[test]
fn dropping_the_analyzer_in_the_middle_of_a_start_is_prompt_and_tidy() {
    if analyzer().is_none() {
        return;
    }
    let fixture = Fixture::new("sys-lsp-drop");
    let cache = ScratchDir::new("lsp-drop-cache");
    let mut analysis = Analysis::new(Some(cache.to_path_buf()), None);
    let system = chain_cartridge(&fixture, 400);
    crate::lsp::SLOW_MODULES.store(2000, std::sync::atomic::Ordering::Relaxed);
    let mut system = enter(system, "load pong");
    analysis.sync(&mut system);
    wait_for_workspace(&cache);

    let began = Instant::now();
    drop(analysis);
    assert!(
        began.elapsed() < Duration::from_secs(4),
        "closing took {:?}",
        began.elapsed()
    );
    assert_eq!(workspaces(&cache), 0, "{:?}", placeholders_in(&cache));
}
