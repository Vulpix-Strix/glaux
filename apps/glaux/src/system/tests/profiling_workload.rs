//! A scripted session for profiling the system with hotpath, driven through
//! the same `render` and `update` calls the application makes every frame,
//! with the real System ROM and no window. It asserts nothing about time: it
//! prints the wall time of each frame per phase and, with a hotpath feature,
//! the usual report. See the Profiling section of `AGENTS.md`.
//!
//! The scenario comes from `GLAUX_WORKLOAD`: `luau` (the default) and `rhai`
//! play Signal Lost for a minute of game time, `bg` scrolls three full
//! background layers for the same time, `obj` moves 128 overlapping objects
//! over them, `editor` edits its Luau
//! scripts, `analyzer` does the same with `luau-lsp` running beside it (it
//! needs `LUAU_LSP`), and `paced` is `editor` with the same short wait after
//! each frame but no analyzer, to tell what the analyzer costs from what the
//! wait does to the CPU. Run one scenario per process.
//!

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};

use super::*;
use glaux_machine::psg::Psg;

use crate::lsp::Analysis;
use crate::rom;
use crate::scratch_dir::ScratchDir;
use crate::system::AnalyzerStatus;

/// Ticks of game time the cartridge scenarios play, one minute at 60 Hz.
const GAME_TICKS: usize = 3600;

/// The time one frame may take at 60 Hz.
const BUDGET: Duration = Duration::from_nanos(16_666_667);

/// Wall time of the frames of each phase, in the order they ran.
#[derive(Default)]
struct Phases {
    names: Vec<&'static str>,
    frames: BTreeMap<&'static str, Vec<Duration>>,
}

impl Phases {
    fn record(&mut self, phase: &'static str, frame: Duration) {
        if !self.names.contains(&phase) {
            self.names.push(phase);
        }
        self.frames.entry(phase).or_default().push(frame);
    }

    fn report(&self) {
        eprintln!(
            "\n{:<14} {:>7} {:>9} {:>9} {:>9} {:>9} {:>6}",
            "phase", "frames", "p50 ms", "p95 ms", "p99 ms", "max ms", "> 16.67"
        );
        for name in &self.names {
            let mut sorted = self.frames[name].clone();
            sorted.sort_unstable();
            let at = |p: f64| sorted[((sorted.len() - 1) as f64 * p).round() as usize];
            let over = sorted.iter().filter(|frame| **frame > BUDGET).count();
            let ms = |d: Duration| d.as_secs_f64() * 1e3;
            eprintln!(
                "{name:<14} {:>7} {:>9.3} {:>9.3} {:>9.3} {:>9.3} {over:>6}",
                sorted.len(),
                ms(at(0.5)),
                ms(at(0.95)),
                ms(at(0.99)),
                ms(sorted[sorted.len() - 1]),
            );
        }
    }
}

/// One frame as the application makes it: draw the current state, advance
/// it with `input`, then hand the tick's audio to the PSG and render the
/// samples the audio device would play.
#[hotpath::measure(label = "workload::frame")]
fn step(
    system: System,
    input: &[InputEvent],
    pointer: Option<Point>,
    frame: &mut Framebuffer,
    rom: &SystemRom,
    psg: &mut Psg,
) -> System {
    system.render(frame, rom, pointer);
    let mut system = system.update(input);
    system.audio(psg);
    psg.render(&mut [0; SAMPLES_PER_TICK]);
    system
}

/// The session: the system, what it draws into and how long each frame took.
struct Session {
    system: Option<System>,
    frame: Framebuffer,
    rom: SystemRom,
    psg: Psg,
    pointer: Option<Point>,
    phases: Phases,
    analysis: Option<Analyzer>,
    paced: bool,
}

/// The analyzer a session drives the way the application does, once per
/// frame, and the cache it keeps its files in.
struct Analyzer {
    analysis: Analysis,
    _cache: ScratchDir,
}

/// Frames are not paced like a window's, so with the analyzer on each one is
/// followed by a short wait that lets the server's answers land between
/// edits. The wait is outside the time recorded for the frame.
const ANALYZER_PACE: Duration = Duration::from_millis(1);

