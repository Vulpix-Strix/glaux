//! Scripts changed on disk while the analyzer runs, with the real watcher.

use super::*;
use pretty_assertions::assert_eq;

const MAIN: &str = "--!strict\nlocal util = require(\"util\")\nlocal text: string = util.double(1)\nprint(text, 0, 0, 0)\n";

/// The analyzer settled on `MAIN` over the usual modules: one error, on the
/// line where a number is stored as a string.
struct Rig {
    fixture: Fixture,
    _cache: ScratchDir,
    analysis: Analysis,
    system: Option<System>,
}

impl Rig {
    fn new(label: &str, analysis: impl FnOnce(Option<std::path::PathBuf>) -> Analysis) -> Self {
        let fixture = Fixture::new(&format!("sys-lsp-ext-{label}"));
        let cache = ScratchDir::new(&format!("lsp-ext-{label}-cache"));
        let analysis = analysis(Some(cache.to_path_buf()));
        let system = code_view(&fixture, MAIN, nested_modules);
        let mut rig = Self {
            fixture,
            _cache: cache,
            analysis,
            system: None,
        };
        rig.system = Some(rig.until(system, |s| !findings_of(s, "code/main.luau").is_empty()));
        rig
    }

    fn until(&mut self, system: System, done: impl Fn(&System) -> bool) -> System {
        settle(system, &mut self.analysis, done)
    }

    fn wait(&mut self, done: impl Fn(&System) -> bool) {
        let system = self.system.take().unwrap();
        self.system = Some(self.until(system, done));
    }

    fn main_errors(&self) -> Vec<(usize, String)> {
        findings_of(self.system.as_ref().unwrap(), "code/main.luau")
    }

    fn path(&self, relative: &str) -> std::path::PathBuf {
        self.fixture.root().join("pong").join(relative)
    }

    /// Changes a file as another program would, without Glaux's help.
    fn outside(&self, relative: &str, text: &str) {
        let file = self.path(relative);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, text).unwrap();
    }

    fn broken(&mut self) {
        self.wait(|s| !findings_of(s, "code/main.luau").is_empty());
        assert_eq!(self.main_errors()[0].0, 3);
    }

    fn fixed(&mut self) {
        self.wait(|s| findings_of(s, "code/main.luau").is_empty());
    }
}

const STRING_UTIL: &str = "--!strict\nlocal M = {}\nfunction M.double(n: number): string\n    return tostring(n * 2)\nend\nreturn M\n";
const NUMBER_UTIL: &str = "--!strict\nlocal M = {}\nfunction M.double(n: number): number\n    return n * 2\nend\nreturn M\n";

#[test]
fn a_script_edited_outside_glaux_reaches_the_tabs_that_require_it() {
    if analyzer().is_none() {
        return;
    }
    let mut rig = Rig::new("modify", |cache| Analysis::new(cache, None));
    rig.outside("code/util.luau", STRING_UTIL);
    rig.fixed();
    rig.outside("code/util.luau", NUMBER_UTIL);
    rig.broken();
}

#[test]
fn a_script_created_and_then_deleted_outside_glaux_is_followed() {
    if analyzer().is_none() {
        return;
    }
    let mut rig = Rig::new("create", |cache| Analysis::new(cache, None));
    // `util` now forwards a module that does not exist, which is `any`.
    rig.outside("code/util.luau", "--!strict\nreturn require(\"extra\")\n");
    rig.fixed();
    // Once it exists, `main` sees a number where it wants a string.
    rig.outside("code/extra.luau", NUMBER_UTIL);
    rig.broken();
    std::fs::remove_file(rig.path("code/extra.luau")).unwrap();
    rig.fixed();
}

#[test]
fn renaming_a_script_outside_glaux_breaks_and_restores_its_dependents() {
    if analyzer().is_none() {
        return;
    }
    let mut rig = Rig::new("rename", |cache| Analysis::new(cache, None));
    rig.outside("code/util.luau", STRING_UTIL);
    rig.fixed();
    std::fs::rename(rig.path("code/util.luau"), rig.path("code/util2.luau")).unwrap();
    rig.wait(|s| {
        findings_of(s, "code/main.luau")
            .iter()
            .any(|(_, m)| m.contains("Unknown require"))
    });
    std::fs::rename(rig.path("code/util2.luau"), rig.path("code/util.luau")).unwrap();
    rig.fixed();
}

