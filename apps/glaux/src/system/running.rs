//! A loaded cartridge together with the Luau VM running it.

use glaux_input::{ButtonLatch, InputEvent};
use glaux_luau::{Budget, DrawCommand, LogEntry, Runtime, RuntimeError, StartFailure};
use glaux_machine::font::BitmapFont;
use glaux_machine::framebuffer::Framebuffer;
use glaux_machine::psg::Psg;
use glaux_machine::sprite::{ObjTileBank, ObjTileBankError};
use glaux_video::draw::{clear, draw_obj_sprite, draw_text, pixel};

use super::audio;
use crate::cartridge::{CartridgeError, LoadedCartridge};

/// Why a cartridge could not start.
#[derive(Debug)]
pub enum StartError {
    /// The entry script could not be read.
    Cartridge(CartridgeError),
    /// `sprites.bin` was not valid native object-tile data.
    ObjTiles(ObjTileBankError),
    /// The entry chunk or `boot()` failed.
    Script(StartFailure),
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
    obj_tiles: ObjTileBank,
    /// Starts with every button released, so nothing held during a previous
    /// run reaches this one, and is reset whenever the system resumes the
    /// cartridge from a pause.
    buttons: ButtonLatch,
}

impl RunningCartridge {
    /// Creates a fresh VM for `cartridge` and runs its entry script and
    /// `boot()`. On failure the cartridge is handed back, still loaded.
    pub fn start(cartridge: LoadedCartridge) -> Result<Self, Box<FailedStart>> {
        let source = match cartridge.entry_source() {
            Ok(source) => source,
            Err(err) => return Err(failed(cartridge, StartError::Cartridge(err))),
        };
        let obj_bytes = match cartridge.read_obj_tiles() {
            Ok(bytes) => bytes,
            Err(err) => return Err(failed(cartridge, StartError::Cartridge(err))),
        };
        let obj_tiles = match obj_bytes {
            Some(bytes) => match ObjTileBank::from_bytes(&bytes) {
                Ok(tiles) => tiles,
                Err(err) => return Err(failed(cartridge, StartError::ObjTiles(err))),
            },
            None => ObjTileBank::new(),
        };
        let entry = &cartridge.manifest().runtime.entry;
        match Runtime::start(entry, &source, cartridge.modules(), Budget::default()) {
            Ok(runtime) => Ok(Self {
                cartridge,
                runtime,
                obj_tiles,
                buttons: ButtonLatch::new(),
            }),
            Err(failure) => Err(failed(cartridge, StartError::Script(failure))),
        }
    }

    pub fn cartridge(&self) -> &LoadedCartridge {
        &self.cartridge
    }

    /// Advances the cartridge by one logical tick, applying the button
    /// events in `input` received since the previous tick.
    pub fn tick(&mut self, input: &[InputEvent]) -> Result<(), RuntimeError> {
        let buttons = self.buttons.advance(input);
        self.runtime.tick(buttons)
    }

