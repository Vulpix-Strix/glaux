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

-- Every `pset` is a separate script call, so the background keeps them under
-- 3000: the HUD sits on the brick wall instead of a filled band, and slab
-- joints are only marked where they cross.
function arena.draw()
    cls(FLOOR)

    -- Back wall: the cap row, then two rows of bricks behind the HUD.
    for x = 0, 304, 16 do
        spr(sheet.wall_top, x, -12, 2, 2)
        spr(sheet.wall_mid, x, 4, 2, 2)
        spr(sheet.wall_mid, x, 20, 2, 2)
    end

    for y = FLOOR_TOP + 16, 223, 16 do
        for x = 16, 304, 16 do
            pset(x, y, JOINT)
            pset(x - 1, y, JOINT)
            pset(x + 1, y, JOINT)
            pset(x, y - 1, JOINT)
            pset(x, y + 1, JOINT)
        end
    end

    for _, slab in decor do
        spr(slab.tile, slab.x, slab.y, 2, 2)
    end

    -- Side and bottom edges.
    for y = FLOOR_TOP, 239 do
        pset(0, y, EDGE_DARK)
        pset(1, y, EDGE_LIGHT)
        pset(318, y, EDGE_LIGHT)
        pset(319, y, EDGE_DARK)
    end
    for x = 2, 317 do
        pset(x, 238, EDGE_LIGHT)
        pset(x, 239, EDGE_DARK)
    end
end

return arena
