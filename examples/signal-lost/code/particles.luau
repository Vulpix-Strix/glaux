-- Short-lived pixels for sparks, dust and bursts, drawn with `pset`, and the
-- skulls fallen enemies leave behind.

local sheet = require("sheet")
local util = require("util")

local particles = {}

-- Each particle costs one draw command; the cap keeps bursts cheap.
local MAX = 120

function particles.new()
    return {}
end

-- Adds one particle unless the pool is full.
function particles.add(list, x, y, vx, vy, life, color)
    if #list < MAX then
        table.insert(list, { x = x, y = y, vx = vx, vy = vy, life = life, color = color })
    end
end

-- `count` particles flying out of `(x, y)` in random directions.
function particles.burst(list, rng, x, y, count, speed, life, colors)
    for _ = 1, count do
        local angle = rng:float() * math.pi * 2
        local v = speed * (0.4 + rng:float() * 0.6)
        local color = colors[rng:int(1, #colors)]
        particles.add(list, x, y, math.cos(angle) * v, math.sin(angle) * v, life + rng:int(0, life // 2), color)
    end
end

function particles.update(list)
    for _, p in list do
        p.x += p.vx
        p.y += p.vy
        p.vx *= 0.9
        p.vy *= 0.9
        p.life -= 1
    end
    util.sweep(list, function(p)
        return p.life <= 0
    end)
end

function particles.draw(list)
    for _, p in list do
        pset(p.x, p.y, p.color)
    end
end

-- Skulls fade after a while; the cap bounds their draw cost.
local MAX_REMAINS = 24
local REMAINS_TICKS = 150

-- Leaves a skull centered on `(cx, cy)`, replacing the oldest when full.
function particles.remains(list, cx, cy)
    if #list >= MAX_REMAINS then
        table.remove(list, 1)
    end
    table.insert(list, { x = cx - 4, y = cy - 4, life = REMAINS_TICKS })
end

function particles.update_remains(list)
    for _, r in list do
        r.life -= 1
    end
    util.sweep(list, function(r)
        return r.life <= 0
    end)
end

function particles.draw_remains(list, tick)
    for _, r in list do
        if r.life > 30 or (tick // 3) % 2 == 0 then
            spr(sheet.skull, r.x, r.y)
        end
    end
end

return particles
