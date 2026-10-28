use super::boot::BootStage;
use super::test_frames::assert_frame_eq;
use super::*;
use crate::cartridge::fixture::write_cartridge;
use crate::cartridge_fs::fixture::Fixture;
use glaux_input::{Button, Key, Point, PointerButton};
use glaux_machine::font::{BitmapFont, FontMetrics};
use glaux_machine::framebuffer::{Framebuffer, HEIGHT, WIDTH};
use glaux_machine::ink_image::InkImage;
use glaux_machine::palette::Palette;
use glaux_machine::psg::{
    Adsr, CHANNELS, ENVELOPE_FULL, EnvelopeStage, SAMPLES_PER_TICK, Waveform,
};
use glaux_machine::system_rom::{Cursor, SystemRom, SystemSfx, SystemUi};
use glaux_video::draw::{clear, draw_text};
use pretty_assertions::assert_eq;
use screen::*;
use semver::Version;
use shell::CartridgeSlot;
use workspace::Tab;

mod boot_sequence;
mod cartridge_audio;
mod code_editing;
mod code_navigation;
mod code_productivity;
mod code_run;
mod code_tabs;
mod credits_view;
mod debugging;
mod docs_browser;
mod docs_shell;
mod drawing;
mod input;
mod lifecycle;
mod log_perf;
mod menu_state;
mod new_cartridge;
mod pausing;
mod profiling_workload;
mod rhai;
mod rom_audio;
mod script_management;
mod shell_screen;
mod sprite_editor;
mod sprite_import;
mod unsaved_changes;
mod workspace_ui;

const CELL_W: i32 = 4;
const CELL_H: i32 = 5;

/// A ROM whose font draws every printable character as a 3×3 block in a
/// 4×5 cell.
fn fake_rom() -> SystemRom {
    let mut font = BitmapFont::new(FontMetrics::new(4, 5).unwrap());
    for ch in ' '..='~' {
        font.insert(ch, 1, &[0b1110, 0b1110, 0b1110, 0, 0]).unwrap();
    }
    SystemRom::new(
        Version::new(0, 1, 0),
        "(C) 2026 TEST".into(),
        Palette::default(),
        font,
    )
    .with_ui(fake_ui())
}

/// Icons framed in outline ink and filled with light ink, and a 2×2
/// pointer whose hotspot is its body pixel at (1, 1).
fn fake_ui() -> SystemUi {
    let icon = || {
        let pixels = (0..16 * 16)
            .map(|i| {
                let (x, y) = (i % 16, i / 16);
                if x == 0 || y == 0 || x == 15 || y == 15 {
                    1
                } else {
                    3
                }
            })
            .collect();
        InkImage::new(16, 16, pixels).unwrap()
    };
    let pointer = Cursor {
        frames: vec![InkImage::new(2, 2, vec![1, 0, 0, 2]).unwrap()],
        hotspot: (1, 1),
    };
    SystemUi::new(
        std::array::from_fn(|_| pointer.clone()),
        std::array::from_fn(|_| icon()),
    )
}

fn video_defaults() -> CartridgeVideoDefaults {
    CartridgeVideoDefaults::from_rom(&fake_rom())
}

fn info() -> SystemInfo {
    SystemInfo {
        rom_version: Version::new(0, 1, 0),
    }
}

fn cartridges() -> CartridgeStorage {
    CartridgeStorage::Unavailable
}

fn sounds() -> SystemSounds {
    crate::rom::sounds().unwrap()
}

fn new_system() -> System {
    System::new(
        info(),
        sounds(),
        video_defaults(),
        cartridges(),
        fake_rom().font(),
        fake_rom().palette().clone(),
    )
}

fn render(system: &System) -> Framebuffer {
    let mut fb = Framebuffer::new();
    system.render(&mut fb, &fake_rom(), None);
    fb
}

fn boot_at(stage: BootStage) -> System {
    let mut system = new_system();
    while !matches!(system.state(), SystemState::Boot { boot, .. } if boot.stage() == stage) {
        system = system.update(&[]);
    }
    system
}

fn shell() -> System {
    let mut system = new_system();
    while matches!(system.state(), SystemState::Boot { .. }) {
        system = system.update(&[]);
    }
    system
}

