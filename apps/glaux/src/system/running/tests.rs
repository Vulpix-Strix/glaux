use std::fs;

use std::path::Path;

use glaux_luau::ScriptError;
use glaux_machine::obj_palette::{ObjBank, ObjPalettes};
use pretty_assertions::assert_eq;

use super::*;
use crate::cartridge::fixture::{write_cartridge, write_rhai_cartridge};
use crate::cartridge_fs::fixture::Fixture;

mod debugging;

fn defaults() -> CartridgeVideoDefaults {
    CartridgeVideoDefaults::from_rom(&crate::rom::load().unwrap())
}

fn texts(entries: Vec<LogEntry>) -> Vec<String> {
    entries.into_iter().map(|entry| entry.text).collect()
}

#[test]
fn a_cartridge_requires_its_own_modules() {
    let fixture = Fixture::new("running-modules");
    let dir = fixture.root().join("pong");
    write_cartridge(&dir, "Pong");
    fs::create_dir_all(dir.join("code/game")).unwrap();
    fs::write(
        dir.join("code/game/enemies.luau"),
        r#"log.info("enemies loaded") return { count = 3 }"#,
    )
    .unwrap();
    fs::write(
        dir.join("code/main.luau"),
        r#"
            local enemies = require("game.enemies")
            function tick() log.info(require("game.enemies").count + enemies.count) end
            "#,
    )
    .unwrap();
    let cartridge = LoadedCartridge::load(&fixture.storage(), "pong").unwrap();

    let mut running =
        RunningCartridge::start(cartridge, &defaults(), VideoOverrides::default()).unwrap();
    assert_eq!(texts(running.take_output()), ["enemies loaded"]);
    running.tick(&[]).unwrap();
    running.tick(&[]).unwrap();
    assert_eq!(texts(running.take_output()), ["6", "6"]);
}

#[test]
fn a_rhai_cartridge_imports_its_own_modules() {
    let fixture = Fixture::new("running-rhai-modules");
    let dir = fixture.root().join("owl");
    write_rhai_cartridge(
        &dir,
        "Owl",
        "import \"game.enemies\" as enemies;\nfn tick() { log::info(enemies::count() * 2); }",
    );
    fs::create_dir_all(dir.join("code/game")).unwrap();
    fs::write(
        dir.join("code/game/enemies.rhai"),
        "log::info(\"enemies loaded\");\nfn count() { 3 }",
    )
    .unwrap();
    // A file outside `code/` is not a module, whatever the name says.
    fs::write(dir.join("secret.rhai"), "fn count() { 99 }").unwrap();
    let cartridge = LoadedCartridge::load(&fixture.storage(), "owl").unwrap();

    let mut running =
        RunningCartridge::start(cartridge, &defaults(), VideoOverrides::default()).unwrap();
    assert_eq!(texts(running.take_output()), ["enemies loaded"]);
    running.tick(&[]).unwrap();
    assert_eq!(texts(running.take_output()), ["6"]);

    fs::write(dir.join("code/main.rhai"), "import \"secret\" as s;").unwrap();
    let cartridge = running.stop();
    let failed =
        RunningCartridge::start(cartridge, &defaults(), VideoOverrides::default()).unwrap_err();
    let StartError::Script(failure) = failed.error else {
        panic!("expected a script error");
    };
    assert_eq!(
        failure.error.to_string(),
        "Module not found: code/secret.rhai (code/main.rhai:1)"
    );
}

