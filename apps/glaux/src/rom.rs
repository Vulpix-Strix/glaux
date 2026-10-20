//! Builds the System ROM from the assets in the repository's `rom/` folder
//! and the sound effects defined here.
//!
//! UI images are pixel art whose source colors are mapped to ink slots, so
//! the system chooses their palette indices when it draws them.

use anyhow::{Context, bail, ensure};
use glaux_audio::sfx::{Sfx, SfxAction, SfxStep};
use glaux_audio::{Envelope, Tone, Waveform};
use glaux_machine::font::FontMetrics;
use glaux_machine::ink_image::InkImage;
use glaux_machine::obj_palette::{OBJ_PALETTE_BANKS, OBJ_PALETTE_COLORS, ObjPalettes};
use glaux_machine::palette::{PALETTE_LEN, Palette, Rgb};
use glaux_machine::system_rom::{
    Cursor, IndexedImage, SystemCursor, SystemIcon, SystemRom, SystemSounds, SystemUi,
};
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
const OBP_TXT: &str = include_str!("../../../rom/palette/obp.txt");
const LOGO_PNG: (&str, &[u8]) = embed!("ui/logo.png");

/// The editorial credits, which the credits view reads when it opens.
pub const CREDITS_RON: &str = include_str!("../../../rom/credits.ron");

/// How one cursor is stored: its image, how many times its native size it
/// is drawn, how many frames it holds side by side, and its hotspot in
/// pixels of one frame of the image as stored.
struct CursorSource {
    png: (&'static str, &'static [u8]),
    scale: usize,
    frames: usize,
    hotspot: (usize, usize),
}

/// The cursors in `SystemCursor` order.
const CURSORS: [CursorSource; SystemCursor::COUNT] = [
    // The arrow's tip.
    CursorSource {
        png: embed!("ui/cursors/pointer.png"),
        scale: 2,
        frames: 1,
        hotspot: (0, 0),
    },
    // The middle of the I-beam's stem.
    CursorSource {
        png: embed!("ui/cursors/beam.png"),
        scale: 2,
        frames: 1,
        hotspot: (8, 15),
    },
    // The tip of the pointing finger.
    CursorSource {
        png: embed!("ui/cursors/link3.png"),
        scale: 2,
        frames: 1,
        hotspot: (8, 0),
    },
    // The middle of the closed hand.
    CursorSource {
        png: embed!("ui/cursors/grab.png"),
        scale: 2,
        frames: 1,
        hotspot: (14, 16),
    },
    // The neck of the hourglass, five frames drawn at their native size.
    CursorSource {
        png: embed!("ui/cursors/hourglass.png"),
        scale: 1,
        frames: 5,
        hotspot: (7, 7),
    },
];

/// The icons in `SystemIcon` order.
const ICON_PNGS: [(&str, &[u8]); SystemIcon::COUNT] = [
    embed!("ui/icons/code.png"),
    embed!("ui/icons/sprites.png"),
    embed!("ui/icons/map.png"),
    embed!("ui/icons/sfx.png"),
    embed!("ui/icons/music.png"),
    embed!("ui/icons/eraser.png"),
    embed!("ui/icons/eyedropper.png"),
    embed!("ui/icons/fill.png"),
    embed!("ui/icons/settings.png"),
];

/// Side of a view icon, in pixels.
pub const ICON_SIZE: usize = 16;

/// Ink slots of the 1-bit Pixel Icons' colors: black outline and white
/// light. The pack has no shade, so slot 2 goes unused.
const ICON_INKS: [(u32, u8); 2] = [(0x000000, 1), (0xffffff, 3)];
/// Ink slots of the light Simple Pixel Cursors' colors: outline, body.
const CURSOR_INKS: [(u32, u8); 2] = [(0x2e222f, 1), (0xffffff, 2)];

mod pixel_code;

/// Decodes the embedded System ROM assets.
pub fn load() -> anyhow::Result<SystemRom> {
    let rom = build(VERSION, COPYRIGHT, PALETTE_HEX, OBP_TXT)?;
    let font = rom.font();
    tracing::info!(rom_version = %rom.version(), "System ROM loaded");
    tracing::debug!(
        cell_width = font.metrics().cell_width(),
        cell_height = font.metrics().cell_height(),
        glyphs = font.glyph_count(),
        "system font"
    );
    Ok(rom)
}

