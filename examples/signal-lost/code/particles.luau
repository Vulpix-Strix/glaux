-- Short-lived pixels for sparks, dust and bursts, drawn with `pset`, and the
-- skulls fallen enemies leave behind.

local arena = require("arena")
local objects = require("objects")
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

function particles.draw(list, dx, dy)
    for _, p in list do
        local y = p.y + dy
        if y >= arena.HUD_HEIGHT then
            pset(p.x + dx, y, p.color)
        end
    end
end

-- Skulls fade after a while.
particles.MAX_REMAINS = 24
local REMAINS_TICKS = 150

-- Leaves a skull centered on `(cx, cy)`, replacing the oldest when full.
function particles.remains(list, cx, cy)
    if #list >= particles.MAX_REMAINS then
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

-- Newest first, so a full OAM leaves out the oldest.
function particles.place_remains(list, tick)
    for i = #list, 1, -1 do
        local r = list[i]
        if r.life > 30 or (tick // 3) % 2 == 0 then
            objects.sprite(sheet.skull, r.x, r.y)
        end
    end
end

return particles