#[test]
fn module_errors_name_cartridge_relative_paths() {
    let fixture = Fixture::new("running-module-errors");
    let dir = fixture.root().join("pong");
    write_cartridge(&dir, "Pong");
    fs::write(dir.join("code/main.luau"), r#"require("boom")"#).unwrap();
    fs::write(dir.join("code/boom.luau"), "\nerror(\"x\")").unwrap();
    let cartridge = LoadedCartridge::load(&fixture.storage(), "pong").unwrap();

    let failed =
        RunningCartridge::start(cartridge, &defaults(), VideoOverrides::default()).unwrap_err();
    let StartError::Script(failure) = failed.error else {
        panic!("expected a script error, got {:?}", failed.error);
    };
    assert_eq!(
        failure.error.error,
        ScriptError::Runtime("code/boom.luau:2: x".into())
    );

    fs::write(dir.join("code/main.luau"), r#"require("nope")"#).unwrap();
    let failed = RunningCartridge::start(failed.cartridge, &defaults(), VideoOverrides::default())
        .unwrap_err();
    let StartError::Script(failure) = failed.error else {
        panic!("expected a script error, got {:?}", failed.error);
    };
    assert_eq!(
        failure.error.to_string(),
        "Module not found: code/nope.luau (code/main.luau:1)"
    );
}

#[test]
fn sprites_resolve_through_the_cartridge_palette_assets() {
    let fixture = Fixture::new("running-video-assets");
    let dir = fixture.root().join("pong");
    write_cartridge(&dir, "Pong");
    fs::write(
        dir.join("code/main.luau"),
        "function draw() spr(0, 0, 0) spr(0, 8, 0, 1, 1, false, false, 0) end",
    )
    .unwrap();
    // Tile 0 is solid local index 3 and defaults to OBP2, which maps it
    // to master index 200.
    fs::write(dir.join("sprites.bin"), [0x33; 32]).unwrap();
    fs::write(dir.join("objattr.bin"), [2]).unwrap();
    let mut obp = *ObjPalettes::linear().as_bytes();
    obp[2 * 16 + 3] = 200;
    fs::write(dir.join("obp.bin"), obp).unwrap();
    let mut palette = *Palette::default().entries();
    palette[200] = glaux_machine::palette::Rgb::from_hex(0x123456);
    let palette = Palette::new(palette);
    fs::write(dir.join("palette.bin"), palette.to_rgb888()).unwrap();
    let cartridge = LoadedCartridge::load(&fixture.storage(), "pong").unwrap();

    let mut running =
        RunningCartridge::start(cartridge, &defaults(), VideoOverrides::default()).unwrap();
    running.tick(&[]).unwrap();
    let rom = crate::rom::load().unwrap();
    let mut framebuffer = Framebuffer::new();
    running.render(&mut framebuffer, rom.font());

    assert_eq!(framebuffer.pixel(0, 0), Some(200));
    assert_eq!(framebuffer.pixel(8, 0), Some(3));
    assert_eq!(running.palette(), &palette);
}

#[test]
fn missing_video_assets_fall_back_to_the_rom() {
    let fixture = Fixture::new("running-video-defaults");
    let dir = fixture.root().join("pong");
    write_cartridge(&dir, "Pong");
    let cartridge = LoadedCartridge::load(&fixture.storage(), "pong").unwrap();
    let rom = crate::rom::load().unwrap();
    let running =
        RunningCartridge::start(cartridge, &defaults(), VideoOverrides::default()).unwrap();
    assert_eq!(running.palette(), rom.palette());
    assert_eq!(running.video.obj.palettes(), rom.obj_palettes());
}

/// A cartridge without `obp.bin` keeps drawing OBP0 tiles with their local
/// indices as master indices, and every bank resolves through the System
/// ROM's curated banks.
#[test]
fn rom_default_banks_render_without_obp_bin() {
    let fixture = Fixture::new("running-default-banks");
    let dir = fixture.root().join("pong");
    write_cartridge(&dir, "Pong");
    fs::write(
        dir.join("code/main.luau"),
        "function draw() cls(255) for bank = 0, 7 do spr(0, bank * 8, 0, 1, 1, false, false, bank) end end",
    )
    .unwrap();
    // Tile 0's top two rows hold local indices 0 to 15 in order.
    let mut tile = [0; 32];
    for (offset, byte) in tile[..8].iter_mut().enumerate() {
        let left = offset as u8 * 2;
        *byte = (left << 4) | (left + 1);
    }
    fs::write(dir.join("sprites.bin"), tile).unwrap();
    let cartridge = LoadedCartridge::load(&fixture.storage(), "pong").unwrap();

    let mut running =
        RunningCartridge::start(cartridge, &defaults(), VideoOverrides::default()).unwrap();
    running.tick(&[]).unwrap();
    let rom = crate::rom::load().unwrap();
    let mut framebuffer = Framebuffer::new();
    running.render(&mut framebuffer, rom.font());

    for bank in 0..8 {
        for local in 1..16 {
            let (x, y) = (bank * 8 + local % 8, local / 8);
            let master = framebuffer.pixel(x, y).unwrap();
            if bank == 0 {
                assert_eq!(i32::from(master), local, "OBP0 local {local}");
            }
            let expected =
                ObjBank::new(bank as u8).and_then(|b| rom.obj_palettes().resolve(b, local as u8));
            assert_eq!(Some(master), expected, "OBP{bank} local {local}");
        }
        // Local 0 stays transparent.
        assert_eq!(framebuffer.pixel(bank * 8, 0), Some(255), "OBP{bank}");
    }
}

/// The imported example (`examples/palettes`) shows every opaque pixel
/// of its PNG in the PNG's own color once its frame is resolved through
/// the cartridge palette.
#[test]
fn the_imported_example_shows_its_png_colors() {
    let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    let root = fs::canonicalize(examples).unwrap();
    let storage = crate::cartridge_fs::CartridgeStorage::Available(
        crate::cartridge_fs::CartridgeFs::new(root.clone()),
    );
    let cartridge = LoadedCartridge::load(&storage, "palettes").unwrap();
    let mut running =
        RunningCartridge::start(cartridge, &defaults(), VideoOverrides::default()).unwrap();
    running.tick(&[]).unwrap();
    let rom = crate::rom::load().unwrap();
    let mut framebuffer = Framebuffer::new();
    running.render(&mut framebuffer, rom.font());

    let file = fs::File::open(root.join("palettes/sprites.png")).unwrap();
    let mut reader = png::Decoder::new(std::io::BufReader::new(file))
        .read_info()
        .unwrap();
    let mut rgba = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut rgba).unwrap();
    assert_eq!(info.color_type, png::ColorType::Rgba);
    let width = info.width as usize;
    let (sheet_x, sheet_y) = (8, 24);
    let mut opaque = 0;
    for (offset, &[r, g, b, alpha]) in rgba.as_chunks::<4>().0.iter().enumerate() {
        if alpha == 0 {
            continue;
        }
        let (x, y) = ((offset % width) as i32, (offset / width) as i32);
        let index = framebuffer.pixel(sheet_x + x, sheet_y + y).unwrap();
        assert_eq!(
            running.palette().get(index),
            glaux_machine::palette::Rgb { r, g, b },
            "sheet pixel ({x}, {y})"
        );
        opaque += 1;
    }
    assert!(opaque > 300, "{opaque} opaque pixels");
}

/// Every pixel of each imported example's PNG resolves through the
/// cartridge's tiles, tile banks, object palettes and master palette to the
/// PNG's own color, and every transparent pixel to local index 0.
#[test]
fn imported_examples_resolve_every_png_pixel_to_its_color() {
    use glaux_machine::sprite::{OBJ_GRID_COLUMNS, OBJ_TILE_HEIGHT, OBJ_TILE_WIDTH};

    let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    let root = fs::canonicalize(examples).unwrap();
    let storage = crate::cartridge_fs::CartridgeStorage::Available(
        crate::cartridge_fs::CartridgeFs::new(root.clone()),
    );
    for name in ["palettes", "signal-lost"] {
        let cartridge = LoadedCartridge::load(&storage, name).unwrap();
        let running =
            RunningCartridge::start(cartridge, &defaults(), VideoOverrides::default()).unwrap();
        let obj = &running.video.obj;

        let file = fs::File::open(root.join(name).join("sprites.png")).unwrap();
        let mut reader = png::Decoder::new(std::io::BufReader::new(file))
            .read_info()
            .unwrap();
        let mut rgba = vec![0; reader.output_buffer_size().unwrap()];
        let info = reader.next_frame(&mut rgba).unwrap();
        assert_eq!(info.color_type, png::ColorType::Rgba, "{name}");
        let width = info.width as usize;
        for (offset, &[r, g, b, alpha]) in rgba.as_chunks::<4>().0.iter().enumerate() {
            let (x, y) = (offset % width, offset / width);
            let tile = (y / OBJ_TILE_HEIGHT) * OBJ_GRID_COLUMNS + x / OBJ_TILE_WIDTH;
            let local = obj
                .tiles()
                .pixel(tile, x % OBJ_TILE_WIDTH, y % OBJ_TILE_HEIGHT)
                .unwrap();
            if alpha == 0 {
                assert_eq!(local, 0, "{name} pixel ({x}, {y})");
                continue;
            }
            let master = obj
                .palettes()
                .resolve(obj.attributes().bank(tile), local)
                .unwrap();
            assert_eq!(
                running.palette().get(master),
                glaux_machine::palette::Rgb { r, g, b },
                "{name} pixel ({x}, {y})"
            );
        }
    }
}

/// Signal Lost, driven through the real loader, module source and
/// object-tile validation, in both its Luau original
/// (`examples/signal-lost`) and its Rhai port (`examples/signal-lost-rhai`).
/// Behavior assertions look at drawn text and command counts rather than
/// sprite tiles or coordinates. The lockstep test holds the port to every
/// command, sound and log line of the original.
mod signal_lost {
    use std::path::{Path, PathBuf};

    use glaux_input::Button;
    use glaux_script::LogLevel;
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::cartridge_fs::{CartridgeFs, CartridgeStorage};

    const LUAU: &str = "signal-lost";
    const RHAI: &str = "signal-lost-rhai";
    const EXAMPLES: [&str; 2] = [LUAU, RHAI];

    /// Ticks after starting a run after which an idle knight must have
    /// fallen. That is about two minutes of play.
    const IDLE_DEATH_LIMIT: usize = 7200;

    fn examples_dir() -> PathBuf {
        let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
        fs::canonicalize(examples).unwrap()
    }

    fn start(name: &str) -> RunningCartridge {
        let storage = CartridgeStorage::Available(CartridgeFs::new(examples_dir()));
        let cartridge = LoadedCartridge::load(&storage, name).unwrap();
        RunningCartridge::start(cartridge, &defaults(), VideoOverrides::default()).unwrap()
    }

    fn down(button: Button) -> InputEvent {
        InputEvent::ButtonDown(button)
    }

    fn up(button: Button) -> InputEvent {
        InputEvent::ButtonUp(button)
    }

    /// Advances one tick, then commits the cartridge's audio as the host
    /// does after every tick, which bounds its pending writes.
    fn step(running: &mut RunningCartridge, input: &[InputEvent]) {
        running.tick(input).unwrap();
        running.runtime.audio_mut().commit();
    }

    fn idle(running: &mut RunningCartridge, ticks: usize) {
        for _ in 0..ticks {
            step(running, &[]);
        }
    }

    /// Taps A on the title screen to begin the first run.
    fn start_playing(name: &str) -> RunningCartridge {
        let mut running = start(name);
        step(&mut running, &[down(Button::A)]);
        step(&mut running, &[up(Button::A)]);
        running
    }

    fn frame_texts(running: &RunningCartridge) -> Vec<String> {
        running
            .runtime
            .frame()
            .iter()
            .filter_map(|command| match command {
                DrawCommand::Text { text, .. } => Some(text.clone()),
                _ => None,
            })
            .collect()
    }

    fn shows(running: &RunningCartridge, needle: &str) -> bool {
        frame_texts(running)
            .iter()
            .any(|text| text.contains(needle))
    }

    fn sprite_count(running: &RunningCartridge) -> usize {
        running
            .runtime
            .frame()
            .iter()
            .filter(|command| matches!(command, DrawCommand::Sprite { .. }))
            .count()
    }

    #[test]
    fn boots_to_a_drawn_title_screen() {
        for name in EXAMPLES {
            let mut running = start(name);
            assert_eq!(
                texts(running.take_output()),
                ["Signal Lost ready"],
                "{name}"
            );

            idle(&mut running, 120);
            assert!(shows(&running, "SIGNAL LOST"), "{name}");
            assert!(shows(&running, "X DASH"), "{name}");
            assert!(sprite_count(&running) > 0, "{name}");
            assert!(
                running
                    .runtime
                    .frame()
                    .iter()
                    .any(|command| matches!(command, DrawCommand::Rect { .. })),
                "{name}"
            );

            let rom = crate::rom::load().unwrap();
            let mut framebuffer = Framebuffer::new();
            running.render(&mut framebuffer, rom.font());
            let first = framebuffer.as_bytes()[0];
            assert!(
                framebuffer.as_bytes().iter().any(|&index| index != first),
                "{name}"
            );
        }
    }

    #[test]
    fn pressing_a_starts_a_run_with_a_hud() {
        for name in EXAMPLES {
            let mut running = start_playing(name);
            assert!(!shows(&running, "PRESS Z TO START"), "{name}");
            assert!(shows(&running, "SCORE 00000"), "{name}");
            assert!(shows(&running, "TIME"), "{name}");
            let band = DrawCommand::Rect {
                x: 0,
                y: 0,
                width: 320,
                height: 16,
                color: 11,
            };
            assert!(running.runtime.frame().contains(&band), "{name}");

            let rom = crate::rom::load().unwrap();
            let mut framebuffer = Framebuffer::new();
            running.render(&mut framebuffer, rom.font());
            assert_eq!(framebuffer.pixel(150, 14), Some(11), "{name}");

            let output = texts(running.take_output());
            assert!(
                output.contains(&"run 1 started".to_string()),
                "{name}: {output:?}"
            );
        }
    }

    #[test]
    fn holding_a_direction_moves_the_knight() {
        for name in EXAMPLES {
            let mut moved = start_playing(name);
            let mut still = start_playing(name);
            step(&mut moved, &[down(Button::Right)]);
            step(&mut still, &[]);
            idle(&mut moved, 20);
            idle(&mut still, 20);
            assert_ne!(moved.runtime.frame(), still.runtime.frame(), "{name}");
        }
    }

    #[test]
    fn holding_a_throws_axes() {
        for name in EXAMPLES {
            let mut throwing = start_playing(name);
            let mut still = start_playing(name);
            step(&mut throwing, &[down(Button::A)]);
            step(&mut still, &[]);
            idle(&mut throwing, 20);
            idle(&mut still, 20);
            assert!(sprite_count(&throwing) > sprite_count(&still), "{name}");
        }
    }

    #[test]
    fn an_idle_knight_falls_and_the_game_restarts_in_the_same_vm() {
        for name in EXAMPLES {
            let mut running = start_playing(name);
            let fell = (0..IDLE_DEATH_LIMIT).any(|_| {
                step(&mut running, &[]);
                shows(&running, "KILLS")
            });
            assert!(fell, "{name}: still alive after {IDLE_DEATH_LIMIT} ticks");
            let output = texts(running.take_output());
            assert!(
                output
                    .iter()
                    .any(|line| line.starts_with("signal lost: score")),
                "{name}: {output:?}"
            );

            // A is ignored briefly so a held button cannot skip the screen.
            step(&mut running, &[down(Button::A)]);
            step(&mut running, &[up(Button::A)]);
            assert!(shows(&running, "KILLS"), "{name}");

            // Once the game-over effects finish, no channel is left open.
            idle(&mut running, 80);
            let channels = *running.runtime.audio_mut().channels();
            assert!(
                channels.iter().all(|channel| !channel.gate),
                "{name}: {channels:?}"
            );

            step(&mut running, &[down(Button::A)]);
            step(&mut running, &[up(Button::A)]);
            assert!(!shows(&running, "KILLS"), "{name}");
            assert!(shows(&running, "SCORE 00000"), "{name}");
            assert!(
                texts(running.take_output()).contains(&"restart: run 2".to_string()),
                "{name}"
            );
        }
    }

    #[test]
    fn the_same_inputs_replay_the_same_game() {
        for name in EXAMPLES {
            let play = || {
                let mut running = start_playing(name);
                step(&mut running, &[down(Button::Right), down(Button::A)]);
                idle(&mut running, 60);
                step(&mut running, &[up(Button::Right), down(Button::Up)]);
                step(&mut running, &[down(Button::B)]);
                idle(&mut running, 120);
                let output = texts(running.take_output());
                (running.runtime.frame().to_vec(), output)
            };
            assert_eq!(play(), play(), "{name}");
        }
    }

    /// The port reuses the original's graphics and palettes unchanged.
    #[test]
    fn the_rhai_port_ships_the_original_assets() {
        let root = examples_dir();
        for file in [
            "sprites.png",
            "sprites.bin",
            "objattr.bin",
            "obp.bin",
            "palette.bin",
            "sprites.import.ron",
            "sprites.sprites.ron",
        ] {
            assert_eq!(
                fs::read(root.join(RHAI).join(file)).unwrap(),
                fs::read(root.join(LUAU).join(file)).unwrap(),
                "{file}"
            );
        }
    }

    /// A log entry as the lockstep test compares it. Rhai's `debug` shows
    /// strings quoted where Luau's `log.debug` does not. The quotes are the
    /// only difference the port is allowed.
    fn comparable(entries: Vec<LogEntry>, unquote_debug: bool) -> Vec<(LogLevel, String)> {
        entries
            .into_iter()
            .map(|entry| {
                let text = match entry.level {
                    LogLevel::Debug if unquote_debug => entry
                        .text
                        .strip_prefix('"')
                        .and_then(|text| text.strip_suffix('"'))
                        .map_or(entry.text.clone(), str::to_owned),
                    _ => entry.text,
                };
                (entry.level, text)
            })
            .collect()
    }

    /// Buttons held on tick `n` of a scripted session that walks a new
    /// direction every 20 ticks, taps A every other tick so the aim follows
    /// the walk, and dashes every 97 ticks. Tuned on the original so the
    /// first run reaches the ogre waves before the knight falls. The A taps
    /// also restart each run once the game-over lock ends.
    fn scripted_buttons(n: usize) -> Vec<Button> {
        const DIRECTIONS: [&[Button]; 8] = [
            &[Button::Right],
            &[Button::Right, Button::Down],
            &[Button::Down],
            &[Button::Down, Button::Left],
            &[Button::Left],
            &[Button::Left, Button::Up],
            &[Button::Up],
            &[Button::Up, Button::Right],
        ];
        let mut held = DIRECTIONS[(n / 20 * 3) % 8].to_vec();
        if n % 2 == 1 {
            held.push(Button::A);
        }
        if n.is_multiple_of(97) {
            held.push(Button::B);
        }
        held
    }

    /// The input events that turn tick `n - 1`'s scripted buttons into
    /// tick `n`'s.
    fn scripted_input(n: usize) -> Vec<InputEvent> {
        let before = n.checked_sub(1).map(scripted_buttons).unwrap_or_default();
        let now = scripted_buttons(n);
        let released = before.iter().filter(|b| !now.contains(b)).map(|&b| up(b));
        let pressed = now.iter().filter(|b| !before.contains(b)).map(|&b| down(b));
        released.chain(pressed).collect()
    }

    /// The Rhai port draws, sounds and logs exactly what the original does,
    /// tick by tick, through runs that reach the ogre waves, the game-over
    /// screen and restarts.
    #[test]
    fn the_rhai_port_plays_in_lockstep_with_the_original() {
        const TICKS: usize = 7_000;
        let mut luau = start(LUAU);
        let mut rhai = start(RHAI);
        assert_eq!(
            comparable(rhai.take_output(), true),
            comparable(luau.take_output(), false)
        );

        let mut ogres = 0;
        let mut runs = 0;
        for n in 0..TICKS {
            let input = scripted_input(n);
            luau.tick(&input).unwrap();
            rhai.tick(&input).unwrap();
            assert!(
                luau.runtime.frame() == rhai.runtime.frame(),
                "frames differ at tick {n}"
            );
            assert_eq!(
                *rhai.runtime.audio_mut(),
                *luau.runtime.audio_mut(),
                "audio differs at tick {n}"
            );
            let lines = comparable(luau.take_output(), false);
            assert_eq!(
                comparable(rhai.take_output(), true),
                lines,
                "logs differ at tick {n}"
            );

            ogres += lines
                .iter()
                .filter(|(_, text)| text.starts_with("an ogre entered"))
                .count();
            runs += lines
                .iter()
                .filter(|(_, text)| text.starts_with("signal lost: score"))
                .count();
            luau.runtime.audio_mut().commit();
            rhai.runtime.audio_mut().commit();
        }
        assert!(ogres > 0, "no ogre appeared");
        assert!(runs > 1, "{runs} runs ended");
    }
}
