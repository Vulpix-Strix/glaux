-- Thrown axes

local arena = require("arena")
local sheet = require("sheet")

local projectiles = {}

local MAX = 12
local SPEED = 4.2
-- Hitbox inside the axe head and haft.
projectiles.HIT_X, projectiles.HIT_Y, projectiles.HIT_W, projectiles.HIT_H = 4, 3, 9, 11

function projectiles.new()
    return {}
end

-- Throws an axe centered on `(cx, cy)` travelling along the unit vector
-- `(dx, dy)`. Returns false when too many axes are already in flight.
function projectiles.throw(list, cx, cy, dx, dy)
    if #list >= MAX then
        return false
    end
    table.insert(list, { x = cx - 8, y = cy - 8, vx = dx * SPEED, vy = dy * SPEED, dead = false })
    return true
end

function projectiles.update(list)
    for _, axe in list do
        axe.x += axe.vx
        axe.y += axe.vy
        if axe.x < arena.LEFT - 16 or axe.x > arena.RIGHT or axe.y < arena.TOP - 16 or axe.y > arena.BOTTOM then
            axe.dead = true
        end
    end
end

function projectiles.draw(list, tick)
    -- Alternating the mirrored copy reads as a spin; G1 cannot flip sprites.
    local tile = sheet.axe[(tick // 4) % 2 + 1]
    for _, axe in list do
        spr(tile, axe.x, axe.y, 2, 2)
    end
end

return projectiles
