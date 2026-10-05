//! A short power-on intro, then a diagnostic screen revealed
//! one line group at a time.
//!
//! Boot frames are machine ticks, so the sequence lasts the same wall-clock
//! time at any host refresh rate.

use glaux_machine::font::BitmapFont;
use glaux_machine::framebuffer::{Framebuffer, HEIGHT, WIDTH};
use glaux_machine::machine::{GENERATION, NAME};
use glaux_machine::system_rom::SystemRom;
use glaux_video::draw::{clear, draw_text_scaled, fill_rect};
use semver::Version;

use super::{ACCENT, BACKGROUND, DIM, Line, OK, TEXT, identity_line};

/// Ticks spent on the power-on intro before the diagnostic screen.
pub(super) const INTRO_FRAMES: u32 = 96;

// Frame at which each stage starts. Frame 0 is the first frame presented.
pub(super) const HEADER_FRAME: u32 = INTRO_FRAMES;
pub(super) const VIDEO_FRAME: u32 = HEADER_FRAME + 20;
pub(super) const OBJ_FRAME: u32 = HEADER_FRAME + 40;
pub(super) const AUDIO_FRAME: u32 = HEADER_FRAME + 60;
pub(super) const READY_FRAME: u32 = HEADER_FRAME + 80;
/// Frame at which the boot hands over to the shell.
pub(super) const FINISH_FRAME: u32 = HEADER_FRAME + 140;

// Intro timeline, in frames from the start of the boot. The intro is one
// stage; these only shape what it draws.
const BEAM_START: u32 = 2;
const BAND_START: u32 = 8;
const GLOW_START: u32 = 14;
const LOGO_START: u32 = 22;
const LOGO_FADE_OUT: u32 = 78;
const LOGO_END: u32 = 86;

/// Frames each step of a color ramp is held.
const RAMP_STEP_FRAMES: u32 = 2;

/// Bright-to-dark steps of the screen glow after the tube opens.
const GLOW_RAMP: [u8; 4] = [14, 13, 12, BACKGROUND];

// Dark-to-bright steps of each intro element; the last entry is the color
// held once the element is fully lit.
const LOGO_RAMP: [u8; 5] = [12, 13, 14, 15, TEXT];
const GENERATION_RAMP: [u8; 5] = [12, 20, 19, ACCENT, ACCENT];
const SUBTITLE_RAMP: [u8; 5] = [BACKGROUND, 12, 13, DIM, DIM];

const BEAM_COLOR: u8 = TEXT;
const BAND_COLOR: u8 = 15;

const LOGO_SCALE: i32 = 5;
const GENERATION_SCALE: i32 = 2;
const LOGO_TOP: i32 = 70;

const OBJ_DETAIL: &str = "128 ENTRIES";
const AUDIO_DETAIL: &str = "4CH PSG";

/// Column where the `OK` of every status line starts.
const STATUS_OK_COLUMN: usize = 36;

/// A visible step of the boot screen. Later stages include everything shown
/// by earlier ones.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum BootStage {
    Intro,
    Header,
    Video,
    Obj,
    Audio,
    Ready,
}

/// Progress through the boot sequence, counted in frames.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Boot {
    frame: u32,
}

impl Boot {
    pub fn new() -> Self {
        Self { frame: 0 }
    }

    #[cfg(test)]
    pub(super) fn at_frame(frame: u32) -> Self {
        Self { frame }
    }

    /// Frames elapsed since the boot started.
    pub fn frame(&self) -> u32 {
        self.frame
    }

    pub fn tick(&mut self) {
        self.frame = self.frame.saturating_add(1);
    }

    pub fn stage(&self) -> BootStage {
        match self.frame {
            f if f >= READY_FRAME => BootStage::Ready,
            f if f >= AUDIO_FRAME => BootStage::Audio,
            f if f >= OBJ_FRAME => BootStage::Obj,
            f if f >= VIDEO_FRAME => BootStage::Video,
            f if f >= HEADER_FRAME => BootStage::Header,
            _ => BootStage::Intro,
        }
    }

    /// Whether the boot has run long enough to hand over to the shell.
    pub fn is_finished(&self) -> bool {
        self.frame >= FINISH_FRAME
    }
}

impl Default for Boot {
    fn default() -> Self {
        Self::new()
    }
}

