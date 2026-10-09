//! Builds the System ROM from the assets in the repository's `rom/` folder
//! and the sound effects defined here.
//!
//! UI images are pixel art whose source colors are mapped to ink slots, so
//! the system chooses their palette indices when it draws them.

use anyhow::{Context, bail, ensure};
use glaux_audio::sfx::{Sfx, SfxAction, SfxStep};
use glaux_audio::{Envelope, Tone, Waveform};
use glaux_machine::font::{BitmapFont, FIRST_CHAR, FontMetrics, LAST_CHAR};
use glaux_machine::ink_image::InkImage;
use glaux_machine::palette::{PALETTE_LEN, Palette, Rgb};
use glaux_machine::system_rom::{Cursor, SystemIcon, SystemRom, SystemSounds, SystemUi};
use png::{BitDepth, ColorType, Transformations};
use semver::Version;

/// A `rom/` asset's name for error messages, and its embedded bytes.
macro_rules! embed {
    ($path:literal) => {
        (
            concat!("rom/", $path),
            include_bytes!(concat!("../../../rom/", $path)),
        )
    };
}

const VERSION: &str = include_str!("../../../rom/version");
const COPYRIGHT: &str = include_str!("../../../rom/copyright");
const PALETTE_HEX: &str = include_str!("../../../rom/palette/system.hex");
const FONT_BDF: &str = include_str!("../../../rom/font/Tamzen5x9r.bdf");

/// The pointer, then the icons in `SystemIcon` order.
const POINTER_PNG: (&str, &[u8]) = embed!("ui/cursors/pointer.png");
const ICON_PNGS: [(&str, &[u8]); SystemIcon::COUNT] = [
    embed!("ui/icons/code.png"),
    embed!("ui/icons/sprites.png"),
    embed!("ui/icons/map.png"),
    embed!("ui/icons/sfx.png"),
    embed!("ui/icons/music.png"),
    embed!("ui/icons/eraser.png"),
    embed!("ui/icons/eyedropper.png"),
    embed!("ui/icons/fill.png"),
];

/// Side of a view icon, in pixels.
pub const ICON_SIZE: usize = 16;
/// The pointer is drawn at twice its native size in its source image.
const POINTER_SOURCE_SCALE: usize = 2;

/// Ink slots of the 1-bit Pixel Icons' colors: black outline and white
/// light. The pack has no shade, so slot 2 goes unused.
const ICON_INKS: [(u32, u8); 2] = [(0x000000, 1), (0xffffff, 3)];
/// Ink slots of the light Simple Pixel Cursors' colors: outline, body.
const CURSOR_INKS: [(u32, u8); 2] = [(0x2e222f, 1), (0xffffff, 2)];

/// Decodes the embedded System ROM assets.
pub fn load() -> anyhow::Result<SystemRom> {
    let rom = build(VERSION, COPYRIGHT, PALETTE_HEX, FONT_BDF)?;
    let metrics = rom.font().metrics();
    tracing::info!(rom_version = %rom.version(), "System ROM loaded");
    tracing::debug!(
        glyph_width = metrics.glyph_width(),
        glyph_height = metrics.glyph_height(),
        cell_width = metrics.cell_width(),
        cell_height = metrics.cell_height(),
        "system font metrics"
    );
    Ok(rom)
}

/// Builds a System ROM from the text of its assets. Errors name the asset
/// that failed to decode.
fn build(version: &str, copyright: &str, palette: &str, font: &str) -> anyhow::Result<SystemRom> {
    let version = Version::parse(version.trim())
        .with_context(|| format!("invalid rom/version: {:?}", version.trim()))?;
    let copyright = parse_copyright(copyright).context("invalid rom/copyright")?;
    let palette = parse_palette(palette).context("invalid rom/palette/system.hex")?;
    let font = parse_bdf(font, FontMetrics::SYSTEM).context("invalid rom/font/Tamzen5x9r.bdf")?;
    Ok(SystemRom::new(version, copyright, palette, font)
        .with_sounds(sounds()?)
        .with_ui(ui()?))
}

