// Short-lived pixels for sparks, dust and bursts, drawn with `pset`, and the
// skulls fallen enemies leave behind.
//
// A pool is an array of particle maps. Functions that change a pool return
// it, and `burst` also returns the RNG state it advanced.

import "sheet" as sheet;
import "util" as util;

// Each particle costs one draw command; the cap keeps bursts cheap.
const MAX = 120;

// Skulls fade after a while; the cap bounds their draw cost.
const MAX_REMAINS = 24;
const REMAINS_TICKS = 150;

// Adds one particle unless the pool is full.
fn add(list, x, y, vx, vy, life, color) {
    if list.len() < global::MAX {
        list.push(#{ x: x, y: y, vx: vx, vy: vy, life: life, color: color });
    }
    list
}

// `count` particles flying out of `(x, y)` in random directions. Returns
// `#{ list, rng }`.
fn burst(list, rng, x, y, count, speed, life, colors) {
    for n in 0..count {
        rng = util::rng_next(rng);
        let angle = util::rng_float(rng) * PI() * 2.0;
        rng = util::rng_next(rng);
        let v = speed * (0.4 + util::rng_float(rng) * 0.6);
        rng = util::rng_next(rng);
        let color = colors[util::rng_int(rng, 0, colors.len() - 1)];
        rng = util::rng_next(rng);
        let extra = util::rng_int(rng, 0, life / 2);
        // Inlined `add`: passing the pool to it would copy it per particle.
        if list.len() < global::MAX {
            list.push(#{ x: x, y: y, vx: cos(angle) * v, vy: sin(angle) * v, life: life + extra, color: color });
        }
    }
    #{ list: list, rng: rng }
}

fn update(list) {
    for i in 0..list.len() {
        list[i].x += list[i].vx;
        list[i].y += list[i].vy;
        list[i].vx *= 0.9;
        list[i].vy *= 0.9;
        list[i].life -= 1;
    }
    list.retain(|p| p.life > 0);
    list
}

fn draw(list) {
    for p in list {
        pset(p.x, p.y, p.color);
    }
}

// Leaves a skull centered on `(cx, cy)`, replacing the oldest when full.
fn remains(list, cx, cy) {
    if list.len() >= global::MAX_REMAINS {
        list.remove(0);
    }
    list.push(#{ x: cx - 4, y: cy - 4, life: util::writable(global::REMAINS_TICKS) });
    list
}

fn update_remains(list) {
    for i in 0..list.len() {
        list[i].life -= 1;
    }
    list.retain(|r| r.life > 0);
    list
}

fn draw_remains(list, tick) {
    for r in list {
        if r.life > 30 || (tick / 3) % 2 == 0 {
            spr(sheet::SKULL, r.x, r.y);
        }
    }
}