    /// Releases every game button as seen by the cartridge, so its next tick
    /// sees no held buttons and no presses from before this call.
    pub fn reset_input(&mut self) {
        self.buttons = ButtonLatch::new();
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
                DrawCommand::Sprite {
                    tile,
                    x,
                    y,
                    width,
                    height,
                } => draw_obj_sprite(
                    framebuffer,
                    &self.obj_tiles,
                    usize::from(tile),
                    x,
                    y,
                    usize::from(width),
                    usize::from(height),
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

    use glaux_luau::ScriptError;
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::cartridge::fixture::write_cartridge;
    use crate::cartridge_fs::fixture::Fixture;

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

        let mut running = RunningCartridge::start(cartridge).unwrap();
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

        let failed = RunningCartridge::start(cartridge).unwrap_err();
        let StartError::Script(failure) = failed.error else {
            panic!("expected a script error, got {:?}", failed.error);
        };
        assert_eq!(
            failure.error.error,
            ScriptError::Runtime("code/boom.luau:2: x".into())
        );

        fs::write(dir.join("code/main.luau"), r#"require("nope")"#).unwrap();
        let failed = RunningCartridge::start(failed.cartridge).unwrap_err();
        let StartError::Script(failure) = failed.error else {
            panic!("expected a script error, got {:?}", failed.error);
        };
        assert_eq!(
            failure.error.to_string(),
            "MODULE NOT FOUND: code/nope.luau (code/main.luau:1)"
        );
    }

    /// Signal Lost (`examples/signal-lost`), driven through the real loader,
    /// module source and object-tile validation. Assertions look at drawn
    /// text and command counts rather than sprite tiles or coordinates.
    mod signal_lost {
        use std::path::Path;

        use glaux_input::GameButton;
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
            RunningCartridge::start(cartridge).unwrap()
        }

        fn down(button: GameButton) -> InputEvent {
            InputEvent::ButtonDown(button)
        }

        fn up(button: GameButton) -> InputEvent {
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
            step(&mut running, &[down(GameButton::A)]);
            step(&mut running, &[up(GameButton::A)]);
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
            assert!(shows(&running, "B DASH"));
            assert!(sprite_count(&running) > 0);
            assert!(
                running
                    .runtime
                    .frame()
                    .iter()
                    .any(|command| matches!(command, DrawCommand::Pixel { .. }))
            );

            let rom = crate::rom::load().unwrap();
            let mut framebuffer = Framebuffer::new();
            running.render(&mut framebuffer, rom.font());
            let first = framebuffer.as_bytes()[0];
            assert!(framebuffer.as_bytes().iter().any(|&index| index != first));
        }

        #[test]
        fn pressing_a_starts_a_run_with_a_hud() {
            let mut running = start_playing();
            assert!(!shows(&running, "PRESS A TO START"));
            assert!(shows(&running, "SCORE 00000"));
            assert!(shows(&running, "TIME"));
            let output = texts(running.take_output());
            assert!(output.contains(&"run 1 started".to_string()), "{output:?}");
        }

        #[test]
        fn holding_a_direction_moves_the_knight() {
            let mut moved = start_playing();
            let mut still = start_playing();
            step(&mut moved, &[down(GameButton::Right)]);
            step(&mut still, &[]);
            idle(&mut moved, 20);
            idle(&mut still, 20);
            assert_ne!(moved.runtime.frame(), still.runtime.frame());
        }

        #[test]
        fn holding_a_throws_axes() {
            let mut throwing = start_playing();
            let mut still = start_playing();
            step(&mut throwing, &[down(GameButton::A)]);
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
            step(&mut running, &[down(GameButton::A)]);
            step(&mut running, &[up(GameButton::A)]);
            assert!(shows(&running, "KILLS"));

            // Once the game-over effects finish, no channel is left open.
            idle(&mut running, 80);
            let channels = *running.runtime.audio_mut().channels();
            assert!(channels.iter().all(|channel| !channel.gate), "{channels:?}");

            step(&mut running, &[down(GameButton::A)]);
            step(&mut running, &[up(GameButton::A)]);
            assert!(!shows(&running, "KILLS"));
            assert!(shows(&running, "SCORE 00000"));
            assert!(texts(running.take_output()).contains(&"restart: run 2".to_string()));
        }

        #[test]
        fn the_same_inputs_replay_the_same_game() {
            let play = || {
                let mut running = start_playing();
                step(
                    &mut running,
                    &[down(GameButton::Right), down(GameButton::A)],
                );
                idle(&mut running, 60);
                step(&mut running, &[up(GameButton::Right), down(GameButton::Up)]);
                step(&mut running, &[down(GameButton::B)]);
                idle(&mut running, 120);
                let output = texts(running.take_output());
                (running.runtime.frame().to_vec(), output)
            };
            assert_eq!(play(), play());
        }
    }
}