#[test]
fn a_save_through_a_temporary_file_is_noticed() {
    if analyzer().is_none() {
        return;
    }
    let mut rig = Rig::new("atomic", |cache| Analysis::new(cache, None));
    rig.outside("code/.util.luau.tmp", STRING_UTIL);
    std::fs::rename(rig.path("code/.util.luau.tmp"), rig.path("code/util.luau")).unwrap();
    rig.fixed();
}

#[test]
fn a_script_in_a_subdirectory_is_followed() {
    if analyzer().is_none() {
        return;
    }
    let fixture = Fixture::new("sys-lsp-ext-nested");
    let cache = ScratchDir::new("lsp-ext-nested-cache");
    let mut analysis = Analysis::new(Some(cache.to_path_buf()), None);
    let system = code_view(&fixture, NESTED_MAIN, nested_modules);
    let system = settle(system, &mut analysis, has_findings);
    assert_eq!(errors(&system).len(), 2, "{:?}", errors(&system));
    // `enemies.hp` stops returning a number, so line 3 is no longer an error.
    std::fs::write(
        fixture.root().join("pong/code/game/enemies.luau"),
        "--!strict\nlocal M = {}\nfunction M.hp(): string\n    return \"x\"\nend\nreturn M\n",
    )
    .unwrap();
    let system = settle(system, &mut analysis, |s| {
        errors(s).iter().all(|(line, _)| *line != 3)
    });
    assert_eq!(errors(&system).len(), 1, "{:?}", errors(&system));
}

#[test]
fn an_open_tab_wins_over_the_disk_until_it_closes() {
    if analyzer().is_none() {
        return;
    }
    let mut rig = Rig::new("tab", |cache| Analysis::new(cache, None));
    let mut system = super::super::code_tabs::open_file(rig.system.take().unwrap(), "util");
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
    rig.system = Some(system);
    rig.fixed();

    // The disk changes underneath a tab with unsaved text. The tab counts.
    rig.outside("code/util.luau", NUMBER_UTIL);
    let system = rig.system.take().unwrap();
    let system = idle_for(system, &mut rig.analysis, Duration::from_millis(1200));
    rig.system = Some(system);
    assert_eq!(rig.main_errors(), []);

    // Closing the tab drops the edit, and the disk is what counts again.
    let mut system = rig.system.take().unwrap();
    system.code.as_mut().unwrap().close(id);
    rig.system = Some(system);
    rig.broken();
}

fn idle_for(mut system: System, analysis: &mut Analysis, span: Duration) -> System {
    let until = Instant::now() + span;
    while Instant::now() < until {
        analysis.sync(&mut system);
        analysis.poll(&mut system);
        system = system.update(&[]);
        std::thread::sleep(Duration::from_millis(5));
    }
    system
}

#[test]
fn without_a_watcher_the_disk_is_compared_when_the_window_is_focused() {
    if analyzer().is_none() {
        return;
    }
    let mut rig = Rig::new("focus", |cache| {
        Analysis::new(cache, None).without_watcher()
    });
    rig.outside("code/util.luau", STRING_UTIL);
    let system = rig.system.take().unwrap();
    let system = idle_for(system, &mut rig.analysis, Duration::from_millis(1000));
    rig.system = Some(system);
    assert!(
        !rig.main_errors().is_empty(),
        "nothing should have noticed yet"
    );

    rig.analysis.window_focused();
    rig.fixed();

    // Deleting a script is noticed the same way.
    std::fs::remove_file(rig.path("code/util.luau")).unwrap();
    rig.analysis.window_focused();
    rig.wait(|s| {
        findings_of(s, "code/main.luau")
            .iter()
            .any(|(_, m)| m.contains("Unknown require"))
    });
}

#[test]
fn a_watched_cartridge_ignores_window_focus() {
    if analyzer().is_none() {
        return;
    }
    let mut rig = Rig::new("watched", |cache| Analysis::new(cache, None));
    let before = rig.analysis.lsp_stats().unwrap();
    rig.analysis.window_focused();
    let system = rig.system.take().unwrap();
    rig.system = Some(idle_for(
        system,
        &mut rig.analysis,
        Duration::from_millis(300),
    ));
    let moved = rig.analysis.lsp_stats().unwrap().since(before);
    assert_eq!(moved.notifications_sent, 0);
}
