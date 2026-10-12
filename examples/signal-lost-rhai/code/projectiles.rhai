// Thrown axes

import "arena" as arena;
import "sheet" as sheet;

const MAX = 12;
const SPEED = 4.2;
// Hitbox inside the axe head and haft.
export const HIT_X = 4;
export const HIT_Y = 3;
export const HIT_W = 9;
export const HIT_H = 11;

// Throws an axe centered on `(cx, cy)` travelling along the unit vector
// `(dx, dy)`. Returns `#{ list, thrown }`; nothing is thrown when too many
// axes are already in flight.
fn throw_axe(list, cx, cy, dx, dy) {
    if list.len() >= global::MAX {
        return #{ list: list, thrown: false };
    }
    list.push(#{ x: cx - 8, y: cy - 8, vx: dx * global::SPEED, vy: dy * global::SPEED, dead: false });
    #{ list: list, thrown: true }
}

fn update(list) {
    for i in 0..list.len() {
        list[i].x += list[i].vx;
        list[i].y += list[i].vy;
        let axe = list[i];
        if axe.x < arena::LEFT - 16 || axe.x > arena::RIGHT || axe.y < arena::TOP - 16 || axe.y > arena::BOTTOM {
            list[i].dead = true;
        }
    }
    list
}

fn draw(list, tick) {
    // Alternating between the axe and its mirror image reads as a spin.
    let flip = (tick / 4) % 2 == 1;
    for axe in list {
        spr(sheet::AXE, axe.x, axe.y, 2, 2, flip);
    }
}
