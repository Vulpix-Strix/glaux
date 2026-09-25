//! Builds the System ROM from the assets in the repository's `rom/` folder.

use anyhow::{Context, bail, ensure};
use glaux_machine::font::{BitmapFont, FIRST_CHAR, FontMetrics, LAST_CHAR};
use glaux_machine::palette::{PALETTE_LEN, Palette, Rgb};
use glaux_machine::system_rom::{SystemRom, SystemRomVersion};

const VERSION_TXT: &str = include_str!("../../../rom/version.txt");
const PALETTE_HEX: &str = include_str!("../../../rom/palette/system.hex");
const FONT_BDF: &str = include_str!("../../../rom/font/Tamzen5x9r.bdf");

/// Decodes the embedded System ROM assets.
pub fn load() -> anyhow::Result<SystemRom> {
    let version = parse_version(VERSION_TXT).context("invalid rom/version.txt")?;
    let palette = parse_palette(PALETTE_HEX).context("invalid rom/palette/system.hex")?;
    let font =
        parse_bdf(FONT_BDF, FontMetrics::SYSTEM).context("invalid rom/font/Tamzen5x9r.bdf")?;
    Ok(SystemRom::new(version, palette, font))
}

/// Parses a `major.minor` version, each part a `u8`.
fn parse_version(text: &str) -> anyhow::Result<SystemRomVersion> {
    let text = text.trim();
    let parse = || {
        let (major, minor) = text.split_once('.')?;
        Some(SystemRomVersion::new(
            major.parse().ok()?,
            minor.parse().ok()?,
        ))
    };
    parse().with_context(|| format!("expected `major.minor` with values 0..=255, got {text:?}"))
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
    fn version_parses_major_dot_minor() {
        assert_eq!(parse_version("0.1\n").unwrap(), SystemRomVersion::new(0, 1));
        assert_eq!(
            parse_version(" 12.34 ").unwrap(),
            SystemRomVersion::new(12, 34)
        );
        for bad in ["", "1", "1.2.3", "a.b", "256.0", "1.-1"] {
            assert!(parse_version(bad).is_err(), "{bad:?}");
        }
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

        assert_eq!(rom.version(), SystemRomVersion::new(0, 1));

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
    }
}