/// The shell of a system whose shell is the active UI.
fn shell_of(system: &System) -> &Shell {
    match system.state() {
        SystemState::Shell { shell, .. } => shell,
        other => panic!("expected the shell, got {other:?}"),
    }
}

/// Whether any pixel in text row `row` (in cells) differs from the
/// background.
fn row_has_ink(fb: &Framebuffer, row: i32) -> bool {
    let top = (row + 1) * CELL_H;
    (top..top + CELL_H).any(|y| (0..320).any(|x| fb.pixel(x, y) != Some(BACKGROUND)))
}

/// A system at the shell, navigating a cartridge root that holds a
/// `Pong` cartridge at `/pong` whose entry script is `script`.
fn shell_with_cartridge(fixture: &Fixture, script: &str) -> System {
    let dir = fixture.root().join("pong");
    write_cartridge(&dir, "Pong");
    std::fs::write(dir.join("code/main.luau"), script).unwrap();
    let mut system = System::new(
        info(),
        sounds(),
        video_defaults(),
        fixture.storage(),
        fake_rom().font(),
        fake_rom().palette().clone(),
    );
    while matches!(system.state(), SystemState::Boot { .. }) {
        system = system.update(&[]);
    }
    system
}

/// Types `line` and presses Enter within one tick.
fn enter(system: System, line: &str) -> System {
    let mut events: Vec<InputEvent> = line.chars().map(InputEvent::Text).collect();
    events.push(InputEvent::Enter);
    system.update(&events)
}

/// Loads and runs the `Pong` cartridge with `script` as its entry.
fn started(fixture: &Fixture, script: &str) -> System {
    let system = shell_with_cartridge(fixture, script);
    enter(enter(system, "load pong"), "run")
}

fn ticks(mut system: System, n: usize) -> System {
    for _ in 0..n {
        system = system.update(&[]);
    }
    system
}

fn press_break(system: System) -> System {
    system.update(&[InputEvent::Break])
}

/// Opens the shell from the pause screen.
fn open_shell(system: System) -> System {
    system.update(&[InputEvent::Enter])
}

/// Pauses the running cartridge and stops it from the shell.
fn stop_cartridge(system: System) -> System {
    enter(open_shell(press_break(system)), "stop")
}

fn is_running(system: &System) -> bool {
    matches!(system.state(), SystemState::Running { .. })
}

fn is_paused_game(system: &System) -> bool {
    matches!(
        system.state(),
        SystemState::Paused {
            view: PausedView::Game(_),
            ..
        }
    )
}

fn is_paused_shell(system: &System) -> bool {
    matches!(
        system.state(),
        SystemState::Paused {
            view: PausedView::Shell,
            ..
        }
    )
}

fn loaded_name(system: &System) -> Option<&str> {
    match system.state() {
        SystemState::Shell { loaded, .. } => loaded
            .as_ref()
            .map(|c| c.manifest().cartridge.name.as_str()),
        other => panic!("expected the shell, got {other:?}"),
    }
}

fn transcript(system: &System) -> Vec<String> {
    match system.state() {
        SystemState::Shell { shell, .. }
        | SystemState::Workspace { shell, .. }
        | SystemState::Running { shell, .. }
        | SystemState::Debugging { shell, .. }
        | SystemState::Paused { shell, .. } => shell.transcript(),
        other => panic!("expected a shell, got {other:?}"),
    }
}

/// What was added to the transcript after the line `from`.
fn after(system: &System, from: &str) -> Vec<String> {
    let lines = transcript(system);
    let start = lines.iter().rposition(|l| l == from).unwrap() + 1;
    lines[start..].to_vec()
}

/// Each channel's (waveform, frequency, volume) after `system` writes
/// its sound into `psg`.
///
/// `System::audio` remembers which source last wrote the PSG, so a test
/// must keep passing it the same PSG.
fn voices(system: &mut System, psg: &mut Psg) -> [(Waveform, u16, u8); CHANNELS] {
    system.audio(psg);
    std::array::from_fn(|index| {
        let channel = psg.channel(index).unwrap();
        (channel.waveform(), channel.frequency(), channel.volume())
    })
}

/// Each channel's gate after `system` writes its sound into `psg`.
fn gates(system: &mut System, psg: &mut Psg) -> [bool; CHANNELS] {
    system.audio(psg);
    std::array::from_fn(|index| psg.channel(index).unwrap().gate())
}