impl Session {
    /// A system at the shell over a scratch cartridge root that holds a copy
    /// of the example `name`.
    fn new(fixture: &Fixture, name: &str) -> Self {
        let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
        copy_dir(&examples.join(name), &fixture.root().join(name));
        let rom = rom::load().unwrap();
        let mut system = System::new(
            SystemInfo {
                rom_version: rom.version().clone(),
            },
            rom.sounds().clone(),
            CartridgeVideoDefaults::from_rom(&rom),
            fixture.storage(),
            rom.font(),
            rom.palette().clone(),
        );
        while matches!(system.state(), SystemState::Boot { .. }) {
            system = system.update(&[]);
        }
        Self {
            system: Some(system),
            frame: Framebuffer::new(),
            rom,
            psg: Psg::new(),
            pointer: None,
            phases: Phases::default(),
            analysis: None,
            paced: false,
        }
    }

    /// Runs `luau-lsp` beside the system from now on.
    fn analyze(&mut self) {
        assert!(
            std::env::var_os("LUAU_LSP").is_some(),
            "the analyzer scenario needs LUAU_LSP set to a luau-lsp executable"
        );
        let cache = ScratchDir::new("workload-analyzer-cache");
        self.paced = true;
        self.analysis = Some(Analyzer {
            analysis: Analysis::new(Some(cache.to_path_buf()), None),
            _cache: cache,
        });
    }

    /// The analyzer's share of a frame.
    fn analyzer_frame(&mut self, system: &mut System) {
        if let Some(analyzer) = &mut self.analysis {
            analyzer.analysis.sync(system);
            analyzer.analysis.poll(system);
        }
    }

    /// Lets the server's answers land. Not part of any frame's time.
    fn pace(&self) {
        if self.paced {
            std::thread::sleep(ANALYZER_PACE);
        }
    }

    /// Runs frames, not counted in any phase, until the analyzer is ready
    /// and the open tab has its findings.
    fn wait_for_analyzer(&mut self) {
        let began = Instant::now();
        loop {
            self.setup(&[]);
            let analyzer = self.analysis.as_ref().unwrap();
            let system = self.system.as_ref().unwrap();
            let code = system.code.as_ref().unwrap();
            let shown = code
                .active_document()
                .and_then(|document| code.diagnostic_summary(document.id()))
                .is_some_and(|summary| !summary.stale);
            if *analyzer.analysis.status() == AnalyzerStatus::Ready && shown {
                eprintln!("analyzer ready after {:?}", began.elapsed());
                return;
            }
            assert!(
                began.elapsed() < Duration::from_secs(20),
                "the analyzer never became ready: {:?}",
                analyzer.analysis.status()
            );
        }
    }

    /// Runs `input` in one frame that counts toward `phase`.
    fn frame(&mut self, phase: &'static str, input: &[InputEvent]) {
        let system = self.system.take().unwrap();
        let started = Instant::now();
        let system = step(
            system,
            input,
            self.pointer,
            &mut self.frame,
            &self.rom,
            &mut self.psg,
        );
        let mut system = system;
        self.analyzer_frame(&mut system);
        self.phases.record(phase, started.elapsed());
        self.system = Some(system);
        self.pace();
    }

    /// Runs `input` in a frame outside the measured phases, as setup.
    fn setup(&mut self, input: &[InputEvent]) {
        let mut system = self.system.take().unwrap().update(input);
        self.analyzer_frame(&mut system);
        self.system = Some(system);
        self.pace();
    }

    fn typed(&mut self, phase: &'static str, text: &str) {
        for ch in text.chars() {
            let event = match ch {
                '\n' => InputEvent::Enter,
                ch => InputEvent::Text(ch),
            };
            self.frame(phase, &[event]);
        }
    }

    fn idle(&mut self, phase: &'static str, frames: usize) {
        for _ in 0..frames {
            self.frame(phase, &[]);
        }
    }

    fn load(&mut self, name: &str) {
        let system = self.system.take().unwrap();
        self.system = Some(enter(system, &format!("load {name}")));
    }
}

pub(super) fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

/// Plays the cartridge for a minute of game time with a fixed pattern of
/// buttons: walk right and left in turn, attack and jump at fixed intervals.
fn play(name: &str) {
    let fixture = Fixture::new("workload-play");
    let session = Session::new(&fixture, name);
    run(session, name);
}

/// A cartridge with three full, opaque background layers that scroll every
/// tick, so the compositor visits and writes every pixel of every layer.
const BACKGROUNDS: &str = "local n = 0
function boot()
  for tile = 1, 15 do memset(0x24000 + tile * 32, tile * 0x11, 32) end
  for layer = 0, 2 do
    for cell = 0, 4095 do
      poke16(0x34000 + layer * 0x2000 + cell * 2, (cell + layer) % 15 + 1)
    end
  end
  poke(0x3C000, 7)
