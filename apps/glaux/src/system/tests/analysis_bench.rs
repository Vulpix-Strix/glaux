//! Measurements of the analyzer on large cartridges.
//!
//! Ignored by default. Run with a real luau-lsp:
//!
//! ```text
//! LUAU_LSP=/path/to/luau-lsp cargo nextest run -p glaux analysis_bench \
//!     --run-ignored only --no-capture
//! ```
//!
//! `GLAUX_BENCH_SIZES` (default `10,100,500`) picks the module counts and
//! `GLAUX_BENCH_FILTER` keeps only the scenarios whose label contains it.
//!
//! A module counts as fresh only when what the server last said about it
//! describes the text it has now. The scenarios edit a module at one end of
//! a dependency graph and wait for the tabs at the other end to show the
//! result, so they measure correctness as well as speed.

use std::process::Command;
use std::time::{Duration, Instant};

use super::analysis::open_tab;
use super::*;
use crate::cartridge::source::SourcePath;
use crate::lsp::Analysis;
use crate::scratch_dir::ScratchDir;
use crate::system::Severity;
use crate::system::editing::history::EditKind;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Layout {
    Flat,
    Nested,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Shape {
    /// Each module requires the previous one, and `main` the last.
    Chain,
    /// Every module requires module 0.
    Fan,
}

const TABS: usize = 8;

fn module_name(layout: Layout, k: usize) -> String {
    match layout {
        Layout::Flat => format!("m{k}"),
        Layout::Nested => format!("g{}.m{k}", k % 10),
    }
}

fn module_path(layout: Layout, k: usize) -> String {
    match layout {
        Layout::Flat => format!("code/m{k}.luau"),
        Layout::Nested => format!("code/g{}/m{k}.luau", k % 10),
    }
}

fn source(layout: Layout, shape: Shape, size: usize, k: usize) -> String {
    let name = |i| module_name(layout, i);
    match (shape, k) {
        (_, 0) => "--!strict\nlocal M = {}\nfunction M.value() return 1 end\nreturn M\n".into(),
        (Shape::Chain, k) => format!(
            "--!strict\nlocal prev = require(\"{}\")\nlocal M = {{}}\nfunction M.value() return prev.value() end\nreturn M\n",
            name(k - 1)
        ),
        (Shape::Fan, _) => format!(
            "--!strict\nlocal hub = require(\"{}\")\nlocal n: number = hub.value()\nlocal M = {{}}\nfunction M.value() return n end\nreturn M\n",
            name(0)
        ),
    }
    .replace("{size}", &size.to_string())
}

fn main_source(layout: Layout, shape: Shape, size: usize) -> String {
    match shape {
        Shape::Chain => format!(
            "--!strict\nlocal last = require(\"{}\")\nlocal n: number = last.value()\nprint(n, 0, 0, 0)\n",
            module_name(layout, size - 1)
        ),
        Shape::Fan => "--!strict\nprint(0, 0, 0, 0)\n".into(),
    }
}

/// The tabs that depend on module 0 without being edited, `main` or leaves.
fn watched(layout: Layout, shape: Shape, size: usize) -> Vec<String> {
    match shape {
        Shape::Chain => std::iter::once("code/main.luau".to_owned())
            .chain(
                (1..size - 1)
                    .step_by(((size - 2) / (TABS - 1)).max(1))
                    .take(TABS - 1)
                    .map(|k| module_path(layout, k)),
            )
            .collect(),
        Shape::Fan => (1..size.min(TABS + 1))
            .map(|k| module_path(layout, k))
            .collect(),
    }
}

/// The tabs whose findings prove the edit travelled: the ones that should
/// show an error when module 0 returns a string.
fn probes(layout: Layout, shape: Shape, size: usize) -> Vec<String> {
    match shape {
        Shape::Chain => vec!["code/main.luau".to_owned()],
        Shape::Fan => watched(layout, shape, size),
    }
}

struct Scenario {
    layout: Layout,
    shape: Shape,
    size: usize,
}

impl Scenario {
    fn label(&self) -> String {
        format!(
            "{} {} {}",
            self.size,
            if self.layout == Layout::Flat {
                "flat"
            } else {
                "nested"
            },
            if self.shape == Shape::Chain {
                "chain"
            } else {
                "fan"
            },
        )
    }
}

/// Sets what module 0 returns, in its open tab.
fn set_value(system: &mut System, layout: Layout, value: &str) {
    let code = system.code.as_mut().unwrap();
    let path = SourcePath::parse(&module_path(layout, 0)).unwrap();
    let document = code.open_documents().find(|d| d.path() == &path).unwrap();
    let (id, text) = (document.id(), document.text().to_owned());
    let start = text.find("return ").unwrap() + "return ".len();
    let end = text.find(" end").unwrap();
    code.edit(id, start..end, value, EditKind::Typing, None)
        .unwrap()
        .unwrap();
}

fn findings(system: &System, path: &str) -> Option<(usize, bool)> {
    let code = system.code.as_ref()?;
    let path = SourcePath::parse(path).ok()?;
    let document = code.open_documents().find(|d| d.path() == &path)?;
    let summary = code.diagnostic_summary(document.id())?;
    let errors = code
        .diagnostics(document.id())
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .count();
    Some((errors, summary.stale))
}

/// Every probe shows at least one error (`broken`) or none, from a result
/// that describes its current text.
fn probes_show(system: &System, probes: &[String], broken: bool) -> bool {
    probes.iter().all(|path| match findings(system, path) {
        Some((errors, stale)) => !stale && (errors > 0) == broken,
        None => false,
    })
}

fn all_fresh(system: &System, tabs: &[String]) -> bool {
    tabs.iter()
        .all(|path| findings(system, path).is_some_and(|(_, stale)| !stale))
}

#[derive(Default)]
struct FrameCosts {
    frames: u32,
    total: Duration,
    max: Duration,
}

impl FrameCosts {
    fn mean(&self) -> Duration {
        self.total / self.frames.max(1)
    }
}

/// One frame: the analyzer's work as the application does it, then a tick.
fn frame(system: System, analysis: &mut Analysis, costs: &mut FrameCosts) -> System {
    let mut system = system;
    let began = Instant::now();
    analysis.sync(&mut system);
    analysis.poll(&mut system);
    let cost = began.elapsed();
    costs.frames += 1;
    costs.total += cost;
    costs.max = costs.max.max(cost);
    system.update(&[])
}

/// Runs frames until `done`, one millisecond apart. Returns the time it
/// took, or panics after `limit`, saying what the analyzer was doing.
fn pump(
    mut system: System,
    analysis: &mut Analysis,
    costs: &mut FrameCosts,
    limit: Duration,
    done: impl Fn(&System) -> bool,
) -> (System, Duration) {
    let began = Instant::now();
    let mut reported = 0;
    loop {
        system = frame(system, analysis, costs);
        if done(&system) {
            return (system, began.elapsed());
        }
        if began.elapsed().as_secs() / 3 > reported {
            reported += 1;
            eprintln!(
                "   ... waiting {:?}: {:?} {:?} main tab {:?}",
                began.elapsed(),
                analysis.status(),
                analysis.lsp_stats(),
                findings(&system, "code/main.luau")
            );
        }
        assert!(
            began.elapsed() < limit,
            "gave up after {limit:?}: status {:?}, stats {:?}",
            analysis.status(),
            analysis.lsp_stats()
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

/// Runs frames for `span`, as an editor does between keystrokes.
fn idle(
    mut system: System,
    analysis: &mut Analysis,
    costs: &mut FrameCosts,
    span: Duration,
) -> System {
    let began = Instant::now();
    while began.elapsed() < span {
        system = frame(system, analysis, costs);
        std::thread::sleep(Duration::from_millis(1));
    }
    system
}

/// Resident memory in MB and CPU time in ms of the server we started.
fn server_usage() -> Option<(f64, u64)> {
    let ours = std::process::id().to_string();
    let output = Command::new("ps")
        .args(["-axo", "pid=,ppid=,rss=,cputime="])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    text.lines().find_map(|line| {
        let mut fields = line.split_whitespace();
        let (_pid, ppid, rss, cpu) = (
            fields.next()?,
            fields.next()?,
            fields.next()?,
            fields.next()?,
        );
        (ppid == ours).then_some(())?;
        let seconds = cpu.split(':').try_fold(0.0_f64, |total, part| {
            Some(total * 60.0 + part.parse::<f64>().ok()?)
        })?;
        Some((rss.parse::<f64>().ok()? / 1024.0, (seconds * 1000.0) as u64))
    })
}

fn median(mut values: Vec<Duration>) -> Duration {
    values.sort();
    values[values.len() / 2]
}

fn ms(duration: Duration) -> String {
    format!("{:.1}", duration.as_secs_f64() * 1000.0)
}

fn run(scenario: &Scenario) {
    let Scenario {
        layout,
        shape,
        size,
    } = *scenario;
    let fixture = Fixture::new(&format!(
        "bench-{size}-{}",
        scenario.label().replace(' ', "-")
    ));
    let cache = ScratchDir::new(&format!(
        "bench-cache-{size}-{}",
        scenario.label().replace(' ', "-")
    ));
    let system = shell_with_cartridge(&fixture, &main_source(layout, shape, size));
    for k in 0..size {
        let file = fixture.root().join("pong").join(module_path(layout, k));
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, source(layout, shape, size, k)).unwrap();
    }
    let mut system = press_break(enter(system, "load pong"));
    let tabs = watched(layout, shape, size);
    let probes = probes(layout, shape, size);
    open_tab(&mut system, &module_path(layout, 0));
    for tab in &tabs {
        open_tab(&mut system, tab);
    }
    let mut analysis = Analysis::new(Some(cache.to_path_buf()), None);
    let mut costs = FrameCosts::default();
    let limit = Duration::from_secs(
        std::env::var("GLAUX_BENCH_LIMIT")
            .ok()
            .and_then(|seconds| seconds.parse().ok())
            .unwrap_or(120),
    );
    eprintln!("-- {}: cold start", scenario.label());

    // Cold start: from the first frame with the cartridge loaded to every
    // tab showing a result for its text.
    let (mut system, cold) = pump(system, &mut analysis, &mut costs, limit, |s| {
        all_fresh(s, &tabs) && probes_show(s, &probes, false)
    });
    let cold_max = costs.max;
    let (rss, cpu_cold) = server_usage().unwrap_or((0.0, 0));

    eprintln!(
        "-- {}: cold took {} ms, editing",
        scenario.label(),
        ms(cold)
    );
    // One edit at a time: module 0 starts returning a string, then a number.
    let mut broke = Vec::new();
    let mut fixed = Vec::new();
    let mut all = Vec::new();
    for _ in 0..5 {
        set_value(&mut system, layout, "\"a\"");
        let began = Instant::now();
        let (next, took) = pump(system, &mut analysis, &mut costs, limit, |s| {
            probes_show(s, &probes, true)
        });
        broke.push(took);
        let (next, _) = pump(next, &mut analysis, &mut costs, limit, |s| {
            all_fresh(s, &tabs)
        });
        all.push(began.elapsed());
        system = next;
        set_value(&mut system, layout, "1");
        let (next, took) = pump(system, &mut analysis, &mut costs, limit, |s| {
            probes_show(s, &probes, false)
        });
        fixed.push(took);
        system = next;
    }

    // Bursts of 20 edits that alternate and end on the broken state.
    let mut bursts = String::new();
    for gap in [10_u64, 50] {
        let before = analysis.lsp_stats().unwrap();
        let cpu_before = server_usage().map_or(0, |(_, cpu)| cpu);
        let mut burst_costs = FrameCosts::default();
        for n in 0..20 {
            let value = if n % 2 == 0 { "\"a\"" } else { "1" };
            // 20 edits, the last one (n = 19) leaves a number: finish broken.
            let value = if n == 19 { "\"a\"" } else { value };
            set_value(&mut system, layout, value);
            system = idle(
                system,
                &mut analysis,
                &mut burst_costs,
                Duration::from_millis(gap),
            );
        }
        let last = Instant::now();
        let (next, _) = pump(system, &mut analysis, &mut burst_costs, limit, |s| {
            probes_show(s, &probes, true)
        });
        system = next;
        let settle = last.elapsed();
        let stats = analysis.lsp_stats().unwrap().since(before);
        let cpu = server_usage().map_or(0, |(_, cpu)| cpu) - cpu_before;
        bursts += &format!(
            "\n    burst {gap:>2} ms: settle {:>7} ms | pulls {} answered {} discarded {} cancelled {} | notifications {} | server cpu {} ms | frame mean {} us max {} us",
            ms(settle),
            stats.pulls_sent,
            stats.pulls_answered,
            stats.pulls_discarded,
            stats.pulls_cancelled,
            stats.notifications_sent,
            cpu,
            burst_costs.mean().as_micros(),
            burst_costs.max.as_micros(),
        );
        // Back to the good state before the next burst.
        set_value(&mut system, layout, "1");
        let (next, _) = pump(system, &mut analysis, &mut costs, limit, |s| {
            probes_show(s, &probes, false)
        });
        system = next;
    }

    let (rss_end, cpu_end) = server_usage().unwrap_or((0.0, 0));
    eprintln!(
        "\n== {} ==\n    cold {} ms (cpu {} ms, rss {:.1} MB) | slowest main-thread call during cold {} us\n    edit: dependent shows it {} ms (median of 5), all tabs fresh {} ms, fix shown {} ms{bursts}\n    end: rss {:.1} MB, server cpu total {} ms | idle frame mean {} us",
        scenario.label(),
        ms(cold),
        cpu_cold,
        rss,
        cold_max.as_micros(),
        ms(median(broke)),
        ms(median(all)),
        ms(median(fixed)),
        rss_end,
        cpu_end,
        costs.mean().as_micros(),
    );
    drop(system);
    drop(fixture);
}

#[test]
#[ignore = "measures with a real luau-lsp, see the module docs"]
fn measure() {
    if std::env::var_os("LUAU_LSP").is_none() {
        eprintln!("set LUAU_LSP to a luau-lsp executable");
        return;
    }
    let sizes: Vec<usize> = std::env::var("GLAUX_BENCH_SIZES")
        .unwrap_or_else(|_| "10,100,500".into())
        .split(',')
        .filter_map(|size| size.trim().parse().ok())
        .collect();
    if std::env::var_os("GLAUX_BENCH_LOG").is_some() {
        let _ = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::DEBUG)
            .with_writer(std::io::stderr)
            .try_init();
    }
    let filter = std::env::var("GLAUX_BENCH_FILTER").unwrap_or_default();
    for size in sizes {
        for layout in [Layout::Flat, Layout::Nested] {
            for shape in [Shape::Chain, Shape::Fan] {
                let scenario = Scenario {
                    layout,
                    shape,
                    size,
                };
                if scenario.label().contains(&filter) {
                    run(&scenario);
                }
            }
        }
    }
}
