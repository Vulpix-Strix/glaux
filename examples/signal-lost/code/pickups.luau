-- Dropped signal coins and healing flasks.

local sheet = require("sheet")
local util = require("util")

local pickups = {}

local MAX = 30
local MAX_FLASKS = 3
local LIFETIME = 600
local BLINK_FROM = 120
local MAGNET_RADIUS = 40
local MAGNET_SPEED = 2.2

pickups.COIN_SCORE = 5
pickups.FLASK_HEAL = 4

function pickups.new()
    return {}
end

-- Drops a pickup of `kind` ("coin" or "flask") centered on `(cx, cy)`.
function pickups.drop(list, kind, cx, cy)
    if #list >= MAX then
        return
    end
    if kind == "flask" then
        local flasks = 0
        for _, p in list do
            if p.kind == "flask" then
                flasks += 1
            end
        end
        if flasks >= MAX_FLASKS then
            return
        end
    end
    local size = kind == "coin" and 8 or 16
    table.insert(list, { kind = kind, x = cx - size / 2, y = cy - size / 2, size = size, life = LIFETIME })
end

-- Ages pickups and pulls nearby ones toward `(px, py)`. Returns the kinds
-- collected by the box `(bx, by, bw, bh)` this tick.
function pickups.update(list, px, py, bx, by, bw, bh)
    local collected = {}
    for _, p in list do
        p.life -= 1
        local half = p.size / 2
        local dx, dy = px - (p.x + half), py - (p.y + half)
        if dx * dx + dy * dy < MAGNET_RADIUS * MAGNET_RADIUS then
            local nx, ny = util.normalize(dx, dy)
            p.x += nx * MAGNET_SPEED
            p.y += ny * MAGNET_SPEED
        end
        -- The flask art leaves a margin; only its bottle counts.
        local inset = p.kind == "flask" and 4 or 1
        if util.overlap(p.x + inset, p.y + inset, p.size - inset * 2, p.size - inset * 2, bx, by, bw, bh) then
            table.insert(collected, p.kind)
            p.life = 0
        end
    end
    util.sweep(list, function(p)
        return p.life <= 0
    end)
    return collected
end

function pickups.draw(list, tick)
    local coin = sheet.coin[(tick // 6) % #sheet.coin + 1]
    for _, p in list do
        if p.life > BLINK_FROM or (tick // 4) % 2 == 0 then
            if p.kind == "coin" then
                spr(coin, p.x, p.y)
            else
                spr(sheet.flask, p.x, p.y, 2, 2)
            end
        end
    end
end

return pickups
