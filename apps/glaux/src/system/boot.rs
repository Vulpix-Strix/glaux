//! A short power-on intro, then a diagnostic screen revealed
//! one line group at a time.
//!
//! Boot frames are machine ticks, so the sequence lasts the same wall-clock
//! time at any host refresh rate.

use glaux_machine::font::BitmapFont;
use glaux_machine::framebuffer::{Framebuffer, HEIGHT, WIDTH};
use glaux_machine::machine::{GENERATION, NAME};
use glaux_machine::system_rom::{SystemRom, SystemSfx};
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
// stage. These only shape what it draws.
const BEAM_START: u32 = 2;
const BAND_START: u32 = 8;
const GLOW_START: u32 = 14;
const LOGO_START: u32 = 22;
const LOGO_FADE_OUT: u32 = 78;
const LOGO_END: u32 = 86;

/// Frame on which the power-on chime starts, as the logo begins to light.
pub(super) const CHIME_FRAME: u32 = LOGO_START;
const _: () = assert!(CHIME_FRAME > 0, "a tick can only cue a frame after 0");

/// Frames each step of a color ramp is held.
const RAMP_STEP_FRAMES: u32 = 2;

/// Bright-to-dark steps of the screen glow after the tube opens.
const GLOW_RAMP: [u8; 4] = [14, 13, 12, BACKGROUND];

// Dark-to-bright steps of each intro element. The last entry is the color
// held once the element is fully lit.
const LOGO_RAMP: [u8; 5] = [12, 13, 14, 15, TEXT];
const GENERATION_RAMP: [u8; 5] = [12, 20, 19, ACCENT, ACCENT];
const SUBTITLE_RAMP: [u8; 5] = [BACKGROUND, 12, 13, DIM, DIM];

const BEAM_COLOR: u8 = TEXT;
const BAND_COLOR: u8 = 15;

const LOGO_SCALE: i32 = 5;
const GENERATION_SCALE: i32 = 2;
const LOGO_TOP: i32 = 70;
/// Unscaled space between the inked rows of the identity screen.
const IDENTITY_ROW_GAP: i32 = 6;

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

    /// Advances one frame and returns the sound the frame it enters starts
    /// with, so the cue plays on the same tick its frame is reached.
    pub fn tick(&mut self) -> Option<SystemSfx> {
        self.frame = self.frame.saturating_add(1);
        (self.frame == CHIME_FRAME).then_some(SystemSfx::Boot)
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

/// The intro view for boot frame `frame`. Every frame from
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
            // Rows are placed by their ink rather than their cells, so the
            // identity looks the same whatever spacing the font's cells
            // leave around its glyphs.
            for (text, scale, color) in rows {
                let Some(ink) = ink_bounds(font, &text) else {
                    continue;
                };
                let x = (width - (ink.right - ink.left) * scale) / 2 - ink.left * scale;
                draw_text_scaled(
                    framebuffer,
                    font,
                    &text,
                    x,
                    y - ink.top * scale,
                    scale,
                    color,
                );
                y += (ink.bottom - ink.top + IDENTITY_ROW_GAP) * scale;
            }
        }
    }
}

/// Edges of the set pixels of some text, relative to where it is drawn.
/// `right` and `bottom` are exclusive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct InkBounds {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

/// Where single-line `text` sets pixels when drawn at scale 1 from the
/// origin, or `None` when it sets none.
fn ink_bounds(font: &BitmapFont, text: &str) -> Option<InkBounds> {
    let cell_w = i32::from(font.metrics().cell_width());
    let mut pen = 0;
    let mut bounds: Option<InkBounds> = None;
    for ch in text.chars() {
        if let Some(glyph) = font.resolve(ch).and_then(|id| font.glyph(id)) {
            let height = glyph.rows().len();
            let set = (0..height)
                .flat_map(|y| (0..usize::from(glyph.width())).map(move |x| (x, y)))
                .filter(|&(x, y)| glyph.is_set(x, y));
            for (x, y) in set {
                let (x, y) = (pen + x as i32, y as i32);
                bounds = Some(match bounds {
                    None => InkBounds {
                        left: x,
                        top: y,
                        right: x + 1,
                        bottom: y + 1,
                    },
                    Some(b) => InkBounds {
                        left: b.left.min(x),
                        top: b.top.min(y),
                        right: b.right.max(x + 1),
                        bottom: b.bottom.max(y + 1),
                    },
                });
            }
        }
        pen += font.cells(ch) as i32 * cell_w;
    }
    bounds
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
mod tests;
