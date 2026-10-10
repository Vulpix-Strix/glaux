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
mod tests;
