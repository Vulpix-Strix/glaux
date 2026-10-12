// The play field
//
// A floor fully tiled with sprites would cost 300 16x16 sprites, more than
// one frame's draw budget, so the floor is the cleared screen color with
// marked slab joints and a few cracked slabs from the sheet.

import "sheet" as sheet;
import "util" as util;

// Area sprite boxes are kept inside, in screen pixels.
export const LEFT = 4;
export const TOP = 30;
export const RIGHT = 316;
export const BOTTOM = 236;

const FLOOR_TOP = 36;
const FLOOR = 1; // matches the slab fill of the sheet's floor tiles
const JOINT = 11;
const EDGE_DARK = 11;
const EDGE_LIGHT = 12;

// Cracked slabs on the 16-pixel floor grid, chosen once with their own seed
// so the layout never depends on gameplay randomness.
let decor = [];
let rng = util::rng_new(0x51A7);
for n in 0..8 {
    rng = util::rng_next(rng);
    let x = 16 * util::rng_int(rng, 0, 19);
    rng = util::rng_next(rng);
    let y = FLOOR_TOP + 16 * util::rng_int(rng, 0, 11);
    rng = util::rng_next(rng);
    let tile = sheet::FLOOR[util::rng_int(rng, 1, sheet::FLOOR.len() - 1)];
    decor.push(#{ x: x, y: y, tile: tile });
}
const DECOR = decor;

fn draw() {
    cls(global::FLOOR);

    // Back wall: the cap row, then two rows of bricks behind the HUD.
    for x in range(0, 305, 16) {
        spr(sheet::WALL_TOP, x, -12, 2, 2);
        spr(sheet::WALL_MID, x, 4, 2, 2);
        spr(sheet::WALL_MID, x, 20, 2, 2);
    }

    // A small cross where slab joints meet.
    for y in range(global::FLOOR_TOP + 16, 224, 16) {
        for x in range(16, 305, 16) {
            rectfill(x - 1, y, 3, 1, global::JOINT);
            rectfill(x, y - 1, 1, 3, global::JOINT);
        }
    }

    for slab in global::DECOR {
        spr(slab.tile, slab.x, slab.y, 2, 2);
    }

    // Side and bottom edges, two pixels thick.
    let side = 240 - global::FLOOR_TOP;
    rectfill(0, global::FLOOR_TOP, 1, side, global::EDGE_DARK);
    rectfill(1, global::FLOOR_TOP, 1, side, global::EDGE_LIGHT);
    rectfill(318, global::FLOOR_TOP, 1, side, global::EDGE_LIGHT);
    rectfill(319, global::FLOOR_TOP, 1, side, global::EDGE_DARK);
    rectfill(2, 238, 316, 1, global::EDGE_LIGHT);
    rectfill(2, 239, 316, 1, global::EDGE_DARK);
}