/// Boot screen text for `stage`, identifying `rom`.
pub(super) fn lines(stage: BootStage, rom: &SystemRom) -> Vec<Line> {
    let mut lines = vec![
        Line::new(0, 0, identity_line(rom.version()), TEXT),
        Line::new(0, 1, rom.copyright(), DIM),
    ];

    let video = format!("{WIDTH}x{HEIGHT} INDEXED");
    let checks = [
        (BootStage::Video, "VIDEO", video.as_str()),
        (BootStage::Obj, "OBJ", OBJ_DETAIL),
        (BootStage::Audio, "AUDIO", AUDIO_DETAIL),
    ];
    for (row, (shown_at, label, detail)) in (3..).zip(checks) {
        if stage >= shown_at {
            let text = status_line(label, detail);
            let dots_end = text.len() - "OK".len();
            lines.push(Line::new(0, row, &text[..dots_end], TEXT));
            lines.push(Line::new(dots_end as i32, row, "OK", OK));
        }
    }

    if stage >= BootStage::Ready {
        lines.push(Line::new(0, 7, "READY.", ACCENT));
    }
    lines
}

/// What the intro shows on one frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IntroView {
    /// The tube is off.
    Dark,
    /// A one-pixel horizontal beam across the middle, `width` pixels wide.
    Beam { width: i32 },
    /// The beam opened into a centered band `height` pixels tall.
    Band { height: i32 },
    /// The whole screen glowing in one palette index.
    Glow(u8),
    /// The identity screen, lit to `step` of the intro color ramps.
    Identity { step: usize },
}

/// The intro view for boot frame `frame`; every frame from
/// [`LOGO_END`] on is dark, ready for the diagnostic screen.
fn intro_view(frame: u32) -> IntroView {
    // `steps` frames spread evenly from 1/steps to the full `extent`.
    let grow = |since: u32, steps: u32, extent: usize| {
        (extent as u32 * (since + 1) / steps).min(extent as u32) as i32
    };
    let ramp_step = |since: u32| (since / RAMP_STEP_FRAMES) as usize;
    let lit = LOGO_RAMP.len() - 1;
    match frame {
        f if f < BEAM_START => IntroView::Dark,
        f if f < BAND_START => IntroView::Beam {
            width: grow(f - BEAM_START, BAND_START - BEAM_START, WIDTH),
        },
        f if f < GLOW_START => IntroView::Band {
            height: grow(f - BAND_START, GLOW_START - BAND_START, HEIGHT),
        },
        f if f < LOGO_START => IntroView::Glow(GLOW_RAMP[ramp_step(f - GLOW_START).min(3)]),
        f if f < LOGO_FADE_OUT => IntroView::Identity {
            step: ramp_step(f - LOGO_START).min(lit),
        },
        f if f < LOGO_END => IntroView::Identity {
            step: lit.saturating_sub(ramp_step(f - LOGO_FADE_OUT) + 1),
        },
        _ => IntroView::Dark,
    }
}

/// Draws boot frame `frame` of the power-on intro.
pub(super) fn render_intro(
    framebuffer: &mut Framebuffer,
    font: &BitmapFont,
    version: &Version,
    frame: u32,
) {
    clear(framebuffer, BACKGROUND);
    let (width, height) = (WIDTH as i32, HEIGHT as i32);
    match intro_view(frame) {
        IntroView::Dark => {}
        IntroView::Beam { width: beam } => {
            fill_rect(
                framebuffer,
                (width - beam) / 2,
                height / 2,
                beam,
                1,
                BEAM_COLOR,
            );
        }
        IntroView::Band { height: band } => {
            fill_rect(framebuffer, 0, (height - band) / 2, width, band, BAND_COLOR);
        }
        IntroView::Glow(color) => clear(framebuffer, color),
        IntroView::Identity { step } => {
            let metrics = font.metrics();
            let line_height = i32::from(metrics.glyph_height());
            let mut y = LOGO_TOP;
            let rows = [
                (NAME.to_owned(), LOGO_SCALE, LOGO_RAMP[step]),
                (
                    GENERATION.to_owned(),
                    GENERATION_SCALE,
                    GENERATION_RAMP[step],
                ),
                (format!("SYSTEM ROM {version}"), 1, SUBTITLE_RAMP[step]),
            ];
            for (text, scale, color) in rows {
                let x = (width - text_width(font, &text, scale)) / 2;
                draw_text_scaled(framebuffer, font, &text, x, y, scale, color);
                y += (line_height + 4) * scale;
            }
        }
    }
}

