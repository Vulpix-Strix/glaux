//! System software: the boot sequence and the shell that follows it.
//!
//! The system advances one tick per host frame and redraws the whole
//! framebuffer from its current state on every frame.

mod boot;
mod shell;

pub use boot::Boot;
pub use shell::Shell;

use glaux_machine::framebuffer::Framebuffer;
use glaux_machine::system_rom::SystemRom;
use glaux_video::draw::{clear, draw_text};

// System UI colors, as indices into the system palette (Zughy 32).
const BACKGROUND: u8 = 11;
const TEXT: u8 = 16;
const DIM: u8 = 14;
const OK: u8 = 8;
const ACCENT: u8 = 18;

/// Where the system is in its own navigation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SystemState {
    Boot(Boot),
    Shell(Shell),
}

/// The system UI state machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct System {
    state: SystemState,
}

impl System {
    /// Starts at frame 0 of the boot sequence.
    pub fn new() -> Self {
        Self {
            state: SystemState::Boot(Boot::new()),
        }
    }

    #[cfg(test)]
    pub fn state(&self) -> &SystemState {
        &self.state
    }

    /// Advances the system by one frame.
    pub fn update(&mut self) {
        match &mut self.state {
            SystemState::Boot(boot) => {
                boot.tick();
                if boot.is_finished() {
                    tracing::info!("boot finished, entering shell");
                    self.state = SystemState::Shell(Shell);
                }
            }
            SystemState::Shell(_) => {}
        }
    }

    /// Redraws the whole framebuffer for the current state using the ROM's
    /// font.
    pub fn render(&self, framebuffer: &mut Framebuffer, rom: &SystemRom) {
        let lines = match &self.state {
            SystemState::Boot(boot) => boot::lines(boot.stage(), rom.version()),
            SystemState::Shell(_) => shell::lines(),
        };

        let font = rom.font();
        let metrics = font.metrics();
        let (cell_w, cell_h) = (
            i32::from(metrics.cell_width()),
            i32::from(metrics.cell_height()),
        );

        clear(framebuffer, BACKGROUND);
        for line in &lines {
            let (x, y) = line.origin(cell_w, cell_h);
            draw_text(framebuffer, font, &line.text, x, y, line.color);
        }
    }
}

impl Default for System {
    fn default() -> Self {
        Self::new()
    }
}

/// One line of system text, positioned in character cells.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Line {
    col: i32,
    row: i32,
    text: String,
    color: u8,
}

impl Line {
    fn new(col: i32, row: i32, text: impl Into<String>, color: u8) -> Self {
        Self {
            col,
            row,
            text: text.into(),
            color,
        }
    }

    /// Top-left pixel of the line, leaving a one-cell margin around the
    /// screen.
    fn origin(&self, cell_w: i32, cell_h: i32) -> (i32, i32) {
        ((self.col + 1) * cell_w, (self.row + 1) * cell_h)
    }
}

#[cfg(test)]
mod tests {
    use super::boot::BootStage;
    use super::*;
    use glaux_machine::font::{BitmapFont, FIRST_CHAR, FontMetrics, LAST_CHAR};
    use glaux_machine::palette::Palette;
    use pretty_assertions::assert_eq;
    use semver::Version;

    const CELL_W: i32 = 4;
    const CELL_H: i32 = 5;

    /// A ROM whose font draws every printable character as a 3×3 block in a
    /// 4×5 cell.
    fn fake_rom() -> SystemRom {
        let mut font = BitmapFont::new(FontMetrics::new(3, 3, 4, 5).unwrap());
        for ch in FIRST_CHAR..=LAST_CHAR {
            font.insert(ch, &[0b111; 3]).unwrap();
        }
        SystemRom::new(Version::new(0, 1, 0), Palette::provisional(), font)
    }

    fn render(system: &System) -> Framebuffer {
        let mut fb = Framebuffer::new();
        system.render(&mut fb, &fake_rom());
        fb
    }

    fn boot_at(stage: BootStage) -> System {
        let mut system = System::new();
        while !matches!(system.state(), SystemState::Boot(b) if b.stage() == stage) {
            system.update();
        }
        system
    }

