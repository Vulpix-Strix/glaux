// Small helpers shared by every module

// xorshift32 over a plain integer state. Glaux has no random source, so
// every run with the same seed and the same input sequence plays out
// identically.
//
// Module functions cannot change their arguments, so the state is threaded
// by hand: `rng_next` returns the next state, which is also the next raw
// 32-bit value, and `rng_float`, `rng_int` and `rng_chance` read a value
// drawn that way:
//
//     rng = util::rng_next(rng);
//     let side = util::rng_int(rng, 1, 4);

fn rng_new(seed) {
    // A zero state would stay zero forever.
    let state = seed & 0xFFFFFFFF;
    if state == 0 {
        state = 0x9E3779B9;
    }
    state
}

fn rng_next(state) {
    let s = state;
    s = s ^ ((s << 13) & 0xFFFFFFFF);
    s = s ^ (s >> 17);
    s = s ^ ((s << 5) & 0xFFFFFFFF);
    s
}

// Uniform float in [0, 1).
fn rng_float(value) {
    value.to_float() / 4294967296.0
}

// Uniform integer in [lo, hi].
fn rng_int(value, lo, hi) {
    lo + value % (hi - lo + 1)
}

// True with probability `p`.
fn rng_chance(value, p) {
    rng_float(value) < p
}

// `value`, writable. Rhai marks a value read through `global::NAME` or
// `module::NAME` as constant, and a map field, array slot or literal entry
// given it directly stays constant: a later `field -= 1` fails with "Cannot
// modify constant". Passing the value through a parameter clears the mark,
// so constants that seed mutable state go through here.
fn writable(value) {
    value
}

fn clamp(v, lo, hi) {
    if v < lo {
        lo
    } else if v > hi {
        hi
    } else {
        v
    }
}

// Whether the boxes at `(ax, ay)` sized `aw`x`ah` and at `(bx, by)` sized
// `bw`x`bh` overlap.
fn overlap(ax, ay, aw, ah, bx, by, bw, bh) {
    ax < bx + bw && bx < ax + aw && ay < by + bh && by < ay + ah
}

// `[dx, dy]` scaled to length 1, or `[0, 0]` for a zero vector.
fn normalize(dx, dy) {
    let len = (dx * dx + dy * dy).to_float().sqrt();
    if len == 0.0 {
        return [0, 0];
    }
    [dx / len, dy / len]
}

// `n` in decimal, padded with leading zeros to `width` digits.
fn pad(n, width) {
    let digits = n.to_string();
    while digits.len() < width {
        digits = "0" + digits;
    }
    digits
}

// "MM:SS" for a duration in ticks.
fn clock(ticks) {
    let seconds = ticks / 60;
    pad(seconds / 60, 2) + ":" + pad(seconds % 60, 2)
}
