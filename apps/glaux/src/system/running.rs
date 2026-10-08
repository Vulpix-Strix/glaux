//! A loaded cartridge together with the Luau VM running it.

use glaux_input::{InputEvent, InputLatch};
use glaux_luau::{Budget, DrawCommand, LogEntry, Runtime, RuntimeError, StartFailure};
use glaux_machine::font::BitmapFont;
use glaux_machine::framebuffer::Framebuffer;
use glaux_machine::obj_palette::{ObjPalettes, ObjPalettesError};
use glaux_machine::palette::{Palette, PaletteBytesError};
use glaux_machine::psg::Psg;
use glaux_machine::sprite::{
    ObjTileAttributes, ObjTileAttributesError, ObjTileBank, ObjTileBankError, ObjVram,
};
use glaux_machine::system_rom::SystemRom;
use glaux_video::draw::{
    ObjPaletteChoice, ObjSprite, clear, draw_obj_sprite, draw_text, fill_rect, pixel,
};

use super::audio;
use crate::cartridge::{CartridgeError, LoadedCartridge};

/// Why a cartridge could not start.
#[derive(Debug)]
pub enum StartError {
    /// The entry script could not be read.
    Cartridge(CartridgeError),
    /// A video asset was not valid native data.
    Video(VideoAssetError),
    /// The entry chunk or `boot()` failed.
    Script(StartFailure),
}

