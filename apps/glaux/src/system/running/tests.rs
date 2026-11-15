use std::fs;

use glaux_luau::ScriptError;
use glaux_machine::obj_palette::{ObjBank, ObjPalettes};
use glaux_machine::registers::GraphicsRegisters;
use pretty_assertions::assert_eq;

use super::*;
use crate::cartridge::fixture::write_cartridge;
use crate::cartridge_fs::fixture::Fixture;
use crate::system::video::VideoOverrides;

mod composition;
mod debugging;
mod maps;
mod memory;
mod publication;

fn defaults() -> CartridgeVideoDefaults {
    CartridgeVideoDefaults::from_rom(&crate::rom::load().unwrap())
}

fn font() -> BitmapFont {
    crate::rom::load().unwrap().font().clone()
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
        RunningCartridge::start(cartridge, &defaults(), VideoOverrides::default(), &font())
            .unwrap();
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

    let failed =
        RunningCartridge::start(cartridge, &defaults(), VideoOverrides::default(), &font())
            .unwrap_err();
    let StartError::Script(failure) = failed.error else {
        panic!("expected a script error, got {:?}", failed.error);
    };
    assert_eq!(
        failure.error.error,
        ScriptError::Runtime("code/boom.luau:2: x".into())
    );

    fs::write(dir.join("code/main.luau"), r#"require("nope")"#).unwrap();
    let failed = RunningCartridge::start(
        failed.cartridge,
        &defaults(),
        VideoOverrides::default(),
        &font(),
    )
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
        RunningCartridge::start(cartridge, &defaults(), VideoOverrides::default(), &font())
            .unwrap();
    running.tick(&[]).unwrap();
    let mut framebuffer = Framebuffer::new();
    running.render(&mut framebuffer);

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
        RunningCartridge::start(cartridge, &defaults(), VideoOverrides::default(), &font())
            .unwrap();
    assert_eq!(running.palette(), rom.palette());
    assert_eq!(running.hardware().obj.palettes(), rom.obj_palettes());
}

#[test]
fn hardware_state_survives_ticks_and_each_run_starts_from_the_initial_values() {
    let fixture = Fixture::new("running-hardware-lifecycle");
    let dir = fixture.root().join("pong");
    write_cartridge(&dir, "Pong");
    let load = || LoadedCartridge::load(&fixture.storage(), "pong").unwrap();

    let mut running =
        RunningCartridge::start(load(), &defaults(), VideoOverrides::default(), &font()).unwrap();
    running.hardware_mut().write(0x00100, 0xAB).unwrap();
    running.hardware_mut().write(0x34000, 0x12).unwrap();
    running.hardware_mut().write(0x3B000, 0xFF).unwrap();
    running.hardware_mut().write16(0x3C010, 600).unwrap();
    running.tick(&[]).unwrap();
    running.tick(&[]).unwrap();
    assert_eq!(running.hardware().read(0x00100), Ok(0xAB));
    assert_eq!(running.hardware().read(0x34000), Ok(0x12));
    assert_eq!(running.hardware().read(0x3B000), Ok(0xFF));
    assert_eq!(running.hardware().read16(0x3C010), Ok(600));

    let restarted = RunningCartridge::start(
        running.stop(),
        &defaults(),
        VideoOverrides::default(),
        &font(),
    )
    .unwrap();
    let hardware = restarted.hardware();
    assert!(hardware.work_ram.iter().all(|&byte| byte == 0));
    assert!(hardware.oam.as_bytes().iter().all(|&byte| byte == 0));
    assert!(
        hardware
            .tilemaps
            .iter()
            .all(|window| window.as_bytes().iter().all(|&byte| byte == 0))
    );
    assert_eq!(hardware.registers, GraphicsRegisters::new());
}

/// The address of the run's hardware, to tell whether it is the same one.
fn hardware_address(running: &RunningCartridge) -> Option<*const CartridgeHardware> {
    running.hardware.as_deref().map(std::ptr::from_ref)
}

#[test]
fn the_hardware_stays_the_same_allocation_across_ticks_and_failures() {
    let fixture = Fixture::new("running-hardware-identity");
    let dir = fixture.root().join("pong");
    write_cartridge(&dir, "Pong");
    fs::write(
        dir.join("code/main.luau"),
        "n = 0\nfunction tick()\n  n = n + 1\n  if n == 3 then error(\"boom\") end\nend",
    )
    .unwrap();
    let cartridge = LoadedCartridge::load(&fixture.storage(), "pong").unwrap();
    let mut running =
        RunningCartridge::start(cartridge, &defaults(), VideoOverrides::default(), &font())
            .unwrap();
    let before = hardware_address(&running);
    assert!(before.is_some());

    for _ in 0..2 {
        running.advance(&[]).unwrap();
        assert_eq!(hardware_address(&running), before);
    }
    let error = running.advance(&[]).unwrap_err();
    assert_eq!(error.phase, Phase::Tick);
    assert_eq!(hardware_address(&running), before);
}

#[test]
fn a_phase_without_hardware_is_an_error_that_runs_no_script() {
    let fixture = Fixture::new("running-hardware-missing");
    let dir = fixture.root().join("pong");
    write_cartridge(&dir, "Pong");
    fs::write(
        dir.join("code/main.luau"),
        "function tick() log.info(\"tick\") end\nfunction draw() log.info(\"draw\") end",
    )
    .unwrap();
    let cartridge = LoadedCartridge::load(&fixture.storage(), "pong").unwrap();
    let mut running =
        RunningCartridge::start(cartridge, &defaults(), VideoOverrides::default(), &font())
            .unwrap();
    running.hardware = None;

    let error = running.advance(&[]).unwrap_err();
    assert_eq!(error.phase, Phase::Tick);
    assert!(error.to_string().contains("internal error"), "{error}");
    assert!(running.take_output().is_empty());
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
        RunningCartridge::start(cartridge, &defaults(), VideoOverrides::default(), &font())
            .unwrap();
    running.tick(&[]).unwrap();
    let rom = crate::rom::load().unwrap();
    let mut framebuffer = Framebuffer::new();
    running.render(&mut framebuffer);

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