/// The System ROM UI images.
pub fn ui() -> anyhow::Result<SystemUi> {
    let (name, bytes) = POINTER_PNG;
    let pointer = parse_cursor(bytes, POINTER_SOURCE_SCALE, (0, 0))
        .with_context(|| format!("invalid {name}"))?;
    let mut icons = Vec::with_capacity(ICON_PNGS.len());
    for (name, bytes) in ICON_PNGS {
        icons.push(parse_icon(bytes).with_context(|| format!("invalid {name}"))?);
    }
    let icons = icons
        .try_into()
        .unwrap_or_else(|_| unreachable!("one image per icon"));
    Ok(SystemUi::new(pointer, icons))
}

/// Decodes a view icon: a [`ICON_SIZE`]-square image in the icon colors.
fn parse_icon(png: &[u8]) -> anyhow::Result<InkImage> {
    let (width, height, rgba) = decode_png(png)?;
    ensure!(
        (width, height) == (ICON_SIZE, ICON_SIZE),
        "expected {ICON_SIZE}x{ICON_SIZE} pixels, got {width}x{height}"
    );
    Ok(InkImage::new(
        width,
        height,
        inks(width, &rgba, &ICON_INKS)?,
    )?)
}

/// Decodes a cursor drawn at `scale` times its native size, with
/// `hotspot` in native pixels. Transparent edges are trimmed and the
/// hotspot moved with them.
fn parse_cursor(png: &[u8], scale: usize, hotspot: (usize, usize)) -> anyhow::Result<Cursor> {
    let (width, height, rgba) = decode_png(png)?;
    let (width, height, rgba) = downscale(width, height, &rgba, scale)?;
    let slots = inks(width, &rgba, &CURSOR_INKS)?;

    let opaque = |x: usize, y: usize| slots[y * width + x] != 0;
    let columns: Vec<usize> = (0..width)
        .filter(|&x| (0..height).any(|y| opaque(x, y)))
        .collect();
    let rows: Vec<usize> = (0..height)
        .filter(|&y| (0..width).any(|x| opaque(x, y)))
        .collect();
    let (Some(&left), Some(&right), Some(&top), Some(&bottom)) =
        (columns.first(), columns.last(), rows.first(), rows.last())
    else {
        bail!("the cursor is fully transparent");
    };
    ensure!(
        (left..=right).contains(&hotspot.0) && (top..=bottom).contains(&hotspot.1),
        "hotspot {hotspot:?} lies outside the visible cursor"
    );

    let trimmed = (top..=bottom)
        .flat_map(|y| (left..=right).map(move |x| (x, y)))
        .map(|(x, y)| slots[y * width + x])
        .collect();
    let image = InkImage::new(right - left + 1, bottom - top + 1, trimmed)?;
    let hotspot = (
        u8::try_from(hotspot.0 - left)?,
        u8::try_from(hotspot.1 - top)?,
    );
    Ok(Cursor { image, hotspot })
}

/// Shrinks an image drawn at `scale` times its size, requiring every
/// `scale`-square block to be one color so no detail is lost.
fn downscale(
    width: usize,
    height: usize,
    rgba: &[[u8; 4]],
    scale: usize,
) -> anyhow::Result<(usize, usize, Vec<[u8; 4]>)> {
    ensure!(
        scale > 0 && width.is_multiple_of(scale) && height.is_multiple_of(scale),
        "{width}x{height} pixels is not a multiple of the {scale}x scale"
    );
    let (native_w, native_h) = (width / scale, height / scale);
    let mut native = Vec::with_capacity(native_w * native_h);
    for y in 0..native_h {
        for x in 0..native_w {
            let color = rgba[y * scale * width + x * scale];
            for (dx, dy) in (0..scale).flat_map(|dy| (0..scale).map(move |dx| (dx, dy))) {
                let (sx, sy) = (x * scale + dx, y * scale + dy);
                ensure!(
                    rgba[sy * width + sx] == color,
                    "pixel ({sx}, {sy}) breaks the {scale}x{scale} block at ({x}, {y})"
                );
            }
            native.push(color);
        }
    }
    Ok((native_w, native_h, native))
}

