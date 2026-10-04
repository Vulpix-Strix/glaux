//! System software: the boot sequence and the shell that follows it.
//!
//! The system advances one tick per host frame and redraws the whole
//! framebuffer from its current state on every frame.

mod boot;
mod line_editor;
mod shell;

pub use boot::Boot;
pub use shell::Shell;

use glaux_input::InputEvent;
use glaux_machine::framebuffer::{Framebuffer, WIDTH};
use glaux_machine::system_rom::SystemRom;
use glaux_video::draw::{clear, draw_text, fill_rect};

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

    /// Advances the system by one frame, applying the frame's `input`.
    ///
    /// Input is ignored during the boot sequence.
    pub fn update(&mut self, input: &[InputEvent]) {
        match &mut self.state {
            SystemState::Boot(boot) => {
                boot.tick();
                if boot.is_finished() {
                    tracing::info!("boot finished, entering shell");
                    self.state = SystemState::Shell(Shell::new());
                }
            }
            SystemState::Shell(shell) => {
                for &event in input {
                    // Submitted lines are dropped until commands exist. They
                    // are deliberately not logged: user input stays out of
                    // application logs.
                    let _submitted = shell.handle_input(event);
                }
            }
        }
    }

    /// Redraws the whole framebuffer for the current state using the ROM's
    /// font.
    pub fn render(&self, framebuffer: &mut Framebuffer, rom: &SystemRom) {
        let font = rom.font();
        let metrics = font.metrics();
        let (cell_w, cell_h) = (
            i32::from(metrics.cell_width()),
            i32::from(metrics.cell_height()),
        );

        let lines = match &self.state {
            SystemState::Boot(boot) => boot::lines(boot.stage(), rom.version()),
            SystemState::Shell(shell) => shell.lines(text_columns(cell_w)),
        };

        clear(framebuffer, BACKGROUND);
        for line in &lines {
            let (x, y) = line.origin(cell_w, cell_h);
            if let Some(background) = line.background {
                let cells = i32::try_from(line.text.chars().count()).unwrap_or(i32::MAX);
                fill_rect(
                    framebuffer,
                    x,
                    y,
                    cells.saturating_mul(cell_w),
                    cell_h,
                    background,
                );
            }
            draw_text(framebuffer, font, &line.text, x, y, line.color);
        }
    }
}

impl Default for System {
    fn default() -> Self {
        Self::new()
    }
}

/// Character cells that fit across the screen inside the margin left by
/// [`Line::origin`].
fn text_columns(cell_w: i32) -> usize {
    let cells = WIDTH as i32 / cell_w.max(1);
    usize::try_from(cells - 2).unwrap_or(0)
}

/// One line of system text, positioned in character cells.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Line {
    col: i32,
    row: i32,
    text: String,
    color: u8,
    /// Fills the cells covered by the text before drawing it.
    background: Option<u8>,
}

impl Line {
    fn new(col: i32, row: i32, text: impl Into<String>, color: u8) -> Self {
        Self {
            col,
            row,
            text: text.into(),
            color,
            background: None,
        }
    }

