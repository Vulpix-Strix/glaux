-- The play field
--
-- A floor fully tiled with sprites would cost 300 16x16 sprites, more than
-- one frame's draw budget, so the floor is the cleared screen color with
-- marked slab joints and a few cracked slabs from the sheet.

local sheet = require("sheet")
local util = require("util")

local arena = {}

-- Area sprite boxes are kept inside, in screen pixels.
arena.LEFT = 4
arena.TOP = 30
arena.RIGHT = 316
arena.BOTTOM = 236

local FLOOR_TOP = 36
local FLOOR = 1 -- matches the slab fill of the sheet's floor tiles
local JOINT = 11
local EDGE_DARK = 11
local EDGE_LIGHT = 12

-- Cracked slabs on the 16-pixel floor grid, chosen once with their own seed
-- so the layout never depends on gameplay randomness.
local decor = {}
do
    local rng = util.rng(0x51A7)
    for _ = 1, 8 do
        table.insert(decor, {
            x = 16 * rng:int(0, 19),
            y = FLOOR_TOP + 16 * rng:int(0, 11),
            tile = sheet.floor[rng:int(2, #sheet.floor)],
        })
    end
end

function arena.draw()
    cls(FLOOR)

    -- Back wall: the cap row, then two rows of bricks behind the HUD.
    for x = 0, 304, 16 do
        spr(sheet.wall_top, x, -12, 2, 2)
        spr(sheet.wall_mid, x, 4, 2, 2)
        spr(sheet.wall_mid, x, 20, 2, 2)
    end

    -- A small cross where slab joints meet.
    for y = FLOOR_TOP + 16, 223, 16 do
        for x = 16, 304, 16 do
            rectfill(x - 1, y, 3, 1, JOINT)
            rectfill(x, y - 1, 1, 3, JOINT)
        end
    end

    for _, slab in decor do
        spr(slab.tile, slab.x, slab.y, 2, 2)
    end

    -- Side and bottom edges, two pixels thick.
    local side = 240 - FLOOR_TOP
    rectfill(0, FLOOR_TOP, 1, side, EDGE_DARK)
    rectfill(1, FLOOR_TOP, 1, side, EDGE_LIGHT)
    rectfill(318, FLOOR_TOP, 1, side, EDGE_LIGHT)
    rectfill(319, FLOOR_TOP, 1, side, EDGE_DARK)
    rectfill(2, 238, 316, 1, EDGE_LIGHT)
    rectfill(2, 239, 316, 1, EDGE_DARK)
end

return arena
