// Enemy kinds, edge spawning with a short warning, chasing and the
// difficulty curve that drives spawning.
//
// The spawner state is a map `#{ list, pending, spawn_timer, ogre_timer }`;
// functions that change it return it.

import "arena" as arena;
import "sheet" as sheet;
import "util" as util;

// `w`/`h` are the sprite size in tiles; `hit` is the body hitbox inside the
// sprite as `[x, y, w, h]`. `knock` scales how far an axe pushes the enemy.
export const KINDS = #{
    goblin: #{
        w: 2, h: 2, frames: sheet::GOBLIN_RUN,
        hit: [5, 6, 8, 9], speed: 0.62, hp: 1, damage: 1, score: 10, knock: 3,
    },
    orc: #{
        w: 2, h: 3, frames: sheet::ORC_RUN,
        hit: [4, 9, 10, 13], speed: 0.5, hp: 3, damage: 2, score: 30, knock: 2,
    },
    ogre: #{
        w: 4, h: 4, frames: sheet::OGRE_RUN,
        hit: [8, 10, 16, 20], speed: 0.33, hp: 14, damage: 3, score: 150, knock: 0.4,
    },
};

// Enemies plus pending spawns. With every slot an orc and two ogres, the
// enemies cost about 17k of the 65k per-frame draw budget.
const MAX = 40;
const MAX_OGRES = 2;
const WARNING_TICKS = 36;
const MIN_SPAWN_DISTANCE = 72;

// Spawning parameters `elapsed` ticks into a run. `ogre_every` is `()`
// before ogres start appearing.
fn difficulty(elapsed) {
    let s = elapsed.to_float() / 60.0;
    let ogre_every = ();
    if s >= 50.0 {
        ogre_every = 20 * 60;
    }
    #{
        interval: max(16, (80.0 - s * 0.9).floor().to_int()),
        batch: 1 + (if s >= 45.0 { 1 } else { 0 }) + (if s >= 90.0 { 1 } else { 0 }),
        orc_chance: if s < 15.0 { 0.0 } else { min(0.4, (s - 15.0) / 90.0) },
        ogre_every: ogre_every,
        speed: 1.0 + min(0.6, s / 150.0),
    }
}

fn create() {
    #{
        list: [],
        // Spawns announced by a warning marker; each one becomes an enemy
        // when its timer runs out.
        pending: [],
        spawn_timer: 60,
        ogre_timer: 0,
    }
}

fn count_ogres(state) {
    let n = 0;
    for e in state.list {
        if e.kind == "ogre" {
            n += 1;
        }
    }
    for s in state.pending {
        if s.kind == "ogre" {
            n += 1;
        }
    }
    n
}

// A point just inside a random arena edge, at least `MIN_SPAWN_DISTANCE`
// from `(px, py)` when a few tries allow it. Returns `#{ x, y, rng }`.
fn edge_point(rng, px, py) {
    let x = 0;
    let y = 0;
    for attempt in 0..6 {
        rng = util::rng_next(rng);
        let side = util::rng_int(rng, 1, 4);
        rng = util::rng_next(rng);
        if side == 1 {
            x = arena::LEFT + 4;
            y = util::rng_int(rng, arena::TOP + 16, arena::BOTTOM - 8);
        } else if side == 2 {
            x = arena::RIGHT - 4;
            y = util::rng_int(rng, arena::TOP + 16, arena::BOTTOM - 8);
        } else if side == 3 {
            x = util::rng_int(rng, arena::LEFT + 8, arena::RIGHT - 8);
            y = arena::TOP + 14;
        } else {
            x = util::rng_int(rng, arena::LEFT + 8, arena::RIGHT - 8);
            y = arena::BOTTOM - 4;
        }
        let dx = x - px;
        let dy = y - py;
        if dx * dx + dy * dy >= global::MIN_SPAWN_DISTANCE * global::MIN_SPAWN_DISTANCE {
            break;
        }
    }
    #{ x: x, y: y, rng: rng }
}

