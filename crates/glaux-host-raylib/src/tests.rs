use super::*;
use glaux_machine::framebuffer::Framebuffer;
use glaux_machine::palette::{DEBUG_MAGENTA, Palette, ZUGHY_32};
use pretty_assertions::assert_eq;

fn rect(r: Rectangle) -> (f32, f32, f32, f32) {
    (r.x, r.y, r.width, r.height)
}

#[test]
fn resolves_indices_to_opaque_rgba() {
    let mut fb = Framebuffer::new();
    fb.clear(3);
    fb.set_pixel(0, 0, 31);
    fb.set_pixel(1, 0, 200);
    let palette = Palette::provisional();
    let mut out = vec![0; RGBA_LEN];

    resolve_rgba(&IndexedFrame::new(&fb, &palette), &mut out);

    let c31 = ZUGHY_32[31];
    let c3 = ZUGHY_32[3];
    let m = DEBUG_MAGENTA;
    assert_eq!(out[0..4], [c31.r, c31.g, c31.b, 255]);
    assert_eq!(out[4..8], [m.r, m.g, m.b, 255]);
    assert_eq!(out[8..12], [c3.r, c3.g, c3.b, 255]);
    assert_eq!(out[RGBA_LEN - 4..], [c3.r, c3.g, c3.b, 255]);
}

#[test]
fn fit_exact_4_3_window_fills_it() {
    assert_eq!(rect(fit_4_3(960, 720)), (0.0, 0.0, 960.0, 720.0));
}

#[test]
fn fit_wide_window_is_pillarboxed_and_centered() {
    assert_eq!(rect(fit_4_3(1280, 720)), (160.0, 0.0, 960.0, 720.0));
}

#[test]
fn fit_tall_window_is_letterboxed_and_centered() {
    assert_eq!(rect(fit_4_3(640, 800)), (0.0, 160.0, 640.0, 480.0));
}

#[test]
fn centers_on_monitor_to_the_right() {
    assert_eq!(
        center_on_monitor(1920, 0, 2560, 1440, 960, 720),
        (2720, 360)
    );
}

#[test]
fn centers_on_monitor_at_negative_offset() {
    assert_eq!(
        center_on_monitor(-1280, -200, 1280, 1024, 960, 720),
        (-1120, -48)
    );
}

#[test]
fn window_larger_than_monitor_overhangs_evenly() {
    assert_eq!(center_on_monitor(0, 0, 800, 600, 960, 720), (-80, -60));
}

#[test]
fn monitors_without_geometry_are_skipped() {
    assert_eq!(
        MonitorArea::new(-1920, 0, 1920, 1080),
        Some(MonitorArea {
            x: -1920,
            y: 0,
            width: 1920,
            height: 1080
        })
    );
    assert_eq!(MonitorArea::new(0, 0, 0, 1080), None);
    assert_eq!(MonitorArea::new(0, 0, 1920, -1), None);
}

#[test]
fn fit_degenerate_window_is_empty() {
    assert_eq!(rect(fit_4_3(0, 0)), (0.0, 0.0, 0.0, 0.0));
}