/// Maps each pixel to the ink slot `table` gives its color. Fully
/// transparent pixels are slot 0; partial transparency and colors missing
/// from `table` are errors.
fn inks(width: usize, rgba: &[[u8; 4]], table: &[(u32, u8)]) -> anyhow::Result<Vec<u8>> {
    rgba.iter()
        .enumerate()
        .map(|(i, &[r, g, b, a])| {
            let (x, y) = (i % width, i / width);
            match a {
                0 => Ok(0),
                255 => {
                    let rgb = u32::from_be_bytes([0, r, g, b]);
                    table
                        .iter()
                        .find(|&&(color, _)| color == rgb)
                        .map(|&(_, slot)| slot)
                        .with_context(|| {
                            format!("pixel ({x}, {y}) has unexpected color #{rgb:06x}")
                        })
                }
                _ => bail!("pixel ({x}, {y}) is partly transparent (alpha {a})"),
            }
        })
        .collect()
}

/// Decodes a static PNG to its size and RGBA pixels.
fn decode_png(bytes: &[u8]) -> anyhow::Result<(usize, usize, Vec<[u8; 4]>)> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(
        Transformations::EXPAND | Transformations::STRIP_16 | Transformations::ALPHA,
    );
    let mut reader = decoder.read_info().context("failed to read PNG")?;
    let width = usize::try_from(reader.info().width)?;
    let height = usize::try_from(reader.info().height)?;
    let size = reader
        .output_buffer_size()
        .context("decoded PNG is too large")?;
    let mut decoded = vec![0; size];
    let info = reader
        .next_frame(&mut decoded)
        .context("failed to decode PNG")?;
    let decoded = &decoded[..info.buffer_size()];
    let rgba = match (info.color_type, info.bit_depth) {
        (ColorType::Rgba, BitDepth::Eight) => decoded.as_chunks::<4>().0.to_vec(),
        (ColorType::GrayscaleAlpha, BitDepth::Eight) => decoded
            .as_chunks::<2>()
            .0
            .iter()
            .map(|&[gray, alpha]| [gray, gray, gray, alpha])
            .collect(),
        (color_type, depth) => {
            bail!("unexpected decoded PNG layout {color_type:?} at {depth:?} bits")
        }
    };
    Ok((width, height, rgba))
}

/// The System ROM sound effects.
pub fn sounds() -> anyhow::Result<SystemSounds> {
    let boot = Sfx::new(boot_chime()).context("invalid boot chime")?;
    Ok(SystemSounds::new(boot))
}

/// The power-on chime, about half a second: a C major arpeggio over a
/// triangle bass, whose top voice leaps an octave by retriggering its gate.
fn boot_chime() -> Vec<SfxStep> {
    const BASS: usize = 2;
    const LEAD: usize = 0;
    const FIFTH: usize = 1;

    let bass_env = Envelope {
        attack_ms: 5,
        decay_ms: 300,
        sustain: 6,
        release_ms: 220,
    };
    let bell_env = Envelope {
        attack_ms: 2,
        decay_ms: 120,
        sustain: 5,
        release_ms: 200,
    };
    let note = |waveform, frequency, volume| Tone {
        waveform,
        frequency,
        volume,
    };
    let at = |tick, action| SfxStep { tick, action };
    let tone = |channel, tone| SfxAction::Tone { channel, tone };
    let envelope = |channel, envelope| SfxAction::Envelope { channel, envelope };
    let gate = |channel, open| SfxAction::Gate { channel, open };

    vec![
        at(0, envelope(BASS, bass_env)),
        at(0, tone(BASS, note(Waveform::Triangle, 131, 12))),
        at(0, gate(BASS, true)),
        at(0, envelope(LEAD, bell_env)),
        at(0, tone(LEAD, note(Waveform::Square, 523, 6))),
        at(0, gate(LEAD, true)),
        at(6, envelope(FIFTH, bell_env)),
        at(6, tone(FIFTH, note(Waveform::Square, 784, 5))),
        at(6, gate(FIFTH, true)),
        at(12, tone(LEAD, note(Waveform::Square, 1047, 7))),
        at(12, gate(LEAD, true)),
        at(20, gate(LEAD, false)),
        at(20, gate(FIFTH, false)),
        at(20, gate(BASS, false)),
    ]
}

