-- Sprites as OAM objects, rebuilt every tick from front to back.

local objects = {}

local OAM = hardware.addr.OAM
local ATTRIBUTES = hardware.addr.OBJ_ATTRIBUTES
local oam = hardware.oam
local ENTRIES = oam.ENTRIES

-- `{size, row offset}` per entry. 16x24 has no size code, so it takes two.
local SHAPES = {
    ["1x1"] = { { oam.SIZE_8X8, 0 } },
    ["2x2"] = { { oam.SIZE_16X16, 0 } },
    ["4x4"] = { { oam.SIZE_32X32, 0 } },
    ["2x3"] = { { oam.SIZE_16X16, 0 }, { oam.SIZE_16X8, 2 } },
}

local used = 0
local previous = 0
local offset_x, offset_y = 0, 0

-- Every object of the table is moved by `(dx, dy)`.
function objects.begin(dx, dy)
    used = 0
    offset_x, offset_y = dx, dy
end

function objects.reserve(n)
    if used + n > ENTRIES then
        return nil
    end
    local first = used
    used += n
    return first
end

local function word(v)
    return bit32.band(math.clamp(math.floor(v), -32768, 32767), 0xFFFF)
end

local function write(index, tile, x, y, size, flip)
    local at = OAM + index * oam.ENTRY_BYTES
    local attributes = bit32.band(peek(ATTRIBUTES + tile), oam.ATTR_BANK_MASK)
    if flip then
        attributes = bit32.bor(attributes, oam.ATTR_FLIP_X)
    end
    poke16(at + oam.FIELD_X, word(x + offset_x))
    poke16(at + oam.FIELD_Y, word(y + offset_y))
    poke16(at + oam.FIELD_TILE, tile)
    poke(at + oam.FIELD_ATTRIBUTES, attributes)
    poke(at + oam.FIELD_SIZE, bit32.bor(size, oam.ENABLED))
end

-- Draws as `spr` would, in the tile's own palette bank. A sprite that does
-- not fit whole is left out.
function objects.sprite(tile, x, y, w, h, flip)
    local parts = SHAPES[(w or 1) .. "x" .. (h or 1)]
    local first = objects.reserve(#parts)
    if first == nil then
        return false
    end
    for i, part in parts do
        local rows = part[2]
        write(first + i - 1, tile + rows * hardware.obj.GRID_COLUMNS, x, y + rows * 8, part[1], flip)
    end
    return true
end

function objects.finish()
    for index = used, previous - 1 do
        poke(OAM + index * oam.ENTRY_BYTES + oam.FIELD_SIZE, 0)
    end
    previous = used
end

return objects