/// A cartridge video asset that is not valid native G1 data.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum VideoAssetError {
    #[error("INVALID SPRITES.BIN: {0}")]
    ObjTiles(#[from] ObjTileBankError),
    #[error("INVALID OBJATTR.BIN: {0}")]
    ObjAttributes(#[from] ObjTileAttributesError),
    #[error("INVALID OBP.BIN: {0}")]
    ObjPalettes(#[from] ObjPalettesError),
    #[error("INVALID PALETTE.BIN: {0}")]
    Palette(#[from] PaletteBytesError),
}

/// The video state a cartridge starts with for each asset it does not
/// supply, taken from the System ROM.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CartridgeVideoDefaults {
    palette: Palette,
    obj_palettes: ObjPalettes,
}

impl CartridgeVideoDefaults {
    pub fn from_rom(rom: &SystemRom) -> Self {
        Self {
            palette: rom.palette().clone(),
            obj_palettes: rom.obj_palettes().clone(),
        }
    }
}

/// A cartridge's video state, loaded once when it starts and unchanged for
/// the whole run, so replaying a frame always resolves the same colors.
#[derive(Debug)]
struct CartridgeVideo {
    /// The master palette the cartridge's frames are presented with.
    palette: Palette,
    obj: ObjVram,
}

impl CartridgeVideo {
    /// Reads the cartridge's optional video assets, falling back to
    /// `defaults` for those it lacks.
    fn load(
        cartridge: &LoadedCartridge,
        defaults: &CartridgeVideoDefaults,
    ) -> Result<Self, StartError> {
        let tiles = match cartridge.read_obj_tiles().map_err(StartError::Cartridge)? {
            Some(bytes) => ObjTileBank::from_bytes(&bytes).map_err(video_error)?,
            None => ObjTileBank::new(),
        };
        let attributes = match cartridge
            .read_obj_attributes()
            .map_err(StartError::Cartridge)?
        {
            Some(bytes) => ObjTileAttributes::from_bytes(&bytes).map_err(video_error)?,
            None => ObjTileAttributes::new(),
        };
        let palettes = match cartridge
            .read_obj_palettes()
            .map_err(StartError::Cartridge)?
        {
            Some(bytes) => ObjPalettes::from_bytes(&bytes).map_err(video_error)?,
            None => defaults.obj_palettes.clone(),
        };
        let palette = match cartridge
            .read_master_palette()
            .map_err(StartError::Cartridge)?
        {
            Some(bytes) => Palette::from_rgb888(&bytes).map_err(video_error)?,
            None => defaults.palette.clone(),
        };
        Ok(Self {
            palette,
            obj: ObjVram::new(tiles, attributes, palettes),
        })
    }
}

fn video_error(err: impl Into<VideoAssetError>) -> StartError {
    StartError::Video(err.into())
}

/// A cartridge that could not start, handed back still loaded.
#[derive(Debug)]
pub struct FailedStart {
    pub cartridge: LoadedCartridge,
    pub error: StartError,
}

/// A cartridge whose VM is alive. Dropping it disposes of the VM.
#[derive(Debug)]
pub struct RunningCartridge {
    cartridge: LoadedCartridge,
    runtime: Runtime,
    video: CartridgeVideo,
    /// Starts with every button and key released, so nothing held during a
    /// previous run reaches this one, and is reset whenever the system
    /// resumes the cartridge from a pause.
    input: InputLatch,
}

impl RunningCartridge {
    /// Creates a fresh VM for `cartridge` and runs its entry script and
    /// `boot()`, with `defaults` standing in for video assets the cartridge
    /// does not supply. On failure the cartridge is handed back, still
    /// loaded.
    pub fn start(
        cartridge: LoadedCartridge,
        defaults: &CartridgeVideoDefaults,
    ) -> Result<Self, Box<FailedStart>> {
        let source = match cartridge.entry_source() {
            Ok(source) => source,
            Err(err) => return Err(failed(cartridge, StartError::Cartridge(err))),
        };
        let video = match CartridgeVideo::load(&cartridge, defaults) {
            Ok(video) => video,
            Err(err) => return Err(failed(cartridge, err)),
        };
        let entry = &cartridge.manifest().runtime.entry;
        match Runtime::start(entry, &source, cartridge.modules(), Budget::default()) {
            Ok(runtime) => Ok(Self {
                cartridge,
                runtime,
                video,
                input: InputLatch::new(),
            }),
            Err(failure) => Err(failed(cartridge, StartError::Script(failure))),
        }
    }

    pub fn cartridge(&self) -> &LoadedCartridge {
        &self.cartridge
    }

    /// The master palette the cartridge's frames resolve to.
    pub fn palette(&self) -> &Palette {
        &self.video.palette
    }

    /// Advances the cartridge by one logical tick, applying the button and
    /// key events in `input` received since the previous tick.
    pub fn tick(&mut self, input: &[InputEvent]) -> Result<(), RuntimeError> {
        let state = self.input.advance(input);
        self.runtime.tick(state)
    }

    /// Releases every button and key as seen by the cartridge, so its next
    /// tick sees nothing held and no presses from before this call.
    pub fn reset_input(&mut self) {
        self.input = InputLatch::new();
    }

    /// Entries the cartridge logged since the last call.
    pub fn take_output(&mut self) -> Vec<LogEntry> {
        self.runtime.take_output()
    }

    /// Redraws the cartridge's last completed frame: a screen cleared to
    /// palette index 0, then its draw commands in the order they were made.
    pub fn render(&self, framebuffer: &mut Framebuffer, font: &BitmapFont) {
        clear(framebuffer, 0);
        for command in self.runtime.frame() {
            match *command {
                DrawCommand::Clear(color) => clear(framebuffer, color),
                DrawCommand::Pixel { x, y, color } => pixel(framebuffer, x, y, color),
                DrawCommand::Rect {
                    x,
                    y,
                    width,
                    height,
                    color,
                } => fill_rect(framebuffer, x, y, width, height, color),
                DrawCommand::Sprite {
                    tile,
                    x,
                    y,
                    width,
                    height,
                    flip_x,
                    flip_y,
                    palette,
                } => draw_obj_sprite(
                    framebuffer,
                    &self.video.obj,
                    &ObjSprite {
                        first: usize::from(tile),
                        x,
                        y,
                        width: usize::from(width),
                        height: usize::from(height),
                        flip_x,
                        flip_y,
                        palette: palette
                            .map_or(ObjPaletteChoice::TileDefault, ObjPaletteChoice::Bank),
                    },
                ),
                DrawCommand::Text {
                    ref text,
                    x,
                    y,
                    color,
                } => draw_text(framebuffer, font, text, x, y, color),
            }
        }
    }

    /// Writes the cartridge's audio into `psg` and commits it, as
    /// [`audio::apply`] describes.
    pub fn apply_audio(&mut self, psg: &mut Psg, fresh: bool) {
        audio::apply(&mut self.runtime.audio_mut(), psg, fresh);
    }

    /// Disposes of the VM and gives back the cartridge.
    pub fn stop(self) -> LoadedCartridge {
        self.cartridge
    }
}

fn failed(cartridge: LoadedCartridge, error: StartError) -> Box<FailedStart> {
    Box::new(FailedStart { cartridge, error })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use std::path::Path;

    use glaux_luau::ScriptError;
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::cartridge::fixture::write_cartridge;
    use crate::cartridge_fs::fixture::Fixture;

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

        let mut running = RunningCartridge::start(cartridge, &defaults()).unwrap();
        assert_eq!(texts(running.take_output()), ["enemies loaded"]);
        running.tick(&[]).unwrap();
        running.tick(&[]).unwrap();
        assert_eq!(texts(running.take_output()), ["6", "6"]);
    }

    #[test]
    fn module_errors_name_cartridge_relative_paths() {
        let fixture = Fixture::new("running-module-errors");
        let dir = fixture.root().join("pong");
        write_cartridge(&dir, "Pong");
        fs::write(dir.join("code/main.luau"), r#"require("boom")"#).unwrap();
        fs::write(dir.join("code/boom.luau"), "\nerror(\"x\")").unwrap();
        let cartridge = LoadedCartridge::load(&fixture.storage(), "pong").unwrap();

        let failed = RunningCartridge::start(cartridge, &defaults()).unwrap_err();
        let StartError::Script(failure) = failed.error else {
            panic!("expected a script error, got {:?}", failed.error);
        };
        assert_eq!(
            failure.error.error,
            ScriptError::Runtime("code/boom.luau:2: x".into())
        );

        fs::write(dir.join("code/main.luau"), r#"require("nope")"#).unwrap();
        let failed = RunningCartridge::start(failed.cartridge, &defaults()).unwrap_err();
        let StartError::Script(failure) = failed.error else {
            panic!("expected a script error, got {:?}", failed.error);
        };
        assert_eq!(
            failure.error.to_string(),
            "MODULE NOT FOUND: code/nope.luau (code/main.luau:1)"
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
        let mut palette = *Palette::provisional().entries();
        palette[200] = glaux_machine::palette::Rgb::from_hex(0x123456);
        let palette = Palette::new(palette);
        fs::write(dir.join("palette.bin"), palette.to_rgb888()).unwrap();
        let cartridge = LoadedCartridge::load(&fixture.storage(), "pong").unwrap();

        let mut running = RunningCartridge::start(cartridge, &defaults()).unwrap();
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
        let running = RunningCartridge::start(cartridge, &defaults()).unwrap();
        assert_eq!(running.palette(), rom.palette());
        assert_eq!(running.video.obj.palettes(), rom.obj_palettes());
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
        let mut running = RunningCartridge::start(cartridge, &defaults()).unwrap();
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

    /// Signal Lost (`examples/signal-lost`), driven through the real loader,
    /// module source and object-tile validation. Assertions look at drawn
    /// text and command counts rather than sprite tiles or coordinates.
    mod signal_lost {
        use std::path::Path;

        use glaux_input::Button;
        use pretty_assertions::assert_eq;

        use super::*;
        use crate::cartridge_fs::{CartridgeFs, CartridgeStorage};

        /// Ticks after starting a run after which an idle knight must have
        /// fallen; about two minutes of play.
        const IDLE_DEATH_LIMIT: usize = 7200;

        fn start() -> RunningCartridge {
            let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
            let root = fs::canonicalize(examples).unwrap();
            let storage = CartridgeStorage::Available(CartridgeFs::new(root));
            let cartridge = LoadedCartridge::load(&storage, "signal-lost").unwrap();
            RunningCartridge::start(cartridge, &defaults()).unwrap()
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
        fn start_playing() -> RunningCartridge {
            let mut running = start();
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
            let mut running = start();
            assert_eq!(texts(running.take_output()), ["Signal Lost ready"]);

            idle(&mut running, 120);
            assert!(shows(&running, "SIGNAL LOST"));
            assert!(shows(&running, "X DASH"));
            assert!(sprite_count(&running) > 0);
            assert!(
                running
                    .runtime
                    .frame()
                    .iter()
                    .any(|command| matches!(command, DrawCommand::Rect { .. }))
            );

            let rom = crate::rom::load().unwrap();
            let mut framebuffer = Framebuffer::new();
            running.render(&mut framebuffer, rom.font());
            let first = framebuffer.as_bytes()[0];
            assert!(framebuffer.as_bytes().iter().any(|&index| index != first));
        }

        #[test]
        fn the_title_knight_faces_left_by_flipping() {
            let mut running = start();
            idle(&mut running, 1);
            let knight = running
                .runtime
                .frame()
                .iter()
                .find_map(|command| match *command {
                    DrawCommand::Sprite {
                        x: 196,
                        y: 116,
                        flip_x,
                        flip_y,
                        ..
                    } => Some((flip_x, flip_y)),
                    _ => None,
                });
            assert_eq!(knight, Some((true, false)));
        }

        #[test]
        fn pressing_a_starts_a_run_with_a_hud() {
            let mut running = start_playing();
            assert!(!shows(&running, "PRESS Z TO START"));
            assert!(shows(&running, "SCORE 00000"));
            assert!(shows(&running, "TIME"));
            let band = DrawCommand::Rect {
                x: 0,
                y: 0,
                width: 320,
                height: 16,
                color: 11,
            };
            assert!(running.runtime.frame().contains(&band));

            let rom = crate::rom::load().unwrap();
            let mut framebuffer = Framebuffer::new();
            running.render(&mut framebuffer, rom.font());
            assert_eq!(framebuffer.pixel(150, 14), Some(11));

            let output = texts(running.take_output());
            assert!(output.contains(&"run 1 started".to_string()), "{output:?}");
        }

        #[test]
        fn holding_a_direction_moves_the_knight() {
            let mut moved = start_playing();
            let mut still = start_playing();
            step(&mut moved, &[down(Button::Right)]);
            step(&mut still, &[]);
            idle(&mut moved, 20);
            idle(&mut still, 20);
            assert_ne!(moved.runtime.frame(), still.runtime.frame());
        }

        #[test]
        fn holding_a_throws_axes() {
            let mut throwing = start_playing();
            let mut still = start_playing();
            step(&mut throwing, &[down(Button::A)]);
            step(&mut still, &[]);
            idle(&mut throwing, 20);
            idle(&mut still, 20);
            assert!(sprite_count(&throwing) > sprite_count(&still));
        }

        #[test]
        fn an_idle_knight_falls_and_the_game_restarts_in_the_same_vm() {
            let mut running = start_playing();
            let fell = (0..IDLE_DEATH_LIMIT).any(|_| {
                step(&mut running, &[]);
                shows(&running, "KILLS")
            });
            assert!(fell, "still alive after {IDLE_DEATH_LIMIT} ticks");
            let output = texts(running.take_output());
            assert!(
                output
                    .iter()
                    .any(|line| line.starts_with("signal lost: score")),
                "{output:?}"
            );

            // A is ignored briefly so a held button cannot skip the screen.
            step(&mut running, &[down(Button::A)]);
            step(&mut running, &[up(Button::A)]);
            assert!(shows(&running, "KILLS"));

            // Once the game-over effects finish, no channel is left open.
            idle(&mut running, 80);
            let channels = *running.runtime.audio_mut().channels();
            assert!(channels.iter().all(|channel| !channel.gate), "{channels:?}");

            step(&mut running, &[down(Button::A)]);
            step(&mut running, &[up(Button::A)]);
            assert!(!shows(&running, "KILLS"));
            assert!(shows(&running, "SCORE 00000"));
            assert!(texts(running.take_output()).contains(&"restart: run 2".to_string()));
        }

        #[test]
        fn the_same_inputs_replay_the_same_game() {
            let play = || {
                let mut running = start_playing();
                step(&mut running, &[down(Button::Right), down(Button::A)]);
                idle(&mut running, 60);
                step(&mut running, &[up(Button::Right), down(Button::Up)]);
                step(&mut running, &[down(Button::B)]);
                idle(&mut running, 120);
                let output = texts(running.take_output());
                (running.runtime.frame().to_vec(), output)
            };
            assert_eq!(play(), play());
        }
    }
}