    fn with_background(mut self, color: u8) -> Self {
        self.background = Some(color);
        self
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
            system.update(&[]);
        }
        system
    }

    fn shell() -> System {
        let mut system = System::new();
        while matches!(system.state(), SystemState::Boot(_)) {
            system.update(&[]);
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
            system.update(&[]);
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
            system.update(&[]);
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
            system.update(&[]);
        }
        assert!(matches!(system.state(), SystemState::Boot(_)));

        system.update(&[]);
        assert_eq!(system.state(), &SystemState::Shell(Shell::new()));

        for _ in 0..100 {
            system.update(&[]);
        }
        assert_eq!(system.state(), &SystemState::Shell(Shell::new()));
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
        let prompt_row = Shell::new()
            .lines(text_columns(CELL_W))
            .into_iter()
            .find(|line| line.text == "> ")
            .unwrap()
            .row;
        assert!(row_has_ink(&render(&shell()), prompt_row));
    }

    #[test]
    fn input_during_boot_is_ignored() {
        let mut system = System::new();
        for n in 1..=10 {
            system.update(&[InputEvent::Text('A'), InputEvent::Enter]);
            assert_eq!(system.state(), &SystemState::Boot(Boot::at_frame(n)));
        }
    }

    #[test]
    fn typed_text_reaches_shell() {
        let mut system = shell();
        system.update(&[InputEvent::Text('H'), InputEvent::Text('I')]);
        system.update(&[InputEvent::Left { select: false }]);

        let mut expected = Shell::new();
        for event in [
            InputEvent::Text('H'),
            InputEvent::Text('I'),
            InputEvent::Left { select: false },
        ] {
            expected.handle_input(event);
        }
        assert_eq!(system.state(), &SystemState::Shell(expected));
    }

    #[test]
    fn typed_text_renders_after_prompt() {
        let mut system = shell();
        let empty = render(&system);
        system.update(&[InputEvent::Text('A')]);
        let typed = render(&system);

        // Cell columns 2 and 3 follow the two-char prompt. The cursor starts
        // in column 2; typing puts 'A' there and moves the cursor to 3.
        let ink = |fb: &Framebuffer, col: i32| fb.pixel((col + 1) * CELL_W, 2 * CELL_H);
        assert_eq!(
            (ink(&empty, 2), ink(&empty, 3)),
            (Some(TEXT), Some(BACKGROUND))
        );
        assert_eq!((ink(&typed, 2), ink(&typed, 3)), (Some(TEXT), Some(TEXT)));
    }

    #[test]
    fn enter_clears_command_line() {
        let mut system = shell();
        system.update(&[InputEvent::Text('A'), InputEvent::Enter]);
        assert_eq!(system.state(), &SystemState::Shell(Shell::new()));
        assert_eq!(render(&system), render(&shell()));
    }

    #[test]
    fn selected_cells_render_inverse() {
        let mut system = shell();
        system.update(&[
            InputEvent::Text('A'),
            InputEvent::Text('B'),
            InputEvent::Left { select: false },
            InputEvent::Right { select: true },
        ]);
        let fb = render(&system);

        // Fake glyphs leave column 3 of each 4-pixel cell empty.
        let y = 2 * CELL_H;
        let gap = |cell: i32| fb.pixel((cell + 1) * CELL_W + 3, y);
        assert_eq!(gap(2), Some(BACKGROUND), "unselected 'A'");
        assert_eq!(gap(3), Some(TEXT), "selected 'B'");
        assert_eq!(fb.pixel(4 * CELL_W, y), Some(BACKGROUND), "glyph of 'B'");
    }

    #[test]
    fn text_columns_leave_a_cell_of_margin_on_each_side() {
        assert_eq!(text_columns(CELL_W), 320 / 4 - 2);
        assert_eq!(text_columns(5), 62);
        assert_eq!(text_columns(400), 0);
    }

    #[test]
    fn long_command_line_wraps_onto_next_row() {
        let mut system = shell();
        let fits = text_columns(CELL_W) - 2;
        let mut input = vec![InputEvent::Text('o'); fits];
        system.update(&input);
        // The cursor sits in the first cell of row 2 once row 1 is full.
        assert!(row_has_ink(&render(&system), 2));

        // Glyph pixels of the first cell on row 2 (margin + row offset).
        let first_cell_of_row_2 = |fb: &Framebuffer| fb.pixel(CELL_W, 3 * CELL_H);
        input.truncate(1);
        system.update(&input);
        let fb = render(&system);
        assert_eq!(first_cell_of_row_2(&fb), Some(TEXT));
        assert!(row_has_ink(&fb, 2));
        // Nothing is drawn past the right margin.
        let right_margin = 320 - CELL_W;
        assert!((0..240).all(|y| (right_margin..320).all(|x| fb.pixel(x, y) == Some(BACKGROUND))));
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