/// Builds a System ROM from the text of its assets. Errors name the asset
/// that failed to decode.
fn build(
    version: &str,
    copyright: &str,
    palette: &str,
    obj_palettes: &str,
) -> anyhow::Result<SystemRom> {
    let version = Version::parse(version.trim())
        .with_context(|| format!("invalid rom/version: {:?}", version.trim()))?;
    let copyright = parse_copyright(copyright).context("invalid rom/copyright")?;
    let palette = parse_palette(palette).context("invalid rom/palette/system.hex")?;
    let obj_palettes = parse_obj_palettes(obj_palettes).context("invalid rom/palette/obp.txt")?;
    let font = pixel_code::load().context("invalid system font")?;
    ensure!(
        font.metrics() == FontMetrics::SYSTEM,
        "the system font's cells are not the system's {}x{}",
        FontMetrics::SYSTEM.cell_width(),
        FontMetrics::SYSTEM.cell_height()
    );
    let logo =
        parse_logo(LOGO_PNG.1, &palette).with_context(|| format!("invalid {}", LOGO_PNG.0))?;
    Ok(SystemRom::new(version, copyright, palette, font)
        .with_logo(logo)
        .with_sounds(sounds()?)
        .with_ui(ui()?)
        .with_obj_palettes(obj_palettes))
}

/// The System ROM UI images.
pub fn ui() -> anyhow::Result<SystemUi> {
    let mut cursors = Vec::with_capacity(CURSORS.len());
    for source in &CURSORS {
        let (name, bytes) = source.png;
        cursors.push(
            parse_cursor(bytes, source.scale, source.frames, source.hotspot)
                .with_context(|| format!("invalid {name}"))?,
        );
    }
    let cursors = cursors
        .try_into()
        .unwrap_or_else(|_| unreachable!("one image per cursor"));
    let mut icons = Vec::with_capacity(ICON_PNGS.len());
    for (name, bytes) in ICON_PNGS {
        icons.push(parse_icon(bytes).with_context(|| format!("invalid {name}"))?);
    }
    let icons = icons
        .try_into()
        .unwrap_or_else(|_| unreachable!("one image per icon"));
    Ok(SystemUi::new(cursors, icons))
}

/// Decodes the logo, whose colors must each be in the first
/// [`SYSTEM_COLORS`] entries of `palette`, the colors of the system's own UI.
/// Transparent pixels stay transparent. Any size is accepted, so replacing
/// the file needs no change here.
fn parse_logo(png: &[u8], palette: &Palette) -> anyhow::Result<IndexedImage> {
    let (width, height, rgba) = decode_png(png)?;
    let mut pixels = Vec::with_capacity(rgba.len());
    for [r, g, b, a] in rgba {
        if a < 128 {
            pixels.push(None);
            continue;
        }
        let color = Rgb { r, g, b };
        let index = palette.entries()[..SYSTEM_COLORS]
            .iter()
            .position(|&entry| entry == color)
            .with_context(|| {
                format!("color #{r:02x}{g:02x}{b:02x} is not in the system palette")
            })?;
        pixels.push(Some(u8::try_from(index)?));
    }
    IndexedImage::new(width, height, pixels).context("pixel count does not match the size")
}

/// Palette entries the system draws its own interface with.
const SYSTEM_COLORS: usize = 32;

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

