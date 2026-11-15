-- Thrown axes

local arena = require("arena")
local objects = require("objects")
local sheet = require("sheet")

local projectiles = {}

projectiles.MAX = 12
local SPEED = 4.2
-- Hitbox inside the axe head and haft.
projectiles.HIT_X, projectiles.HIT_Y, projectiles.HIT_W, projectiles.HIT_H = 4, 3, 9, 11

function projectiles.new()
    return {}
end

-- Throws an axe centered on `(cx, cy)` travelling along the unit vector
-- `(dx, dy)`. Returns false when too many axes are already in flight.
function projectiles.throw(list, cx, cy, dx, dy)
    if #list >= projectiles.MAX then
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

function projectiles.place(list, tick)
    -- Alternating between the axe and its mirror image reads as a spin.
    local flip = (tick // 4) % 2 == 1
    for i = #list, 1, -1 do
        local axe = list[i]
        objects.sprite(sheet.axe, axe.x, axe.y, 2, 2, flip)
    end
end

return projectiles