/// Parses the copyright notice: one non-empty line of printable ASCII, so
/// the system font can draw every character of it.
fn parse_copyright(text: &str) -> anyhow::Result<String> {
    let notice = text.trim();
    ensure!(!notice.is_empty(), "the copyright notice is empty");
    if let Some(ch) = notice
        .chars()
        .find(|ch| !(FIRST_CHAR..=LAST_CHAR).contains(ch))
    {
        bail!("{ch:?} in {notice:?} is not printable ASCII");
    }
    Ok(notice.to_owned())
}

/// Parses one `rrggbb` color per non-empty line. Exactly 256 colors are
/// required.
fn parse_palette(text: &str) -> anyhow::Result<Palette> {
    let mut entries = [Rgb::from_hex(0); PALETTE_LEN];
    let mut count = 0;

    for (number, line) in text.lines().enumerate().map(|(i, l)| (i + 1, l.trim())) {
        if line.is_empty() {
            continue;
        }
        ensure!(
            line.len() == 6,
            "line {number}: expected a 6-digit hex color, got {line:?}"
        );
        let hex = u32::from_str_radix(line, 16)
            .with_context(|| format!("line {number}: {line:?} is not a hex color"))?;
        ensure!(
            count < PALETTE_LEN,
            "line {number}: more than {PALETTE_LEN} colors"
        );
        entries[count] = Rgb::from_hex(hex);
        count += 1;
    }

    ensure!(
        count == PALETTE_LEN,
        "expected {PALETTE_LEN} colors, found {count}"
    );
    Ok(Palette::new(entries))
}

/// Parses the subset of BDF used by fixed-cell fonts such as Tamzen.
///
/// Every glyph's bounding box must match the font's `glyph_width` ×
/// `glyph_height` with offsets `0 -2`; glyph offsets are not supported, so
/// any other box is rejected rather than placed incorrectly. Only glyphs in
/// printable ASCII are loaded, and all of them must be present.
fn parse_bdf(text: &str, metrics: FontMetrics) -> anyhow::Result<BitmapFont> {
    let (width, height) = (metrics.glyph_width(), metrics.glyph_height());
    let expected_box = format!("{width} {height} 0 -2");
    let mut font = BitmapFont::new(metrics);
    let mut loaded = 0;

    let mut lines = text.lines().enumerate().map(|(i, l)| (i + 1, l.trim()));
    let mut encoding: Option<i64> = None;
    let mut saw_font_box = false;

    while let Some((number, line)) = lines.next() {
        let (keyword, args) = line.split_once(' ').unwrap_or((line, ""));
        match keyword {
            "FONTBOUNDINGBOX" => {
                ensure!(
                    args == expected_box,
                    "line {number}: font bounding box is {args:?}, expected {expected_box:?}"
                );
                saw_font_box = true;
            }
            "ENCODING" => {
                let code = args
                    .split_whitespace()
                    .next()
                    .and_then(|code| code.parse().ok())
                    .with_context(|| format!("line {number}: invalid ENCODING {args:?}"))?;
                encoding = Some(code);
            }
            "BBX" => ensure!(
                args == expected_box,
                "line {number}: glyph bounding box is {args:?}, expected {expected_box:?}"
            ),
            "BITMAP" => {
                let code = encoding
                    .take()
                    .with_context(|| format!("line {number}: BITMAP without ENCODING"))?;

                let mut rows = Vec::with_capacity(usize::from(height));
                for _ in 0..height {
                    let (number, row) = lines
                        .next()
                        .with_context(|| format!("line {number}: truncated BITMAP"))?;
                    rows.push(
                        parse_bdf_row(row, width).with_context(|| {
                            format!("line {number}: invalid bitmap row {row:?}")
                        })?,
                    );
                }

                let ch = u32::try_from(code).ok().and_then(char::from_u32);
                if let Some(ch) = ch.filter(|ch| (FIRST_CHAR..=LAST_CHAR).contains(ch)) {
                    ensure!(
                        font.glyph(ch).is_none(),
                        "line {number}: duplicate glyph for {ch:?}"
                    );
                    font.insert(ch, &rows)
                        .with_context(|| format!("line {number}: glyph {ch:?}"))?;
                    loaded += 1;
                }
            }
            _ => {}
        }
    }

    ensure!(saw_font_box, "missing FONTBOUNDINGBOX");
    if loaded != (FIRST_CHAR..=LAST_CHAR).count() {
        let missing: String = (FIRST_CHAR..=LAST_CHAR)
            .filter(|&ch| font.glyph(ch).is_none())
            .collect();
        bail!("missing glyphs for {missing:?}");
    }
    Ok(font)
}

