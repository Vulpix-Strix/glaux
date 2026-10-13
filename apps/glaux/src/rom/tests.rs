use super::*;
use glaux_machine::obj_palette::ObjBank;
use glaux_machine::palette::ZUGHY_32;
use pretty_assertions::assert_eq;

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
        let err = build("0.1.0", text, PALETTE_HEX, OBP_TXT).unwrap_err();
        let message = format!("{err:#}");
        assert!(message.contains("rom/copyright"), "{message}");
        assert!(message.contains(detail), "{message}");
    }
}

#[test]
fn invalid_version_names_the_asset() {
    let err = build("zero.one", COPYRIGHT, PALETTE_HEX, OBP_TXT).unwrap_err();
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

// Guards the embedded repository assets.
#[test]
fn embedded_rom_loads() {
    let rom = load().unwrap();

    assert_eq!(rom.version(), &Version::new(0, 1, 0));
    assert_eq!(rom.copyright(), "(C) 2026 CUERVOLU");

    // The machine's own default palette is the one the ROM ships.
    assert_eq!(rom.palette(), &Palette::default());

    let font = rom.font();
    assert_eq!(font.metrics(), FontMetrics::SYSTEM);
    assert!(
        rom.copyright()
            .chars()
            .all(|ch| font.glyph_id(ch).is_some())
    );
    assert_eq!(rom.sounds(), &sounds().unwrap());
    assert_eq!(rom.ui(), &ui().unwrap());
}

// Cartridges without obp.bin, including sheets from `glaux convert sprites`,
// store master indices 1-15 directly as tile-local indices.
#[test]
fn embedded_obp0_is_the_identity_mapping() {
    let rom = load().unwrap();
    let obp0 = &rom.obj_palettes().as_bytes()[..OBJ_PALETTE_COLORS];
    assert_eq!(
        obp0,
        &ObjPalettes::linear().as_bytes()[..OBJ_PALETTE_COLORS]
    );
}

// `glaux import sprites` rewrites master entries from 32 up with each
// sheet's colors, so a default bank that pointed there would change color in
// every cartridge with imported sprites. Zughy 32 stays the same in all of them.
#[test]
fn embedded_banks_are_transparent_at_zero_and_use_distinct_zughy_colors() {
    let rom = load().unwrap();
    for bank in (0..OBJ_PALETTE_BANKS as u8).filter_map(ObjBank::new) {
        assert_eq!(rom.obj_palettes().resolve(bank, 0), None, "{bank:?}");
        let mut masters = Vec::new();
        for local in 1..OBJ_PALETTE_COLORS as u8 {
            let master = rom.obj_palettes().resolve(bank, local).unwrap();
            assert!(
                usize::from(master) < ZUGHY_32.len(),
                "{bank:?} local {local} is master {master}"
            );
            masters.push(master);
        }
        masters.sort_unstable();
        masters.dedup();
        assert_eq!(
            masters.len(),
            OBJ_PALETTE_COLORS - 1,
            "{bank:?} repeats a color"
        );
    }
}

#[test]
fn obj_palettes_are_read_in_order_with_comments() {
    let row = |first: u8| -> String {
        (first..first + 15)
            .map(|m| format!("{m} "))
            .collect::<String>()
    };
    let mut text = String::from("# header\n\n");
    for bank in 0..8 {
        text += &format!("{}  # bank {bank}\n", row(bank));
    }
    let banks = parse_obj_palettes(&text).unwrap();
    let bank = ObjBank::new(7).unwrap();
    assert_eq!(banks.resolve(bank, 0), None);
    assert_eq!(banks.resolve(bank, 1), Some(7));
    assert_eq!(banks.resolve(bank, 15), Some(21));
}

#[test]
fn invalid_obj_palettes_name_the_asset_and_the_problem() {
    let good = "1 2 3 4 5 6 7 8 9 10 11 12 13 14 15\n";
    for (text, detail) in [
        (good.repeat(7), "expected 8 banks, found 7".to_owned()),
        (good.repeat(9), "line 9: more than 8 banks".to_owned()),
        (
            format!("{}1 2 3\n", good.repeat(7)),
            "line 8: expected 15 master indices, got 3".to_owned(),
        ),
        (
            format!("{}1 2 3 4 5 6 7 8 9 10 11 12 13 14 256\n", good.repeat(7)),
            "\"256\" is not a master index".to_owned(),
        ),
    ] {
        let err = build("0.1.0", COPYRIGHT, PALETTE_HEX, &text).unwrap_err();
        let message = format!("{err:#}");
        assert!(message.contains("rom/palette/obp.txt"), "{message}");
        assert!(message.contains(&detail), "{message}");
    }
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

const SAMPLE: [&str; 4] = ["....", ".o..", ".o#o", "...."];

fn sample_image() -> InkImage {
    InkImage::new(3, 2, vec![1, 0, 0, 1, 2, 1]).unwrap()
}

#[test]
fn cursors_are_downscaled_and_trimmed_with_their_hotspot() {
    let source = cursor_png(&SAMPLE, 2);
    let cursor = parse_cursor(&source, 2, 1, (2, 2)).unwrap();
    assert_eq!(cursor.frames, [sample_image()]);
    assert_eq!(cursor.hotspot, (0, 0));

    // Hotspots are in pixels as stored, so they shrink with the image.
    let cursor = parse_cursor(&source, 2, 1, (5, 5)).unwrap();
    assert_eq!(cursor.hotspot, (1, 1));
    let err = parse_cursor(&source, 2, 1, (1, 1)).unwrap_err();
    assert!(err.to_string().contains("hotspot"), "{err:#}");
}

#[test]
fn cursors_drawn_off_their_scales_grid_still_decode() {
    // The sample shifted down one stored pixel, so its blocks start on odd
    // rows.
    let source = cursor_png(&SAMPLE, 2);
    let mut decoder = png::Decoder::new(std::io::Cursor::new(&source));
    decoder.set_transformations(png::Transformations::EXPAND);
    let mut reader = decoder.read_info().unwrap();
    let mut pixels = vec![0; reader.output_buffer_size().unwrap()];
    reader.next_frame(&mut pixels).unwrap();
    let mut shifted = vec![CLEAR; 8];
    shifted.extend(pixels.as_chunks::<4>().0);
    let cursor = parse_cursor(&png(8, &shifted), 2, 1, (2, 3)).unwrap();
    assert_eq!(cursor.frames, [sample_image()]);
    assert_eq!(cursor.hotspot, (0, 0));
}

#[test]
fn cursor_frames_share_one_trim_and_hotspot() {
    // Two frames side by side: an outline pixel, then a body pixel, at
    // different spots of their frames.
    let source = cursor_png(&["....", "o..#"], 1);
    let cursor = parse_cursor(&source, 1, 2, (0, 1)).unwrap();
    assert_eq!(
        cursor.frames,
        [
            InkImage::new(2, 1, vec![1, 0]).unwrap(),
            InkImage::new(2, 1, vec![0, 2]).unwrap(),
        ]
    );
    assert_eq!(cursor.hotspot, (0, 0));
    let err = parse_cursor(&source, 1, 3, (0, 1)).unwrap_err();
    assert!(err.to_string().contains("3 frames"), "{err:#}");
}

#[test]
fn cursors_must_be_whole_blocks_of_known_colors() {
    let mut pixels = vec![BODY; 4];
    pixels[3] = EDGE;
    let err = parse_cursor(&png(2, &pixels), 2, 1, (0, 0)).unwrap_err();
    assert!(format!("{err:#}").contains("block"), "{err:#}");

    let err = parse_cursor(&png(2, &[[1, 2, 3, 255]; 4]), 2, 1, (0, 0)).unwrap_err();
    assert!(err.to_string().contains("#010203"), "{err:#}");

    let err = parse_cursor(&png(2, &[[255, 255, 255, 128]; 4]), 2, 1, (0, 0)).unwrap_err();
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
    let arrow = &ui.cursor(SystemCursor::Arrow).frames[0];
    assert_eq!((arrow.width(), arrow.height()), (12, 16));
    assert_eq!(arrow.pixel(0, 0), 1, "the tip is outlined");
    assert_eq!(arrow.pixel(1, 2), 2, "the body is inside it");
    for (cursor, size, frames, hotspot) in [
        (SystemCursor::Arrow, (12, 16), 1, (0, 0)),
        (SystemCursor::Text, (7, 15), 1, (3, 7)),
        (SystemCursor::Hand, (14, 16), 1, (4, 0)),
        (SystemCursor::Grab, (13, 13), 1, (6, 6)),
        (SystemCursor::Busy, (15, 16), 5, (7, 7)),
    ] {
        let decoded = ui.cursor(cursor);
        assert_eq!(decoded.frames.len(), frames, "{cursor:?}");
        assert_eq!(decoded.hotspot, hotspot, "{cursor:?}");
        for image in &decoded.frames {
            assert_eq!((image.width(), image.height()), size, "{cursor:?}");
        }
        let (x, y) = hotspot;
        let at_hotspot = decoded.frames[0].pixel(usize::from(x), usize::from(y));
        assert_ne!(at_hotspot, 0, "{cursor:?} points with a pixel it draws");
    }
    // The hourglass's sand moves from frame to frame.
    let busy = &ui.cursor(SystemCursor::Busy).frames;
    assert!(busy.windows(2).all(|pair| pair[0] != pair[1]));

    for icon in [
        SystemIcon::Code,
        SystemIcon::Sprites,
        SystemIcon::Map,
        SystemIcon::Sfx,
        SystemIcon::Music,
        SystemIcon::Eraser,
        SystemIcon::Eyedropper,
        SystemIcon::Fill,
        SystemIcon::Settings,
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

    // The options bar fits only the cogwheel's central 12×12 pixels.
    let settings = ui.icon(SystemIcon::Settings);
    let inset = 2..ICON_SIZE - 2;
    for (x, y) in (0..ICON_SIZE).flat_map(|y| (0..ICON_SIZE).map(move |x| (x, y))) {
        if !inset.contains(&x) || !inset.contains(&y) {
            assert_eq!(settings.pixel(x, y), 0, "settings icon at ({x}, {y})");
        }
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
