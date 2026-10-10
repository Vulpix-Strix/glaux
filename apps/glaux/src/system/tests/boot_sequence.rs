use super::*;
use pretty_assertions::assert_eq;

#[test]
fn frame_n_is_reached_after_n_updates() {
    let mut system = new_system();
    for n in 1..=10 {
        system = system.update(&[]);
        assert_boot_frame(&system, n);
    }
}

#[test]
fn boot_stages_advance_in_order_at_their_frames() {
    let mut system = new_system();
    let mut seen = vec![(0, BootStage::Intro)];
    let mut frame = 0;

    while let SystemState::Boot { boot, .. } = system.state() {
        let stage = boot.stage();
        if seen.last().map(|&(_, s)| s) != Some(stage) {
            seen.push((frame, stage));
        }
        system = system.update(&[]);
        frame += 1;
    }

    assert_eq!(
        seen,
        [
            (0, BootStage::Intro),
            (boot::HEADER_FRAME, BootStage::Header),
            (boot::VIDEO_FRAME, BootStage::Video),
            (boot::OBJ_FRAME, BootStage::Obj),
            (boot::AUDIO_FRAME, BootStage::Audio),
            (boot::READY_FRAME, BootStage::Ready),
        ]
    );
    assert_eq!(frame, boot::FINISH_FRAME);
}

#[test]
fn switches_to_shell_exactly_at_finish_frame() {
    let mut system = new_system();
    for _ in 0..boot::FINISH_FRAME - 1 {
        system = system.update(&[]);
    }
    assert!(matches!(system.state(), SystemState::Boot { .. }));

    system = system.update(&[]);
    assert_eq!(shell_of(&system), &Shell::new(info(), cartridges()));

    let mut expected = Shell::new(info(), cartridges());
    for _ in 0..100 {
        system = system.update(&[]);
        expected.tick();
    }
    assert_eq!(shell_of(&system), &expected);
}

/// A background-filled frame showing each `(row, text, color)` in the fake
/// ROM font, where every character is a 3×3 block, with one cell of margin
/// above and to the left of the text grid.
fn text_screen(rows: &[(i32, &str, u8)]) -> Framebuffer {
    let mut fb = Framebuffer::new();
    fb.clear(BACKGROUND);
    for &(row, text, color) in rows {
        for (col, _) in (0..).zip(text.chars()) {
            let (x, y) = ((col + 1) * CELL_W, (row + 1) * CELL_H);
            for (dx, dy) in (0..3).flat_map(|dy| (0..3).map(move |dx| (dx, dy))) {
                fb.set_pixel(x + dx, y + dy, color);
            }
        }
    }
    fb
}

#[test]
fn boot_header_renders_at_its_position() {
    assert_frame_eq!(
        render(&boot_at(BootStage::Header)),
        text_screen(&[
            (0, "GLAUX G1  SYSTEM ROM 0.1.0", TEXT),
            (1, "(C) 2026 TEST", DIM),
        ])
    );
}

#[test]
fn intro_renders_through_the_intro_renderer() {
    let rom = fake_rom();
    for frame in [0, boot::INTRO_FRAMES / 2, boot::INTRO_FRAMES - 1] {
        let mut system = new_system();
        for _ in 0..frame {
            system = system.update(&[]);
        }
        let mut expected = Framebuffer::new();
        boot::render_intro(&mut expected, rom.font(), rom.version(), frame);
        assert_frame_eq!(render(&system), expected, "frame {frame}");
    }
}

#[test]
fn video_line_appears_at_video_stage() {
    let video_row = boot::lines(BootStage::Video, &fake_rom())
        .into_iter()
        .find(|line| line.text.starts_with("VIDEO"))
        .unwrap()
        .row;

    assert!(!row_has_ink(
        &render(&boot_at(BootStage::Header)),
        video_row
    ));
    assert!(row_has_ink(&render(&boot_at(BootStage::Video)), video_row));
}

#[test]
fn input_during_boot_is_ignored() {
    let mut system = new_system();
    for n in 1..=10 {
        system = system.update(&[InputEvent::Text('A'), InputEvent::Enter]);
        assert_boot_frame(&system, n);
    }
}

fn assert_boot_frame(system: &System, frame: u32) {
    match system.state() {
        SystemState::Boot { boot, .. } => assert_eq!(boot, &Boot::at_frame(frame)),
        other => panic!("expected boot, got {other:?}"),
    }
}