/// Converts one BDF bitmap row (a hex byte whose most significant bit is the
/// leftmost pixel) into a `width`-bit row.
fn parse_bdf_row(row: &str, width: u8) -> anyhow::Result<u8> {
    ensure!(row.len() == 2, "expected one hex byte");
    let byte = u8::from_str_radix(row, 16)?;
    let unused = 8 - u32::from(width);
    ensure!(
        u32::from(byte) & ((1 << unused) - 1) == 0,
        "pixels set beyond the {width}-pixel glyph width"
    );
    Ok((u32::from(byte) >> unused) as u8)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    fn tiny_bdf(glyphs: &str) -> String {
        format!("STARTFONT 2.1\nFONTBOUNDINGBOX 5 9 0 -2\nCHARS 1\n{glyphs}ENDFONT\n")
    }

    fn bdf_glyph(code: u32, rows: [&str; 9]) -> String {
        format!(
            "STARTCHAR c{code}\nENCODING {code}\nBBX 5 9 0 -2\nBITMAP\n{}\nENDCHAR\n",
            rows.join("\n")
        )
    }

    const BLANK: [&str; 9] = ["00"; 9];

    #[test]
    fn copyright_is_trimmed() {
        assert_eq!(
            parse_copyright("  (C) 2026 CUERVOLU \n").unwrap(),
            "(C) 2026 CUERVOLU"
        );
    }

    #[test]
    fn invalid_copyright_names_the_asset() {
        for (text, detail) in [
            (" \n", "empty"),
            ("(C) 2026 BÚHO", "'Ú'"),
            ("A\nB", "'\\n'"),
        ] {
            let err = build("0.1.0", text, PALETTE_HEX, FONT_BDF).unwrap_err();
            let message = format!("{err:#}");
            assert!(message.contains("rom/copyright"), "{message}");
            assert!(message.contains(detail), "{message}");
        }
    }

    #[test]
    fn invalid_version_names_the_asset() {
        let err = build("zero.one", COPYRIGHT, PALETTE_HEX, FONT_BDF).unwrap_err();
        let message = format!("{err:#}");
        assert!(message.contains("rom/version"), "{message}");
        assert!(message.contains("zero.one"), "{message}");
    }

    #[test]
    fn palette_requires_exactly_256_colors() {
        let full = "ff00ff\n".repeat(256);
        assert!(parse_palette(&full).is_ok());
        assert!(parse_palette(&"ff00ff\n".repeat(255)).is_err());
        assert!(parse_palette(&"ff00ff\n".repeat(257)).is_err());
    }

    #[test]
    fn palette_keeps_order_and_reports_bad_lines() {
        let text = format!("472d3c\n\n5e3643\n{}", "000000\n".repeat(254));
        let palette = parse_palette(&text).unwrap();
        assert_eq!(palette.get(0), Rgb::from_hex(0x472d3c));
        assert_eq!(palette.get(1), Rgb::from_hex(0x5e3643));

        let err = parse_palette("472d3c\nnope!!\n").unwrap_err();
        assert!(err.to_string().contains("line 2"), "{err:#}");
    }

    #[test]
    fn bdf_row_is_shifted_to_glyph_width() {
        assert_eq!(parse_bdf_row("F8", 5).unwrap(), 0b11111);
        assert_eq!(parse_bdf_row("90", 5).unwrap(), 0b10010);
        assert_eq!(parse_bdf_row("08", 5).unwrap(), 0b00001);
        assert!(parse_bdf_row("04", 5).is_err());
        assert!(parse_bdf_row("zz", 5).is_err());
    }

    #[test]
    fn bdf_rejects_unsupported_bounding_boxes() {
        let text = "FONTBOUNDINGBOX 6 12 0 -3\n";
        assert!(parse_bdf(text, FontMetrics::SYSTEM).is_err());

        let glyph = bdf_glyph(65, BLANK).replace("BBX 5 9 0 -2", "BBX 4 9 1 -2");
        assert!(parse_bdf(&tiny_bdf(&glyph), FontMetrics::SYSTEM).is_err());
    }

    #[test]
    fn bdf_reports_missing_ascii_glyphs() {
        let a = ["00", "00", "60", "90", "F0", "90", "90", "00", "00"];
        let err = parse_bdf(&tiny_bdf(&bdf_glyph(65, a)), FontMetrics::SYSTEM).unwrap_err();
        assert!(err.to_string().contains("missing glyphs"), "{err:#}");
    }

    #[test]
    fn bdf_loads_ascii_and_skips_other_encodings() {
        let mut glyphs: String = (FIRST_CHAR..=LAST_CHAR)
            .map(|ch| bdf_glyph(ch as u32, BLANK))
            .collect();
        glyphs.push_str(&bdf_glyph(0x2592, ["F8"; 9]));
        let font = parse_bdf(&tiny_bdf(&glyphs), FontMetrics::SYSTEM).unwrap();
        assert!((FIRST_CHAR..=LAST_CHAR).all(|ch| font.glyph(ch).is_some()));
    }

    #[test]
    fn bdf_rejects_duplicate_glyphs() {
        let glyphs = format!("{0}{0}", bdf_glyph(65, BLANK));
        let err = parse_bdf(&tiny_bdf(&glyphs), FontMetrics::SYSTEM).unwrap_err();
        assert!(err.to_string().contains("duplicate"), "{err:#}");
    }

    // Guards the embedded repository assets.
    #[test]
    fn embedded_rom_loads() {
        let rom = load().unwrap();

        assert_eq!(rom.version(), &Version::new(0, 1, 0));
        assert_eq!(rom.copyright(), "(C) 2026 CUERVOLU");

        assert_eq!(rom.palette(), &Palette::provisional());

        let font = rom.font();
        assert_eq!(font.metrics(), FontMetrics::SYSTEM);
        assert!((FIRST_CHAR..=LAST_CHAR).all(|ch| font.glyph(ch).is_some()));
        assert_eq!(
            font.glyph('A').unwrap().rows(),
            &[
                0b00000, 0b00000, 0b01100, 0b10010, 0b11110, 0b10010, 0b10010, 0b00000, 0b00000
            ]
        );
        assert_eq!(rom.sounds(), &sounds().unwrap());
        assert_eq!(rom.ui(), &ui().unwrap());
    }

    /// Encodes RGBA rows as a PNG.
    fn png(width: u32, pixels: &[[u8; 4]]) -> Vec<u8> {
        let height = pixels.len() as u32 / width;
        let mut out = Vec::new();
        let mut encoder = png::Encoder::new(&mut out, width, height);
        encoder.set_color(ColorType::Rgba);
        encoder.set_depth(BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(pixels.as_flattened()).unwrap();
        writer.finish().unwrap();
        out
    }

    const CLEAR: [u8; 4] = [0, 0, 0, 0];
    const EDGE: [u8; 4] = [0x2e, 0x22, 0x2f, 255];
    const BODY: [u8; 4] = [255, 255, 255, 255];

    /// `rows` of `.`, `o` (outline) and `#` (body), each pixel drawn as a
    /// `scale`-square block.
    fn cursor_png(rows: &[&str], scale: usize) -> Vec<u8> {
        let width = rows[0].len() * scale;
        let pixels: Vec<[u8; 4]> = rows
            .iter()
            .flat_map(|row| std::iter::repeat_n(row, scale))
            .flat_map(|row| {
                row.chars().flat_map(move |c| {
                    let color = match c {
                        'o' => EDGE,
                        '#' => BODY,
                        _ => CLEAR,
                    };
                    std::iter::repeat_n(color, scale)
                })
            })
            .collect();
        png(width as u32, &pixels)
    }

    #[test]
    fn cursors_are_downscaled_and_trimmed_with_their_hotspot() {
        let source = cursor_png(&["....", ".o..", ".o#o", "...."], 2);
        let cursor = parse_cursor(&source, 2, (1, 1)).unwrap();
        assert_eq!(
            cursor.image,
            InkImage::new(3, 2, vec![1, 0, 0, 1, 2, 1]).unwrap()
        );
        assert_eq!(cursor.hotspot, (0, 0));

        let cursor = parse_cursor(&source, 2, (2, 2)).unwrap();
        assert_eq!(cursor.hotspot, (1, 1));
        let err = parse_cursor(&source, 2, (0, 0)).unwrap_err();
        assert!(err.to_string().contains("hotspot"), "{err:#}");
    }

    #[test]
    fn cursors_must_be_whole_blocks_of_known_colors() {
        let mut pixels = vec![BODY; 4];
        pixels[3] = EDGE;
        let err = parse_cursor(&png(2, &pixels), 2, (0, 0)).unwrap_err();
        assert!(err.to_string().contains("block"), "{err:#}");

        let err = parse_cursor(&png(2, &[[1, 2, 3, 255]; 4]), 2, (0, 0)).unwrap_err();
        assert!(err.to_string().contains("#010203"), "{err:#}");

        let err = parse_cursor(&png(2, &[[255, 255, 255, 128]; 4]), 2, (0, 0)).unwrap_err();
        assert!(err.to_string().contains("partly transparent"), "{err:#}");
    }

    #[test]
    fn icons_must_be_icon_sized() {
        let err = parse_icon(&png(2, &[CLEAR; 4])).unwrap_err();
        assert!(err.to_string().contains("16x16"), "{err:#}");
        let icon = parse_icon(&png(16, &[[255, 255, 255, 255]; 256])).unwrap();
        assert_eq!(icon.pixel(15, 15), 3);
    }

    // Guards the embedded UI assets: sizes, the pointer's tip, and that each
    // icon uses the pack's outline and light inks.
    #[test]
    fn embedded_ui_images_load() {
        let ui = ui().unwrap();
        let pointer = ui.pointer();
        assert_eq!((pointer.image.width(), pointer.image.height()), (12, 16));
        assert_eq!(pointer.hotspot, (0, 0));
        assert_eq!(pointer.image.pixel(0, 0), 1, "the tip is outlined");
        assert_eq!(pointer.image.pixel(1, 2), 2, "the body is inside it");

        for icon in [
            SystemIcon::Code,
            SystemIcon::Sprites,
            SystemIcon::Map,
            SystemIcon::Sfx,
            SystemIcon::Music,
            SystemIcon::Eraser,
            SystemIcon::Eyedropper,
            SystemIcon::Fill,
        ] {
            let image = ui.icon(icon);
            assert_eq!((image.width(), image.height()), (ICON_SIZE, ICON_SIZE));
            let used: std::collections::BTreeSet<u8> = (0..ICON_SIZE)
                .flat_map(|y| (0..ICON_SIZE).map(move |x| (x, y)))
                .map(|(x, y)| image.pixel(x, y))
                .collect();
            assert!(used.contains(&1) && used.contains(&3), "{icon:?}: {used:?}");
            assert_eq!(image.pixel(0, 0), 0, "{icon:?} keeps its padding");
        }
    }

    #[test]
    fn boot_chime_is_short_valid_and_ends_released() {
        use glaux_audio::AudioState;
        use glaux_audio::sfx::SfxPlayer;
        use glaux_machine::system_rom::SystemSfx;

        let mut player = SfxPlayer::new();
        player.play(sounds().unwrap().get(SystemSfx::Boot).clone());
        let mut audio = AudioState::new();
        let mut ticks = 0;
        while player.is_active() {
            player.tick(&mut audio).unwrap();
            audio.commit();
            ticks += 1;
        }
        // 200-600 ms at 60 ticks per second, before the release tails.
        assert!((12..=36).contains(&ticks), "{ticks} ticks");
        assert!(audio.channels().iter().all(|channel| !channel.gate));
    }
}