/// Inked width of single-line `text` drawn at `scale`: whole cells, minus
/// the spacing after the last glyph.
fn text_width(font: &BitmapFont, text: &str, scale: i32) -> i32 {
    let metrics = font.metrics();
    let chars = i32::try_from(text.chars().count()).unwrap_or(i32::MAX);
    let cells = chars.saturating_mul(i32::from(metrics.cell_width()));
    let spacing = if chars > 0 {
        i32::from(metrics.cell_width() - metrics.glyph_width())
    } else {
        0
    };
    (cells - spacing).saturating_mul(scale)
}

/// `LABEL   detail ...... OK`, with `OK` always at [`STATUS_OK_COLUMN`].
fn status_line(label: &str, detail: &str) -> String {
    let mut line = format!("{label:<8}{detail} ");
    let dots = STATUS_OK_COLUMN.saturating_sub(line.len() + 1).max(1);
    line.push_str(&".".repeat(dots));
    line.push_str(" OK");
    line
}

#[cfg(test)]
mod tests {
    use super::*;
    use glaux_machine::font::{FIRST_CHAR, FontMetrics, LAST_CHAR};
    use pretty_assertions::assert_eq;

    /// 3×3 solid glyphs in 4×5 cells for every printable character.
    fn block_font() -> BitmapFont {
        let mut font = BitmapFont::new(FontMetrics::new(3, 3, 4, 5).unwrap());
        for ch in FIRST_CHAR..=LAST_CHAR {
            font.insert(ch, &[0b111; 3]).unwrap();
        }
        font
    }

    fn intro(frame: u32) -> Framebuffer {
        let mut fb = Framebuffer::new();
        render_intro(&mut fb, &block_font(), &v0_1_0(), frame);
        fb
    }

    /// Pixels in the frame that are not the background.
    fn ink(fb: &Framebuffer) -> Vec<(i32, i32)> {
        (0..HEIGHT as i32)
            .flat_map(|y| (0..WIDTH as i32).map(move |x| (x, y)))
            .filter(|&(x, y)| fb.pixel(x, y) != Some(BACKGROUND))
            .collect()
    }

    #[test]
    fn intro_is_one_stage_before_the_header() {
        for frame in 0..HEADER_FRAME {
            assert_eq!(Boot::at_frame(frame).stage(), BootStage::Intro);
        }
        assert_eq!(Boot::at_frame(HEADER_FRAME).stage(), BootStage::Header);
    }

    #[test]
    fn intro_starts_dark() {
        assert_eq!(ink(&intro(0)), []);
    }

    #[test]
    fn beam_is_a_centered_line_that_widens() {
        let row = HEIGHT as i32 / 2;
        let mut previous = 0;
        for frame in BEAM_START..BAND_START {
            let ink = ink(&intro(frame));
            assert!(ink.iter().all(|&(_, y)| y == row), "frame {frame}");
            let (left, right) = (ink.first().unwrap().0, ink.last().unwrap().0);
            // Odd widths on the even screen sit one pixel left of center.
            let gap = (WIDTH as i32 - 1 - right) - left;
            assert!((0..=1).contains(&gap), "centered at frame {frame}");
            assert!(ink.len() > previous);
            previous = ink.len();
        }
        assert_eq!(previous, WIDTH, "the beam ends full width");
    }

    #[test]
    fn band_opens_to_full_height() {
        let rows = |frame| {
            let fb = intro(frame);
            (0..HEIGHT as i32)
                .filter(|&y| fb.pixel(0, y) == Some(BAND_COLOR))
                .count()
        };
        let opened: Vec<usize> = (BAND_START..GLOW_START).map(rows).collect();
        assert!(opened.is_sorted());
        assert_eq!(opened.last(), Some(&HEIGHT));
    }

    #[test]
    fn identity_is_centered_with_clear_margins() {
        let lit = intro(LOGO_START + 40);
        let ink = ink(&lit);
        assert!(!ink.is_empty());

        let (min_x, max_x) = (
            ink.iter().map(|p| p.0).min().unwrap(),
            ink.iter().map(|p| p.0).max().unwrap(),
        );
        let gap = (WIDTH as i32 - 1 - max_x) - min_x;
        assert!((0..=1).contains(&gap), "horizontally centered");
        assert!(ink.iter().all(|&(_, y)| y >= LOGO_TOP));
        assert!(min_x > 16 && ink.iter().all(|&(_, y)| y < HEIGHT as i32 - 16));

        // The logo row is drawn at LOGO_SCALE in its lit color.
        assert_eq!(lit.pixel(min_x, LOGO_TOP), Some(TEXT));
    }