    fn shell() -> System {
        let mut system = System::new();
        while matches!(system.state(), SystemState::Boot(_)) {
            system.update();
        }
        system
    }

    /// Whether any pixel in text row `row` (in cells) differs from the
    /// background.
    fn row_has_ink(fb: &Framebuffer, row: i32) -> bool {
        let top = (row + 1) * CELL_H;
        (top..top + CELL_H).any(|y| (0..320).any(|x| fb.pixel(x, y) != Some(BACKGROUND)))
    }

    #[test]
    fn starts_at_boot_frame_zero() {
        match System::new().state() {
            SystemState::Boot(boot) => {
                assert_eq!(boot.frame(), 0);
                assert_eq!(boot.stage(), BootStage::Header);
            }
            other => panic!("expected boot, got {other:?}"),
        }
    }

    #[test]
    fn frame_n_is_reached_after_n_updates() {
        let mut system = System::new();
        for n in 1..=10 {
            system.update();
            assert_eq!(system.state(), &SystemState::Boot(Boot::at_frame(n)));
        }
    }

    #[test]
    fn boot_stages_advance_in_order_at_their_frames() {
        let mut system = System::new();
        let mut seen = vec![(0, BootStage::Header)];
        let mut frame = 0;

        while let SystemState::Boot(boot) = system.state() {
            let stage = boot.stage();
            if seen.last().map(|&(_, s)| s) != Some(stage) {
                seen.push((frame, stage));
            }
            system.update();
            frame += 1;
        }

        assert_eq!(
            seen,
            [
                (0, BootStage::Header),
                (boot::VIDEO_FRAME, BootStage::Video),
                (boot::OBJ_FRAME, BootStage::Obj),
                (boot::AUDIO_FRAME, BootStage::Audio),
                (boot::READY_FRAME, BootStage::Ready),
            ]
        );
        assert_eq!(frame, boot::FINISH_FRAME);
    }

    #[test]
    fn switches_to_shell_exactly_at_finish_frame() {
        let mut system = System::new();
        for _ in 0..boot::FINISH_FRAME - 1 {
            system.update();
        }
        assert!(matches!(system.state(), SystemState::Boot(_)));

        system.update();
        assert_eq!(system.state(), &SystemState::Shell(Shell));

        for _ in 0..100 {
            system.update();
        }
        assert_eq!(system.state(), &SystemState::Shell(Shell));
    }

    #[test]
    fn boot_header_renders_at_its_position() {
        let rom = fake_rom();
        let fb = render(&System::new());

        let mut expected = Framebuffer::new();
        clear(&mut expected, BACKGROUND);
        for line in boot::lines(BootStage::Header, rom.version()) {
            draw_text(
                &mut expected,
                rom.font(),
                &line.text,
                (line.col + 1) * CELL_W,
                (line.row + 1) * CELL_H,
                line.color,
            );
        }
        assert_eq!(fb, expected);
        assert!(row_has_ink(&fb, 0));
    }

    #[test]
    fn video_line_appears_at_video_stage() {
        let video_row = boot::lines(BootStage::Video, &Version::new(0, 1, 0))
            .into_iter()
            .find(|line| line.text.starts_with("VIDEO"))
            .unwrap()
            .row;

        assert!(!row_has_ink(
            &render(&boot_at(BootStage::Header)),
            video_row
        ));
        assert!(row_has_ink(&render(&boot_at(BootStage::Video)), video_row));
    }

    #[test]
    fn shell_renders_prompt() {
        let prompt_row = shell::lines()
            .into_iter()
            .find(|line| line.text == "> _")
            .unwrap()
            .row;
        assert!(row_has_ink(&render(&shell()), prompt_row));
    }

    #[test]
    fn render_redraws_from_scratch() {
        let rom = fake_rom();
        for system in [boot_at(BootStage::Audio), shell()] {
            let mut first = Framebuffer::new();
            let mut second = Framebuffer::new();
            second.clear(200);
            second.set_pixel(300, 230, 3);

            system.render(&mut first, &rom);
            system.render(&mut second, &rom);
            assert_eq!(first, second);
        }
    }
}