// Announces a spawn of `kind` unless the enemy cap is reached. Returns
// `#{ state, rng, announced }`.
fn announce(state, rng, kind, px, py) {
    if state.list.len() + state.pending.len() >= global::MAX {
        return #{ state: state, rng: rng, announced: false };
    }
    let at = edge_point(rng, px, py);
    state.pending.push(#{ kind: kind, x: at.x, y: at.y, timer: util::writable(global::WARNING_TICKS) });
    #{ state: state, rng: at.rng, announced: true }
}

// Runs the spawn schedule for one tick. Returns
// `#{ state, rng, appeared, throttled }`: the kinds of the enemies that
// appeared this tick, and whether a spawn was skipped because the enemy cap
// was reached.
fn advance_spawns(state, rng, elapsed, px, py) {
    let d = difficulty(elapsed);

    let throttled = false;
    state.spawn_timer -= 1;
    if state.spawn_timer <= 0 {
        state.spawn_timer = d.interval;
        for n in 0..d.batch {
            rng = util::rng_next(rng);
            let kind = if util::rng_chance(rng, d.orc_chance) { "orc" } else { "goblin" };
            let r = announce(state, rng, kind, px, py);
            state = r.state;
            rng = r.rng;
            if !r.announced {
                throttled = true;
            }
        }
    }

    if d.ogre_every != () {
        state.ogre_timer -= 1;
        if state.ogre_timer <= 0 {
            state.ogre_timer = d.ogre_every;
            if count_ogres(state) < global::MAX_OGRES {
                let r = announce(state, rng, "ogre", px, py);
                state = r.state;
                rng = r.rng;
            }
        }
    }

    let appeared = [];
    for i in 0..state.pending.len() {
        state.pending[i].timer -= 1;
        let s = state.pending[i];
        if s.timer <= 0 {
            let kind = global::KINDS[s.kind];
            rng = util::rng_next(rng);
            let speed = kind.speed * d.speed * (0.9 + util::rng_float(rng) * 0.2);
            rng = util::rng_next(rng);
            let phase = util::rng_int(rng, 0, 3);
            state.list.push(#{
                kind: s.kind,
                // `(s.x, s.y)` is where the warning was, the sprite's center.
                x: s.x - kind.w * 4,
                y: s.y - kind.h * 4,
                // The kind's hitbox, copied so the per-pair separation pass
                // does not look the kind up.
                hit_x: kind.hit[0],
                hit_y: kind.hit[1],
                hit_w: kind.hit[2],
                hit_h: kind.hit[3],
                hp: kind.hp,
                speed: speed,
                phase: phase,
                flip: false,
                kx: 0,
                ky: 0,
                flash: 0,
            });
            appeared.push(s.kind);
        }
    }
    state.pending.retain(|s| s.timer > 0);
    #{ state: state, rng: rng, appeared: appeared, throttled: throttled }
}

// The enemy's body hitbox in screen coordinates, as `[x, y, w, h]`.
fn box(e) {
    [e.x + e.hit_x, e.y + e.hit_y, e.hit_w, e.hit_h]
}

// Center of the body hitbox, as `[x, y]`.
fn center(e) {
    [
        e.x + e.hit_x + e.hit_w.to_float() / 2.0,
        e.y + e.hit_y + e.hit_h.to_float() / 2.0,
    ]
}

// Moves every enemy toward `(px, py)`, keeping them from stacking up.
fn update(state, px, py) {
    let list = state.list;
    let count = list.len();
    for i in 0..count {
        let e = list[i];
        let c = center(e);
        let d = util::normalize(px - c[0], py - c[1]);
        let mx = d[0] * e.speed;
        let my = d[1] * e.speed;

        // Separation: a gentle push away from close neighbours.
        for j in (i + 1)..count {
            let o = center(list[j]);
            let sx = c[0] - o[0];
            let sy = c[1] - o[1];
            let d2 = sx * sx + sy * sy;
            if d2 < 144.0 && d2 > 0.0 {
                let n = util::normalize(sx, sy);
                mx += n[0] * 0.3;
                my += n[1] * 0.3;
                list[j].x -= n[0] * 0.3;
                list[j].y -= n[1] * 0.3;
            }
        }

        e.x += mx + e.kx;
        e.y += my + e.ky;
        e.kx *= 0.75;
        e.ky *= 0.75;
        if d[0].abs() > 0.2 {
            e.flip = d[0] < 0.0;
        }
        if e.flash > 0 {
            e.flash -= 1;
        }
        list[i] = e;
    }
    state.list = list;
    state
}

// Deals one point of damage, pushing the enemy along `(dx, dy)`. Returns
// the enemy; it died if its `hp` is 0 or less.
fn hit(e, dx, dy) {
    let kind = global::KINDS[e.kind];
    e.hp -= 1;
    e.flash = 6;
    e.kx += dx * kind.knock;
    e.ky += dy * kind.knock;
    e
}

fn remove_dead(state) {
    state.list.retain(|e| e.hp > 0);
    state
}

// Draws pending-spawn warnings: a pulsing ring of pixels.
fn draw_warning(s, tick) {
    let radius = 2 + (s.timer / 3) % 5;
    let color = if (tick / 4) % 2 == 0 { 6 } else { 5 };
    for k in 0..8 {
        let angle = k * PI() / 4.0 + tick * 0.08;
        pset(s.x + cos(angle) * radius, s.y + sin(angle) * radius, color);
    }
}

fn draw(state, tick) {
    for s in state.pending {
        draw_warning(s, tick);
    }
    for e in state.list {
        // Skipping frames is the only hit flash available: sprites cannot be
        // tinted without OBJ palette banks.
        if e.flash == 0 || (tick / 2) % 2 == 0 {
            let kind = global::KINDS[e.kind];
            let frames = kind.frames;
            spr(frames[((tick / 7) + e.phase) % frames.len()], e.x, e.y, kind.w, kind.h, e.flip);
        }
    }
}