/// Logs a tick counter kept in the VM, and its entry and `boot()` runs.
const COUNTER: &str = r#"
        log.info("entry")
        function boot() log.info("boot") end
        local n = 0
        function tick() n += 1 log.info("tick", n) end
    "#;

fn assert_silent(psg: &mut Psg) {
    for index in 0..CHANNELS {
        let channel = psg.channel(index).unwrap();
        assert_eq!(channel.volume(), 0, "channel {index}");
        assert_eq!(
            channel.envelope_stage(),
            EnvelopeStage::Idle,
            "channel {index}"
        );
    }
    assert_eq!(render_psg(psg), vec![0; SAMPLES_PER_TICK]);
}

fn stage(psg: &Psg, index: usize) -> EnvelopeStage {
    psg.channel(index).unwrap().envelope_stage()
}

fn render_psg(psg: &mut Psg) -> Vec<i16> {
    let mut out = vec![0; SAMPLES_PER_TICK];
    psg.render(&mut out);
    out
}

/// Runs one tick per entry of `ticks` and returns what the cartridge
/// logged on each.
fn logged_per_tick(mut system: System, ticks: &[&[InputEvent]]) -> (System, Vec<String>) {
    let mut lines = Vec::new();
    for input in ticks {
        let before = transcript(&system).len();
        system = system.update(input);
        lines.extend(transcript(&system).split_off(before));
    }
    (system, lines)
}

const UP_STATE: &str = r#"function tick() log.info(btn(Button.Up), btnp(Button.Up)) end"#;

fn started_with_palette(fixture: &Fixture, script: &str, palette: &Palette) -> System {
    let system = shell_with_cartridge(fixture, script);
    std::fs::write(fixture.root().join("pong/palette.bin"), palette.to_rgb888()).unwrap();
    enter(enter(system, "load pong"), "run")
}

/// The fake ROM's palette in reverse order, so every system color sits
/// at a different index of the cartridge palette.
fn reversed_rom_palette() -> Palette {
    let mut entries = *fake_rom().palette().entries();
    entries.reverse();
    Palette::new(entries)
}

/// A framebuffer filled with `color`.
fn filled(color: u8) -> Framebuffer {
    let mut fb = Framebuffer::new();
    clear(&mut fb, color);
    fb
}

const SPACE_STATE: &str = r#"function tick() log.info(key(Key.Space), keyp(Key.Space)) end"#;

fn space_down() -> InputEvent {
    InputEvent::KeyDown(Key::Space)
}

fn down() -> InputEvent {
    InputEvent::ButtonDown(Button::Up)
}

fn up() -> InputEvent {
    InputEvent::ButtonUp(Button::Up)
}

fn is_workspace(system: &System) -> bool {
    matches!(system.state(), SystemState::Workspace { .. })
}

fn workspace_tab(system: &System) -> Tab {
    system.workspace.tab()
}

/// A system in the shell with the `Pong` cartridge loaded.
fn loaded(fixture: &Fixture) -> System {
    enter(shell_with_cartridge(fixture, ""), "load pong")
}

/// The point at the middle of `tab` in the workspace's tab bar.
fn tab_centre(tab: Tab) -> Point {
    let layout = workspace::tests_support::tab_rect(tab, fake_rom().font().metrics());
    Point::new(
        (layout.x + layout.width / 2) as u16,
        (layout.y + layout.height / 2) as u16,
    )
}

fn click(at: Point) -> [InputEvent; 2] {
    let button = PointerButton::Primary;
    [
        InputEvent::PointerDown {
            button,
            at,
            select: false,
        },
        InputEvent::PointerUp { button, at },
    ]
}

/// The title and buttons of the question about unsaved edits, while open.
fn unsaved_question(system: &System) -> Option<(String, Vec<String>)> {
    let dialog = system.unsaved_flow()?.dialog();
    Some((
        dialog.title().to_owned(),
        dialog.labels().into_iter().map(str::to_owned).collect(),
    ))
}

/// The question about unsaved edits that quitting or a shell command
/// asks, as it first opens.
fn asks_about_unsaved() -> Option<(String, Vec<String>)> {
    Some((
        "UNSAVED CHANGES".to_owned(),
        vec![
            "SAVE ALL".to_owned(),
            "DISCARD".to_owned(),
            "CANCEL".to_owned(),
        ],
    ))
}
