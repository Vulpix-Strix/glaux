//! Validation of the values scripts pass to Glaux functions.
//!
//! Each function takes a number already converted from the script's own
//! representation and returns either the Glaux value or the detail of what
//! was expected, as in `palette index 0-255 expected, got 256`. Runtimes
//! wrap that detail as `bad argument #n to 'f' (detail)`, located at the
//! calling script line. Values are never wrapped or clamped into range.

use glaux_audio::{CHANNELS, Waveform};
use glaux_machine::obj_palette::{OBJ_PALETTE_BANKS, ObjBank};

/// Whether `n` is a whole number in `min..=max`.
fn whole_in(n: f64, min: f64, max: f64) -> bool {
    n.fract() == 0.0 && (min..=max).contains(&n)
}

/// A screen coordinate: floored, and saturated to the `i32` range so that
/// huge values land off-screen instead of failing.
pub fn coordinate(n: f64) -> Result<i32, String> {
    if n.is_nan() {
        return Err("number is NaN".to_owned());
    }
    // `as` saturates float-to-int conversions, including infinities.
    Ok(n.floor() as i32)
}

/// A size in pixels: a positive integer. `what` names the size in the
/// error. Values past `i32::MAX` saturate, as coordinates do; the result is
/// always at least 1.
pub fn size(n: f64, what: &str) -> Result<i32, String> {
    if n.fract() == 0.0 && n >= 1.0 {
        Ok(n as i32)
    } else {
        Err(format!("positive {what} expected, got {n}"))
    }
}

/// A palette index: an integer in `0..=255`.
pub fn color(n: f64) -> Result<u8, String> {
    if whole_in(n, 0.0, 255.0) {
        Ok(n as u8)
    } else {
        Err(format!("palette index 0-255 expected, got {n}"))
    }
}

/// An object-tile index: an integer in `0..=1023`.
pub fn tile(n: f64) -> Result<u16, String> {
    if whole_in(n, 0.0, 1023.0) {
        Ok(n as u16)
    } else {
        Err(format!("object tile index 0-1023 expected, got {n}"))
    }
}

/// A sprite block's `extent` (width or height) in tiles: a positive
/// integer that keeps the block inside the object-tile grid when it starts
/// at grid `axis` `start` of `limit`.
pub fn tile_span(
    n: f64,
    extent: &str,
    start: usize,
    limit: usize,
    axis: &str,
) -> Result<u8, String> {
    if !(n.fract() == 0.0 && n >= 1.0) {
        return Err(format!("positive tile count expected, got {n}"));
    }
    let available = limit - start;
    if n > available as f64 {
        return Err(format!(
            "{extent} {n} from tile {axis} {start} exceeds the {limit}-{axis} object-tile grid"
        ));
    }
    // `available` is at most the grid size, which fits in a byte.
    Ok(n as u8)
}

/// An integer in `0..=max`. `what` names the value in the error.
pub fn bounded(n: f64, what: &str, max: u32) -> Result<u32, String> {
    if whole_in(n, 0.0, f64::from(max)) {
        Ok(n as u32)
    } else {
        Err(format!("{what} 0-{max} expected, got {n}"))
    }
}

/// An object palette bank number.
pub fn obj_bank(n: f64) -> Result<ObjBank, String> {
    let max = OBJ_PALETTE_BANKS as u32 - 1;
    let bank = bounded(n, "object palette bank", max)?;
    // `bounded` keeps `bank` below OBJ_PALETTE_BANKS, so it is a valid bank.
    ObjBank::new(bank as u8).ok_or_else(|| format!("object palette bank 0-{max} expected"))
}

/// An audio channel number.
pub fn channel(n: f64) -> Result<usize, String> {
    let max = CHANNELS as u32 - 1;
    bounded(n, "channel", max).map(|channel| channel as usize)
}

/// A waveform, named as scripts name it.
pub fn waveform(name: &str) -> Result<Waveform, String> {
    Waveform::from_name(name).ok_or_else(|| {
        format!("unknown waveform '{name}'; expected square, triangle, saw or noise")
    })
}