    #[test]
    fn identity_fades_in_and_out_through_the_ramp() {
        let logo_color = |frame| {
            let fb = intro(frame);
            let (x, _) = ink(&fb)[0];
            fb.pixel(x, LOGO_TOP).unwrap()
        };
        let fade_in: Vec<u8> = (LOGO_START..LOGO_START + 10)
            .step_by(RAMP_STEP_FRAMES as usize)
            .map(logo_color)
            .collect();
        assert_eq!(fade_in, LOGO_RAMP);
        assert_eq!(logo_color(LOGO_FADE_OUT - 1), TEXT);
        assert_eq!(logo_color(LOGO_END - 1), LOGO_RAMP[0]);
    }

    #[test]
    fn intro_ends_dark_before_the_header() {
        for frame in LOGO_END..HEADER_FRAME {
            assert_eq!(ink(&intro(frame)), [], "frame {frame}");
        }
    }

    #[test]
    fn text_width_excludes_trailing_spacing() {
        let font = block_font();
        assert_eq!(text_width(&font, "", 3), 0);
        assert_eq!(text_width(&font, "A", 1), 3);
        assert_eq!(text_width(&font, "AB", 2), (4 + 3) * 2);
    }

    fn v0_1_0() -> Version {
        Version::new(0, 1, 0)
    }

    fn rom_with(version: Version, copyright: &str) -> SystemRom {
        SystemRom::new(
            version,
            copyright.into(),
            glaux_machine::palette::Palette::provisional(),
            block_font(),
        )
    }

    fn rom() -> SystemRom {
        rom_with(v0_1_0(), "(C) 2026 TEST")
    }

    fn texts(lines: &[Line]) -> Vec<&str> {
        lines.iter().map(|line| line.text.as_str()).collect()
    }

    /// Joins lines on the same row back into the text shown on screen.
    fn row_text(lines: &[Line], row: i32) -> String {
        let mut text = String::new();
        for line in lines.iter().filter(|line| line.row == row) {
            let col = line.col as usize;
            if text.len() < col {
                text.push_str(&" ".repeat(col - text.len()));
            }
            text.push_str(&line.text);
        }
        text
    }

    #[test]
    fn header_stage_shows_version_and_copyright_only() {
        let lines = lines(BootStage::Header, &rom());
        assert_eq!(
            texts(&lines),
            ["GLAUX G1  SYSTEM ROM 0.1.0", "(C) 2026 TEST"]
        );
    }

    #[test]
    fn header_uses_rom_version_and_copyright() {
        let lines = lines(
            BootStage::Header,
            &rom_with(Version::new(3, 7, 0), "(C) 1999 OWL"),
        );
        assert_eq!(
            texts(&lines),
            ["GLAUX G1  SYSTEM ROM 3.7.0", "(C) 1999 OWL"]
        );
    }

    #[test]
    fn ready_stage_shows_all_checks_in_order() {
        let lines = lines(BootStage::Ready, &rom());
        assert_eq!(row_text(&lines, 3), status_line("VIDEO", "320x240 INDEXED"));
        assert_eq!(row_text(&lines, 4), status_line("OBJ", "128 ENTRIES"));
        assert_eq!(row_text(&lines, 5), status_line("AUDIO", "4CH PSG"));
        assert_eq!(row_text(&lines, 7), "READY.");

        let rows: Vec<i32> = lines.iter().map(|line| line.row).collect();
        assert!(rows.is_sorted());
    }

    #[test]
    fn each_stage_adds_its_line() {
        let count = |stage| lines(stage, &rom()).len();
        assert!(count(BootStage::Header) < count(BootStage::Video));
        assert!(count(BootStage::Video) < count(BootStage::Obj));
        assert!(count(BootStage::Obj) < count(BootStage::Audio));
        assert!(count(BootStage::Audio) < count(BootStage::Ready));
    }

    #[test]
    fn status_lines_are_aligned() {
        let video = status_line("VIDEO", "320x240 INDEXED");
        let obj = status_line("OBJ", "128 ENTRIES");
        let audio = status_line("AUDIO", "4CH PSG");

        assert_eq!(video, "VIDEO   320x240 INDEXED ........... OK");
        assert_eq!(obj.len(), video.len());
        assert_eq!(audio.len(), video.len());
        for line in [&video, &obj, &audio] {
            assert!(line.ends_with(" OK"), "{line:?}");
        }
    }
}
