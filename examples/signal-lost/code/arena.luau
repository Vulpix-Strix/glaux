-- The play field, from the `arena` map.

local util = require("util")

local arena = {}

-- Area sprite boxes are kept inside, in screen pixels.
arena.LEFT = 4
arena.TOP = 30
arena.RIGHT = 316
arena.BOTTOM = 236

arena.HUD_HEIGHT = 16

arena.dx, arena.dy = 0, 0

local ARENA = "arena"
local WORN = "arena-worn"
local HEARTS = "hud-hearts"

local FLOOR, WALLS, HUD = 0, 1, 2
-- Keeps the floor at y = 36, between tile rows.
local SCROLL_Y = 12
local BACKDROP = 1

local SLAB_TOP = 6
local SLABS_X, SLABS_Y = 20, 13
local WEAR_PER_MILESTONE = 8
local WEAR_SEED = 0xC4A7

local HEART_COLUMNS, HEART_ROWS = 10, 2

local SHAKE = {
    { 2, 1 }, { -2, -1 }, { 1, -2 }, { -1, 2 }, { 2, 0 },
    { -2, 1 }, { 1, -1 }, { -1, 0 }, { 1, 1 }, { 0, -1 },
}

local bg = hardware.bg
local display = hardware.display

local shake = 0
local hearts_shown = nil
local wear = nil

local function scroll(layer, x, y)
    poke16(bg[layer].SCROLL_X, bit32.band(x, 0xFFFF))
    poke16(bg[layer].SCROLL_Y, bit32.band(y, 0xFFFF))
end

function arena.boot()
    for layer = FLOOR, HUD do
        map.load(ARENA, layer)
    end
    poke(bg[FLOOR].PRIORITY, 3)
    poke(bg[WALLS].PRIORITY, 3)
    poke(bg[HUD].PRIORITY, 0)
    poke(hardware.addr.BACKDROP_COLOR, BACKDROP)
    arena.show_hud(false)
    arena.update()
end

function arena.show_hud(on)
    local enable = bit32.bor(display.BG0, display.BG1, display.OBJ)
    if on then
        enable = bit32.bor(enable, display.BG2)
    end
    poke(hardware.addr.DISPLAY_ENABLE, enable)
end

function arena.reset()
    map.load(ARENA, FLOOR)
    wear = util.rng(WEAR_SEED)
    shake = 0
end

function arena.wear()
    for _ = 1, WEAR_PER_MILESTONE do
        local x = 2 * wear:int(0, SLABS_X - 1)
        local y = SLAB_TOP + 2 * wear:int(0, SLABS_Y - 1)
        map.copy(WORN, FLOOR, x, y, 2, 2)
    end
end

function arena.hearts(hp)
    if hp == hearts_shown then
        return
    end
    hearts_shown = hp
    local window = bg[HUD].TILEMAP
    for row = 0, HEART_ROWS - 1 do
        for col = 0, HEART_COLUMNS - 1 do
            local _, entry = map.get(HEARTS, HUD, col, hp * HEART_ROWS + row)
            poke16(window + (row * bg.MAP_SIDE + col) * hardware.tile.ENTRY_BYTES, entry)
        end
    end
end

function arena.shake()
    shake = #SHAKE
end

function arena.update()
    local dx, dy = 0, 0
    if shake > 0 then
        local step = SHAKE[#SHAKE - shake + 1]
        dx, dy = step[1], step[2]
        shake -= 1
    end
    scroll(FLOOR, -dx, SCROLL_Y - dy)
    scroll(WALLS, -dx, SCROLL_Y - dy)
    arena.dx, arena.dy = dx, dy
    return dx, dy
end

return arena
