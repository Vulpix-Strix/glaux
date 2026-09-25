//! Boot sequence, it reveals the boot screen one line group at a time.

use glaux_machine::framebuffer::{HEIGHT, WIDTH};
use glaux_machine::system_rom::SystemRomVersion;

use super::{ACCENT, DIM, Line, OK, TEXT};

// Frame at which each stage starts. Frame 0 is the first frame presented.
pub(super) const VIDEO_FRAME: u32 = 20;
pub(super) const OBJ_FRAME: u32 = 40;
pub(super) const AUDIO_FRAME: u32 = 60;
pub(super) const READY_FRAME: u32 = 80;
/// Frame at which the boot hands over to the shell.
pub(super) const FINISH_FRAME: u32 = 140;

const OBJ_DETAIL: &str = "128 ENTRIES";
const AUDIO_DETAIL: &str = "4CH PSG";

/// Column where the `OK` of every status line starts.
const STATUS_OK_COLUMN: usize = 36;

/// A visible step of the boot screen. Later stages include everything shown
/// by earlier ones.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum BootStage {
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
    #[cfg(test)]
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
            _ => BootStage::Header,
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

/// Boot screen text for `stage`.
pub(super) fn lines(stage: BootStage, version: SystemRomVersion) -> Vec<Line> {
    let mut lines = vec![
        Line::new(0, 0, format!("GLAUX G1  SYSTEM ROM {version}"), TEXT),
        Line::new(0, 1, "(C) 2026 CUERVOLU", DIM),
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
    use pretty_assertions::assert_eq;

    const V0_1: SystemRomVersion = SystemRomVersion::new(0, 1);

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
        let lines = lines(BootStage::Header, V0_1);
        assert_eq!(
            texts(&lines),
            ["GLAUX G1  SYSTEM ROM 0.1", "(C) 2026 CUERVOLU"]
        );
    }

    #[test]
    fn header_uses_rom_version() {
        let lines = lines(BootStage::Header, SystemRomVersion::new(3, 7));
        assert_eq!(lines[0].text, "GLAUX G1  SYSTEM ROM 3.7");
    }

    #[test]
    fn ready_stage_shows_all_checks_in_order() {
        let lines = lines(BootStage::Ready, V0_1);
        assert_eq!(row_text(&lines, 3), status_line("VIDEO", "320x240 INDEXED"));
        assert_eq!(row_text(&lines, 4), status_line("OBJ", "128 ENTRIES"));
        assert_eq!(row_text(&lines, 5), status_line("AUDIO", "4CH PSG"));
        assert_eq!(row_text(&lines, 7), "READY.");

        let rows: Vec<i32> = lines.iter().map(|line| line.row).collect();
        assert!(rows.is_sorted());
    }

    #[test]
    fn each_stage_adds_its_line() {
        let count = |stage| lines(stage, V0_1).len();
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
