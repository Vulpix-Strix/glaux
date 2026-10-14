//! Pointer drags as unbroken lines of tile pixels.
//!
//! The host reports at most one pointer move per frame, so a fast drag
//! jumps several pixels between moves. Joining consecutive positions with
//! a line leaves no gaps.

/// The pixels of the line from `from` to `to`, both included, in order.
/// Each pixel is a step of at most one on each axis from the one before.
pub fn line(from: (i32, i32), to: (i32, i32)) -> impl Iterator<Item = (i32, i32)> {
    let (dx, dy) = ((to.0 - from.0).abs(), -(to.1 - from.1).abs());
    let (sx, sy) = ((to.0 - from.0).signum(), (to.1 - from.1).signum());
    let mut at = from;
    let mut error = dx + dy;
    let mut done = false;
    std::iter::from_fn(move || {
        if done {
            return None;
        }
        let current = at;
        if at == to {
            done = true;
            return Some(current);
        }
        let twice = 2 * error;
        if twice >= dy {
            error += dy;
            at.0 += sx;
        }
        if twice <= dx {
            error += dx;
            at.1 += sy;
        }
        Some(current)
    })
}

#[cfg(test)]
mod tests;