end
function tick()
  n += 1
  for layer = 0, 2 do
    poke16(0x3C010 + layer * 16, n * (layer + 1))
    poke16(0x3C012 + layer * 16, n)
  end
end
function draw() end";

/// A cartridge with all 128 objects enabled over three background layers.
/// The objects are 32 by 32 pixels on a lattice spaced 20 by 30 pixels, so
/// neighbours overlap, with the priority taken from the index, and the lattice
/// moves every tick so objects keep crossing the edges of the screen.
const OBJECTS: &str = "local n = 0
function boot()
  for row = 0, 3 do
    for col = 0, 3 do
      local tile = row * 16 + col
      memset(0x2C000 + tile * 32, (tile % 15 + 1) * 0x11, 32)
    end
  end
  for tile = 1, 15 do memset(0x24000 + tile * 32, tile * 0x11, 32) end
  for layer = 0, 2 do
    for cell = 0, 4095 do
      poke16(0x34000 + layer * 0x2000 + cell * 2, (cell + layer) % 15 + 1)
    end
  end
  for i = 0, 127 do
    local entry = 0x3B000 + i * 8
    poke(entry + 6, i % 8 + (i % 4) * 32)
    poke(entry + 7, 0x0A)
  end
  poke(0x3C000, 15)
end
function tick()
  n += 1
  for i = 0, 127 do
    local entry = 0x3B000 + i * 8
    local x = (i % 16 * 20 + n * 3) % 350 - 30
    local y = (i // 16 * 30 + n * 2) % 270 - 30
    poke16(entry, bit32.band(x, 0xFFFF))
    poke16(entry + 2, bit32.band(y, 0xFFFF))
  end
end
function draw() end";

/// Plays [`OBJECTS`] for a minute of game time.
fn objects() {
    let fixture = Fixture::new("workload-objects");
    let session = Session::new(&fixture, "signal-lost");
    let root = fixture.root().join("objects");
    crate::cartridge::fixture::write_cartridge(&root, "objects");
    fs::write(root.join("code/main.luau"), OBJECTS).unwrap();
    run(session, "objects");
}

/// Plays [`BACKGROUNDS`] for a minute of game time.
fn backgrounds() {
    let fixture = Fixture::new("workload-backgrounds");
    let session = Session::new(&fixture, "signal-lost");
    let root = fixture.root().join("backgrounds");
    crate::cartridge::fixture::write_cartridge(&root, "backgrounds");
    fs::write(root.join("code/main.luau"), BACKGROUNDS).unwrap();
    run(session, "backgrounds");
}

/// Loads the cartridge `name` and plays it for a minute of game time.
fn run(mut session: Session, name: &str) {
    session.load(name);
    let system = session.system.take().unwrap();
    session.system = Some(enter(system, "run"));
    let system = session.system.as_ref().unwrap();
    assert!(
        is_running(system),
        "{name} is not running: {:?}",
        transcript(system).iter().rev().take(6).collect::<Vec<_>>()
    );

    session.frame("title", &[InputEvent::ButtonDown(Button::A)]);
    session.frame("title", &[InputEvent::ButtonUp(Button::A)]);
    for tick in 0..GAME_TICKS {
        let mut input = Vec::new();
        let phase = tick % 240;
        match phase {
            0 => input.push(InputEvent::ButtonDown(Button::Right)),
            120 => {
                input.push(InputEvent::ButtonUp(Button::Right));
                input.push(InputEvent::ButtonDown(Button::Left));
            }
            _ => {}
        }
        if tick % 30 == 0 {
            input.push(InputEvent::ButtonDown(Button::A));
        } else if tick % 30 == 2 {
            input.push(InputEvent::ButtonUp(Button::A));
        }
        if tick % 45 == 0 {
            input.push(InputEvent::ButtonDown(Button::B));
        } else if tick % 45 == 2 {
            input.push(InputEvent::ButtonUp(Button::B));
        }
        session.frame("play", &input);
    }
    let system = session.system.as_ref().unwrap();
    assert!(
        is_running(system) && !transcript(system).iter().any(|line| line.starts_with("! ")),
        "{name} stopped with an error: {:?}",
        transcript(system).iter().rev().take(3).collect::<Vec<_>>()
    );
    session.phases.report();
}

/// Moves around, types, completes, hovers and follows definitions in the
/// scripts of Signal Lost, without saving.
fn edit(analyzed: bool, paced: bool) {
    let fixture = Fixture::new("workload-edit");
    let mut session = Session::new(&fixture, "signal-lost");
    session.paced = paced || analyzed;
    if analyzed {
        session.analyze();
    }
    session.load("signal-lost");
    session.setup(&[InputEvent::Break]);
    if analyzed {
        session.wait_for_analyzer();
    }
    session.idle("editor idle", 240);

    let down = InputEvent::Down { select: false };
    let page_down = InputEvent::PageDown { select: false };
    let page_up = InputEvent::PageUp { select: false };
    let word_right = InputEvent::WordRight { select: false };

    for _ in 0..12 {
        session.frame("scroll", &[page_down]);
    }
    for _ in 0..12 {
        session.frame("scroll", &[page_up]);
    }
    for _ in 0..150 {
        session.frame("navigate", &[down]);
        session.frame("navigate", &[word_right]);
    }

    // Edits at the end of the entry script, with suggestions and signatures
    // asked for as someone typing would.
    session.frame("typing", &[InputEvent::DocumentEnd { select: false }]);
    for line in 0..12 {
        session.typed(
            "typing",
            &format!("\nlocal speed{line} = util.cl\nspr(1, 2, 3)\naudio.to"),
        );
        session.frame("complete", &[InputEvent::Complete]);
        session.frame("complete", &[InputEvent::Tab { reverse: false }]);
        session.frame("signature", &[InputEvent::SignatureHelp]);
        session.typed("typing", "\nlog.in");
        session.frame("complete", &[InputEvent::Tab { reverse: false }]);
    }
    session.idle("editor idle", 120);

    // The pointer rests on a grid of places in the text long enough for a
    // hover to open.
    for y in (20..HEIGHT as u16 - 12).step_by(24) {
        for x in (30..WIDTH as u16 - 30).step_by(60) {
            session.pointer = Some(Point::new(x, y));
            session.frame("hover", &[InputEvent::PointerMove(session.pointer)]);
            session.idle("hover", 31);
        }
    }
    session.pointer = None;

    // Follow whatever the caret is on and come back.
    session.frame("definition", &[InputEvent::DocumentStart { select: false }]);
    for _ in 0..60 {
        session.frame("definition", &[down]);
        session.frame("definition", &[word_right, word_right]);
        session.frame("definition", &[InputEvent::GoToDefinition]);
        session.frame("definition", &[InputEvent::Back]);
    }

    // Every module of the cartridge, each opened and scrolled through.
    for module in [
        "arena",
        "audio",
        "enemies",
        "particles",
        "pickups",
        "player",
        "projectiles",
        "sheet",
        "util",
    ] {
        let mut open = vec![InputEvent::OpenFile];
        open.extend(module.chars().map(InputEvent::Text));
        open.push(InputEvent::Enter);
        session.frame("open file", &open);
        for _ in 0..6 {
            session.frame("tabs", &[page_down]);
        }
    }
    for _ in 0..20 {
        session.frame("tabs", &[InputEvent::NextDocument]);
    }
    for _ in 0..40 {
        session.frame("undo", &[InputEvent::Undo]);
    }
    session.idle("editor idle", 120);
    if analyzed {
        // Another program rewrites a module the open tabs require.
        let util = fixture.root().join("signal-lost/code/util.luau");
        let text = fs::read_to_string(&util).unwrap();
        for round in 0..10 {
            fs::write(&util, format!("{text}\n-- rewritten {round}\n")).unwrap();
            session.idle("external", 60);
        }
    }
    session.phases.report();
}

#[test]
#[ignore = "a profiling workload, see the Profiling section of AGENTS.md"]
#[hotpath::main(functions_limit = 40)]
fn workload() {
    match std::env::var("GLAUX_WORKLOAD").as_deref() {
        Ok("editor") => edit(false, false),
        Ok("paced") => edit(false, true),
        Ok("analyzer") => edit(true, true),
        Ok("rhai") => play("signal-lost-rhai"),
        Ok("bg") => backgrounds(),
        Ok("obj") => objects(),
        Ok("luau") | Err(_) => play("signal-lost"),
        Ok(other) => {
            panic!(
                "unknown GLAUX_WORKLOAD {other:?}, expected luau, rhai, bg, obj, editor, paced or analyzer"
            )
        }
    }
}
