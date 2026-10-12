// The knight

import "arena" as arena;
import "sheet" as sheet;
import "util" as util;

export const WIDTH = 16;
export const HEIGHT = 24;
// Body hitbox inside the 16x24 sprite, ignoring the plume and the empty
// margins of the source art.
export const HIT_X = 4;
export const HIT_Y = 10;
export const HIT_W = 8;
export const HIT_H = 13;

export const MAX_HP = 10; // half-hearts
const SPEED = 1.4;
const DASH_SPEED = 4.2;
const DASH_TICKS = 8;
export const DASH_COOLDOWN = 75;
const FIRE_COOLDOWN = 9;
const HURT_TICKS = 60;
const DIAGONAL = sqrt(0.5);

fn create() {
    #{
        x: 152,
        y: 130,
        // Last aim direction, a unit vector; the knight starts facing right.
        aim_x: 1,
        aim_y: 0,
        flip: false,
        moving: false,
        hp: util::writable(global::MAX_HP),
        dash: 0,
        dash_x: 0,
        dash_y: 0,
        dash_cooldown: 0,
        fire_cooldown: 0,
        hurt: 0,
    }
}

// Direction held on the d-pad, normalized, as `[dx, dy]`, or `[0, 0]`.
fn stick() {
    let dx = 0;
    let dy = 0;
    if btn(Button::Left) {
        dx -= 1;
    }
    if btn(Button::Right) {
        dx += 1;
    }
    if btn(Button::Up) {
        dy -= 1;
    }
    if btn(Button::Down) {
        dy += 1;
    }
    if dx != 0 && dy != 0 {
        return [dx * global::DIAGONAL, dy * global::DIAGONAL];
    }
    [dx, dy]
}

// Applies one tick of input. Returns `#{ player, dashed, fire }`: the
// updated knight, whether a dash started and whether an axe should be
// thrown this tick.
fn update(p) {
    let d = stick();
    let dx = d[0];
    let dy = d[1];
    let firing = btn(Button::A);
    p.moving = dx != 0 || dy != 0;

    // Holding A locks the aim so the knight can back away while throwing.
    if p.moving && !firing {
        p.aim_x = dx;
        p.aim_y = dy;
        if dx != 0 {
            p.flip = dx < 0;
        }
    }

    let dashed = false;
    if btnp(Button::B) && p.dash_cooldown == 0 {
        p.dash = util::writable(global::DASH_TICKS);
        p.dash_cooldown = util::writable(global::DASH_COOLDOWN);
        if p.moving {
            p.dash_x = dx;
            p.dash_y = dy;
        } else {
            p.dash_x = p.aim_x;
            p.dash_y = p.aim_y;
        }
        dashed = true;
    }

    if p.dash > 0 {
        p.dash -= 1;
        p.x += p.dash_x * global::DASH_SPEED;
        p.y += p.dash_y * global::DASH_SPEED;
    } else {
        p.x += dx * global::SPEED;
        p.y += dy * global::SPEED;
    }
    p.x = util::clamp(p.x, arena::LEFT, arena::RIGHT - global::WIDTH);
    p.y = util::clamp(p.y, arena::TOP, arena::BOTTOM - global::HEIGHT);

    if p.dash_cooldown > 0 {
        p.dash_cooldown -= 1;
    }
    if p.hurt > 0 {
        p.hurt -= 1;
    }

    let fire = false;
    if p.fire_cooldown > 0 {
        p.fire_cooldown -= 1;
    } else if firing {
        p.fire_cooldown = util::writable(global::FIRE_COOLDOWN);
        fire = true;
    }
    #{ player: p, dashed: dashed, fire: fire }
}

// Whether contact damage can land right now.
fn vulnerable(p) {
    p.dash == 0 && p.hurt == 0
}

// Applies `amount` half-hearts of damage and knocks the knight away from
// `(from_x, from_y)`. Returns the knight; it died if its `hp` is 0.
fn damage(p, amount, from_x, from_y) {
    p.hp = max(0, p.hp - amount);
    p.hurt = util::writable(global::HURT_TICKS);
    let k = util::normalize(p.x + 8 - from_x, p.y + 16 - from_y);
    p.x = util::clamp(p.x + k[0] * 10, arena::LEFT, arena::RIGHT - global::WIDTH);
    p.y = util::clamp(p.y + k[1] * 10, arena::TOP, arena::BOTTOM - global::HEIGHT);
    p
}

fn heal(p, amount) {
    p.hp = min(global::MAX_HP, p.hp + amount);
    p
}

// Center of the body hitbox, as `[x, y]`.
fn center(p) {
    [
        p.x + global::HIT_X + global::HIT_W.to_float() / 2.0,
        p.y + global::HIT_Y + global::HIT_H.to_float() / 2.0,
    ]
}

fn draw(p, tick) {
    // Blink while invulnerable after a hit.
    if p.hurt > 0 && (tick / 3) % 2 == 0 {
        return;
    }
    let frames = if p.moving || p.dash > 0 { sheet::KNIGHT_RUN } else { sheet::KNIGHT_IDLE };
    let speed = if p.dash > 0 { 3 } else { 6 };
    spr(frames[(tick / speed) % frames.len()], p.x, p.y, 2, 3, p.flip);
}
