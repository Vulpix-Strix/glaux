use super::*;
use pretty_assertions::assert_eq;
use proptest::prelude::*;

const RED_HUE: Hsv = Hsv { h: 0, s: 0, v: 0 };

fn hsv(h: u16, s: u8, v: u8) -> Hsv {
    Hsv { h, s, v }
}

proptest! {
    /// Typed RGB values go through the picker's HSV state; they must come
    /// back unchanged.
    #[test]
    fn rgb_survives_a_trip_through_hsv(r: u8, g: u8, b: u8, h in 0..HUE_STEPS, s: u8, v: u8) {
        let rgb = Rgb { r, g, b };
        prop_assert_eq!(Hsv::from_rgb(rgb, hsv(h, s, v)).to_rgb(), rgb);
    }
}

#[test]
fn the_six_primaries_and_secondaries_sit_at_sixths_of_the_circle() {
    let at = |sixth: u16| hsv(sixth * 256, 255, 255).to_rgb();
    assert_eq!(
        [at(0), at(1), at(2), at(3), at(4), at(5)],
        [
            Rgb::from_hex(0xff0000),
            Rgb::from_hex(0xffff00),
            Rgb::from_hex(0x00ff00),
            Rgb::from_hex(0x00ffff),
            Rgb::from_hex(0x0000ff),
            Rgb::from_hex(0xff00ff),
        ]
    );
    assert_eq!(
        hsv(HUE_STEPS, 255, 255).to_rgb(),
        at(0),
        "the hue wraps around"
    );
}

#[test]
fn saturation_and_value_fade_to_grey_and_black() {
    assert_eq!(hsv(700, 0, 0x80).to_rgb(), Rgb::from_hex(0x808080));
    assert_eq!(hsv(700, 255, 0).to_rgb(), Rgb::from_hex(0x000000));
    // Half saturation of pure blue at full value: the other channels sit
    // halfway up.
    assert_eq!(hsv(4 * 256, 128, 255).to_rgb(), Rgb::from_hex(0x7f7fff));
}

#[test]
fn greys_keep_the_previous_hue_and_black_the_previous_saturation() {
    let previous = hsv(900, 200, 90);
    assert_eq!(
        Hsv::from_rgb(Rgb::from_hex(0x404040), previous),
        hsv(900, 0, 0x40)
    );
    assert_eq!(
        Hsv::from_rgb(Rgb::from_hex(0x000000), previous),
        hsv(900, 200, 0)
    );
}

#[test]
fn a_colored_rgb_sets_every_component() {
    assert_eq!(
        Hsv::from_rgb(Rgb::from_hex(0x00ff00), RED_HUE),
        hsv(2 * 256, 255, 255)
    );
    // Magenta's neighbour toward red stays below the wrap.
    let near_red = Hsv::from_rgb(Rgb::from_hex(0xff0001), RED_HUE);
    assert_eq!((near_red.s, near_red.v), (255, 255));
    assert!(near_red.h > 5 * 256, "{near_red:?}");
}

#[test]
fn hex_is_six_digits_with_an_optional_hash_in_either_case() {
    assert_eq!(parse_hex("#4a9BcD"), Some(Rgb::from_hex(0x4a9bcd)));
    assert_eq!(parse_hex("4A9BCD"), Some(Rgb::from_hex(0x4a9bcd)));
    for bad in [
        "", "#", "#4A9BC", "#4A9BCD0", "##4A9BC", "#4A9BCG", "+4A9BC", "#4A 9BC",
    ] {
        assert_eq!(parse_hex(bad), None, "{bad:?}");
    }
    assert_eq!(format_hex(Rgb::from_hex(0x0a0b0c)), "#0A0B0C");
}
