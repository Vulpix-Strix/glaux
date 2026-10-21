//! A measurement of what logging costs a running cartridge, with the log
//! panel closed and open, at several rates of messages per tick. It asserts
//! nothing about time: it prints a table to read and compare.
//!
//! Run it in release mode, where the numbers mean something:
//!
//! ```text
//! cargo nextest run --release -p glaux --run-ignored only --no-capture log_cost
//! ```
//!
//! Times are per tick, in microseconds. Memory is the log store's own
//! estimate of what it retains. The process's real peak is read apart, by
//! running the same command under `/usr/bin/time -l` (macOS) or
//! `/usr/bin/time -v` (Linux) and comparing the maximum resident set size
//! between builds.
//!
//! To compare with a build without the log panel, check out the commit
//! before it in a `git worktree`, copy this file there, make `toggle_logs`
//! return its argument, drop the `retained_bytes` column and keep only the
//! `Closed` scenarios. The rest uses only what that commit already had.

use std::time::{Duration, Instant};

use super::rhai::shell_with_cartridge_root;
use super::*;
use crate::cartridge::fixture::write_rhai_cartridge;

const RATES: [usize; 5] = [0, 10, 100, 1_000, 10_000];
const TICKS: usize = 120;

/// Opens or closes the log panel.
fn toggle_logs(system: System) -> System {
    system.update(&[InputEvent::ToggleLogs])
}

#[derive(Clone, Copy, Debug)]
enum Panel {
    Closed,
    Following,
    ScrolledBack,
}

#[derive(Default)]
struct Samples(Vec<Duration>);

impl Samples {
    fn micros(&self, percentile: f64) -> f64 {
        let mut sorted = self.0.clone();
        sorted.sort_unstable();
        let at = ((sorted.len() - 1) as f64 * percentile).round() as usize;
        sorted[at].as_secs_f64() * 1e6
    }
}

fn luau(rate: usize) -> String {
    format!(
        "function tick() for i = 1, {rate} do log.info(\"message\", i, frame()) end end\n\
         function draw() cls(1) end\n"
    )
}

fn rhai(rate: usize) -> String {
    format!(
        "fn tick() {{ for i in 0..{rate} {{ log::info(`message ${{i}}`); }} }}\n\
         fn draw() {{ cls(1); }}\n"
    )
}

/// Runs `system` for [`TICKS`] ticks, updating and rendering each, and
/// returns the time each took with the system at the end.
fn measure(mut system: System) -> (Samples, Samples, System) {
    let rom = fake_rom();
    let (mut update, mut draw) = (Samples::default(), Samples::default());
    let mut frame = Framebuffer::new();
    for _ in 0..TICKS {
        let started = Instant::now();
        system = system.update(&[]);
        update.0.push(started.elapsed());
        let started = Instant::now();
        system.render(&mut frame, &rom, None);
        draw.0.push(started.elapsed());
    }
    (update, draw, system)
}

fn scenario(language: &str, rate: usize, panel: Panel) {
    let fixture = Fixture::new("sys-log-cost");
    let mut system = match language {
        "luau" => started(&fixture, &luau(rate)),
        _ => {
            write_rhai_cartridge(&fixture.root().join("pong"), "Pong", &rhai(rate));
            enter(
                enter(shell_with_cartridge_root(&fixture), "load pong"),
                "run",
            )
        }
    };
    assert!(is_running(&system));
    match panel {
        Panel::Closed => {}
        Panel::Following => system = toggle_logs(system),
        Panel::ScrolledBack => {
            system = ticks(toggle_logs(system), 5);
            system = system.update(&[InputEvent::ButtonDown(Button::Up)]);
        }
    }
    let (update, draw, system) = measure(system);
    println!(
        "{language:<5} {rate:>6}/tick {:<13} update p50 {:>9.1} p95 {:>9.1} max {:>9.1}   \
         render p50 {:>8.1} p95 {:>8.1}   retained {:>9} bytes",
        format!("{panel:?}"),
        update.micros(0.5),
        update.micros(0.95),
        update.micros(1.0),
        draw.micros(0.5),
        draw.micros(0.95),
        system.logs.retained_bytes(),
    );
}

#[test]
#[ignore = "a measurement to read, not a check"]
fn log_cost_by_rate_and_panel() {
    for language in ["luau", "rhai"] {
        for rate in RATES {
            for panel in [Panel::Closed, Panel::Following, Panel::ScrolledBack] {
                scenario(language, rate, panel);
            }
        }
    }
}