/// Decodes a cursor of `frames` frames side by side, each drawn at
/// `scale` times its native size, with `hotspot` in pixels of one frame
/// as stored. Transparent edges every frame shares are trimmed before the
/// frames are shrunk, so art drawn off the scale's grid still decodes, and
/// the hotspot moves with them.
fn parse_cursor(
    png: &[u8],
    scale: usize,
    frames: usize,
    hotspot: (usize, usize),
) -> anyhow::Result<Cursor> {
    let (width, height, rgba) = decode_png(png)?;
    ensure!(
        frames > 0 && width.is_multiple_of(frames),
        "{width} pixels across do not split into {frames} frames"
    );
    let frame_width = width / frames;
    let slots = &inks(width, &rgba, &CURSOR_INKS)?;

    let opaque = |x: usize, y: usize| {
        (0..frames).any(|frame| slots[y * width + frame * frame_width + x] != 0)
    };
    let columns: Vec<usize> = (0..frame_width)
        .filter(|&x| (0..height).any(|y| opaque(x, y)))
        .collect();
    let rows: Vec<usize> = (0..height)
        .filter(|&y| (0..frame_width).any(|x| opaque(x, y)))
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

    let (trimmed_width, trimmed_height) = (right - left + 1, bottom - top + 1);
    let images = (0..frames)
        .map(|frame| {
            let x0 = frame * frame_width + left;
            let trimmed: Vec<u8> = (top..=bottom)
                .flat_map(|y| (x0..x0 + trimmed_width).map(move |x| slots[y * width + x]))
                .collect();
            let (native_width, native_height, native) =
                downscale(trimmed_width, trimmed_height, &trimmed, scale)
                    .with_context(|| format!("frame {frame}"))?;
            Ok(InkImage::new(native_width, native_height, native)?)
        })
        .collect::<anyhow::Result<_>>()?;
    let hotspot = (
        u8::try_from((hotspot.0 - left) / scale)?,
        u8::try_from((hotspot.1 - top) / scale)?,
    );
    Ok(Cursor {
        frames: images,
        hotspot,
    })
}

/// Shrinks an image drawn at `scale` times its size, requiring every
/// `scale`-square block to be one value so no detail is lost.
fn downscale<T: Copy + PartialEq>(
    width: usize,
    height: usize,
    pixels: &[T],
    scale: usize,
) -> anyhow::Result<(usize, usize, Vec<T>)> {
    ensure!(
        scale > 0 && width.is_multiple_of(scale) && height.is_multiple_of(scale),
        "{width}x{height} pixels is not a multiple of the {scale}x scale"
    );
    let (native_w, native_h) = (width / scale, height / scale);
    let mut native = Vec::with_capacity(native_w * native_h);
    for y in 0..native_h {
        for x in 0..native_w {
            let value = pixels[y * scale * width + x * scale];
            for (dx, dy) in (0..scale).flat_map(|dy| (0..scale).map(move |dx| (dx, dy))) {
                let (sx, sy) = (x * scale + dx, y * scale + dy);
                ensure!(
                    pixels[sy * width + sx] == value,
                    "pixel ({sx}, {sy}) breaks the {scale}x{scale} block at ({x}, {y})"
                );
            }
            native.push(value);
        }
    }
    Ok((native_w, native_h, native))
}

/// Maps each pixel to the ink slot `table` gives its color. Fully
/// transparent pixels are slot 0. Partial transparency and colors missing
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
    if let Some(ch) = notice.chars().find(|ch| !(' '..='~').contains(ch)) {
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

/// Parses the default object palette banks: one line per bank holding the
/// master indices of locals 1–15, with `#` starting a comment. Local 0 is
/// implicit and transparent.
fn parse_obj_palettes(text: &str) -> anyhow::Result<ObjPalettes> {
    let mut bytes = [0; OBJ_PALETTE_BANKS * OBJ_PALETTE_COLORS];
    let mut bank = 0;

    for (number, line) in text.lines().enumerate().map(|(i, l)| (i + 1, l)) {
        let line = line.split_once('#').map_or(line, |(data, _)| data).trim();
        if line.is_empty() {
            continue;
        }
        ensure!(
            bank < OBJ_PALETTE_BANKS,
            "line {number}: more than {OBJ_PALETTE_BANKS} banks"
        );
        let entries = line.split_whitespace().collect::<Vec<_>>();
        ensure!(
            entries.len() == OBJ_PALETTE_COLORS - 1,
            "line {number}: expected {} master indices, got {}",
            OBJ_PALETTE_COLORS - 1,
            entries.len()
        );
        let row = &mut bytes[bank * OBJ_PALETTE_COLORS..(bank + 1) * OBJ_PALETTE_COLORS];
        for (slot, entry) in row[1..].iter_mut().zip(entries) {
            *slot = entry
                .parse()
                .with_context(|| format!("line {number}: {entry:?} is not a master index 0-255"))?;
        }
        bank += 1;
    }

    ensure!(
        bank == OBJ_PALETTE_BANKS,
        "expected {OBJ_PALETTE_BANKS} banks, found {bank}"
    );
    Ok(ObjPalettes::from_bytes(&bytes)?)
}

#[cfg(test)]
mod tests;
