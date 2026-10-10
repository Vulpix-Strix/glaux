//! Hue, saturation and value in integer steps, for picking colors by eye,
//! and the `#RRGGBB` notation for typing them.

use glaux_machine::palette::Rgb;

/// Steps in one turn of the hue circle: 256 per sixth, so each sixth
/// interpolates one channel over a full byte.
pub const HUE_STEPS: u16 = 6 * 256;

/// A color as hue (`0..HUE_STEPS`, red at 0, then yellow, green, cyan,
/// blue, magenta), saturation and value (`0..=255`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hsv {
    pub h: u16,
    pub s: u8,
    pub v: u8,
}

impl Hsv {
    /// The RGB color, rounded to the nearest byte per channel. A hue at or
    /// past [`HUE_STEPS`] wraps around.
    pub fn to_rgb(self) -> Rgb {
        let max = self.v;
        // At most `v`, so it fits in a byte.
        let min = div_round(u32::from(max) * (255 - u32::from(self.s)), 255) as u8;
        let h = u32::from(self.h % HUE_STEPS);
        let (sector, f) = (h / 256, h % 256);
        let rising = between(min, max, f);
        let falling = between(min, max, 256 - f);
        let (r, g, b) = match sector {
            0 => (max, rising, min),
            1 => (falling, max, min),
            2 => (min, max, rising),
            3 => (min, falling, max),
            4 => (rising, min, max),
            _ => (max, min, falling),
        };
        Rgb { r, g, b }
    }

    /// The hue, saturation and value of `rgb`. Where `rgb` leaves one
    /// undefined, `previous` keeps it: the hue of a grey, and also the
    /// saturation of black, so a picker cursor does not jump when the color
    /// passes through them.
    pub fn from_rgb(rgb: Rgb, previous: Self) -> Self {
        let (r, g, b) = (i32::from(rgb.r), i32::from(rgb.g), i32::from(rgb.b));
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let delta = max - min;
        // `max` is a channel, so it fits in a byte.
        let v = max as u8;
        if max == 0 {
            return Self { v, ..previous };
        }
        // `delta <= max`, so the saturation is at most 255.
        let s = div_round((delta * 255) as u32, max as u32) as u8;
        if delta == 0 {
            return Self {
                h: previous.h,
                s,
                v,
            };
        }
        let sixth = |base: i32, rising: i32| base * 256 + signed_div_round(rising * 256, delta);
        let h = if max == r {
            sixth(0, g - b)
        } else if max == g {
            sixth(2, b - r)
        } else {
            sixth(4, r - g)
        };
        Self {
            // Wrapped into 0..HUE_STEPS, so it fits in u16.
            h: h.rem_euclid(i32::from(HUE_STEPS)) as u16,
            s,
            v,
        }
    }
}

/// The channel `f` 256ths of the way from `min` to `max`.
fn between(min: u8, max: u8, f: u32) -> u8 {
    let (min, max) = (u32::from(min), u32::from(max));
    // Between min and max, so it fits in a byte.
    (min + div_round((max - min) * f, 256)) as u8
}

fn div_round(n: u32, d: u32) -> u32 {
    (n + d / 2) / d
}

fn signed_div_round(n: i32, d: i32) -> i32 {
    if n >= 0 {
        (n + d / 2) / d
    } else {
        -((-n + d / 2) / d)
    }
}

/// `color` as `#RRGGBB` in upper case.
pub fn format_hex(color: Rgb) -> String {
    let Rgb { r, g, b } = color;
    format!("#{r:02X}{g:02X}{b:02X}")
}

/// The color `text` writes as six hex digits, with or without a leading
/// `#`, in either case.
pub fn parse_hex(text: &str) -> Option<Rgb> {
    let digits = text.strip_prefix('#').unwrap_or(text);
    if digits.len() != 6 || !digits.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    u32::from_str_radix(digits, 16).ok().map(Rgb::from_hex)
}

#[cfg(test)]
mod tests;
