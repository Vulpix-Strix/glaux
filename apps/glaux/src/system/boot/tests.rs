use super::*;
use crate::system::test_frames::assert_frame_eq;
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

/// A frame with nothing but the background.
fn dark() -> Framebuffer {
    let mut fb = Framebuffer::new();
    fb.clear(BACKGROUND);
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
fn the_chime_is_cued_once_on_reaching_its_frame() {
    let mut boot = Boot::new();
    let mut cues = Vec::new();
    while !boot.is_finished() {
        if let Some(sfx) = boot.tick() {
            cues.push((boot.frame(), sfx));
        }
    }
    assert_eq!(cues, [(CHIME_FRAME, SystemSfx::Boot)]);
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
    assert_frame_eq!(intro(0), dark());
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
        assert_frame_eq!(intro(frame), dark(), "frame {frame}");
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

/// The diagnostic screen at `stage`, as `(row, text)` in drawing order.
fn screen(stage: BootStage) -> Vec<(i32, String)> {
    let lines = lines(stage, &rom());
    let mut rows: Vec<i32> = lines.iter().map(|line| line.row).collect();
    rows.dedup();
    rows.into_iter()
        .map(|row| (row, row_text(&lines, row)))
        .collect()
}

#[test]
fn each_stage_keeps_the_earlier_lines_and_adds_its_own() {
    let mut expected = vec![
        (0, "GLAUX G1  SYSTEM ROM 0.1.0".to_string()),
        (1, "(C) 2026 TEST".to_string()),
    ];
    assert_eq!(screen(BootStage::Header), expected);
    for (stage, row, text) in [
        (
            BootStage::Video,
            3,
            "VIDEO   320x240 INDEXED ........... OK",
        ),
        (BootStage::Obj, 4, "OBJ     128 ENTRIES ............... OK"),
        (
            BootStage::Audio,
            5,
            "AUDIO   4CH PSG ................... OK",
        ),
        (BootStage::Ready, 7, "READY."),
    ] {
        expected.push((row, text.to_string()));
        assert_eq!(screen(stage), expected, "{stage:?}");
    }
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
