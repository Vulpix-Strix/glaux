use super::*;
use pretty_assertions::assert_eq;
use proptest::prelude::*;

const MAX_X: i32 = WIDTH as i32 - 1;
const MAX_Y: i32 = HEIGHT as i32 - 1;

// The resolution is part of the machine format: cartridges address pixels by
// it and the host presents exactly this many bytes.
#[test]
fn the_machine_format_is_320_by_240_bytes() {
    assert_eq!((WIDTH, HEIGHT, LEN), (320, 240, 76_800));
}

#[test]
fn starts_cleared_to_zero() {
    assert!(Framebuffer::new().as_bytes().iter().all(|&b| b == 0));
}

#[test]
fn clear_fills_every_pixel() {
    let mut fb = Framebuffer::new();
    fb.set_pixel(10, 10, 3);
    fb.clear(17);
    assert!(fb.as_bytes().iter().all(|&b| b == 17));
}

#[test]
fn reads_back_written_pixel() {
    let mut fb = Framebuffer::new();
    assert!(fb.set_pixel(42, 17, 9));
    assert_eq!(fb.pixel(42, 17), Some(9));
    assert_eq!(fb.as_bytes()[17 * WIDTH + 42], 9);
}

#[test]
fn writes_and_reads_corners() {
    let mut fb = Framebuffer::new();
    assert!(fb.set_pixel(0, 0, 1));
    assert!(fb.set_pixel(MAX_X, MAX_Y, 2));
    assert_eq!(fb.pixel(0, 0), Some(1));
    assert_eq!(fb.pixel(MAX_X, MAX_Y), Some(2));
    assert_eq!(fb.as_bytes()[0], 1);
    assert_eq!(fb.as_bytes()[LEN - 1], 2);
}

#[test]
fn out_of_range_coordinates_are_clipped() {
    let mut fb = Framebuffer::new();
    fb.clear(5);
    let before = fb.clone();

    let outside = [
        (-1, 0),
        (0, -1),
        (WIDTH as i32, 0),
        (0, HEIGHT as i32),
        (WIDTH as i32, HEIGHT as i32),
        (i32::MIN, i32::MIN),
        (i32::MAX, i32::MAX),
        (i32::MIN, 0),
        (0, i32::MAX),
    ];
    for (x, y) in outside {
        assert_eq!(fb.pixel(x, y), None, "read at ({x}, {y})");
        assert!(!fb.set_pixel(x, y, 9), "write at ({x}, {y})");
    }

    assert!(fb == before, "a rejected access changed the framebuffer");
}

// The signature alone allows a mutable view of some other array.
#[test]
fn mutable_bytes_are_the_pixels() {
    let mut fb = Framebuffer::new();
    fb.as_bytes_mut()[17 * WIDTH + 42] = 9;
    assert_eq!(fb.pixel(42, 17), Some(9));
}

#[test]
fn write_does_not_touch_neighbors() {
    let mut fb = Framebuffer::new();
    fb.set_pixel(100, 100, 7);

    for dy in -1..=1 {
        for dx in -1..=1 {
            let expected = if dx == 0 && dy == 0 { 7 } else { 0 };
            assert_eq!(fb.pixel(100 + dx, 100 + dy), Some(expected));
        }
    }
    assert_eq!(fb.as_bytes().iter().filter(|&&b| b != 0).count(), 1);
}

/// Coordinates biased toward the screen edges, plus the full `i32` range.
fn coordinate(len: usize) -> impl Strategy<Value = i32> {
    prop_oneof![-2..len as i32 + 2, any::<i32>()]
}

proptest! {
    // A write lands on the byte the row-major format gives its coordinates
    // when they are on screen, and changes nothing otherwise.
    #[test]
    fn a_write_changes_only_its_own_pixel(
        x in coordinate(WIDTH),
        y in coordinate(HEIGHT),
        color in 1..=u8::MAX,
    ) {
        let mut fb = Framebuffer::new();
        let on_screen = (0..WIDTH as i32).contains(&x) && (0..HEIGHT as i32).contains(&y);

        prop_assert_eq!(fb.set_pixel(x, y, color), on_screen);
        prop_assert_eq!(fb.pixel(x, y), on_screen.then_some(color));
        let bytes = fb.as_bytes();
        let changed: Vec<usize> = (0..LEN).filter(|&i| bytes[i] != 0).collect();
        let expected = if on_screen {
            vec![y as usize * WIDTH + x as usize]
        } else {
            vec![]
        };
        prop_assert_eq!(changed, expected);
    }
}
