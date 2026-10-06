-- Enemy kinds, edge spawning with a short warning, chasing and the
-- difficulty curve that drives spawning.

local arena = require("arena")
local sheet = require("sheet")
local util = require("util")

local enemies = {}

-- `w`/`h` are the sprite size in tiles; `hit` is the body hitbox inside the
-- sprite as `{x, y, w, h}`. `knock` scales how far an axe pushes the enemy.
enemies.KINDS = {
    goblin = {
        w = 2, h = 2, right = sheet.goblin_run_r, left = sheet.goblin_run_l,
        hit = { 5, 6, 8, 9 }, speed = 0.62, hp = 1, damage = 1, score = 10, knock = 3,
    },
    orc = {
        w = 2, h = 3, right = sheet.orc_run_r, left = sheet.orc_run_l,
        hit = { 4, 9, 10, 13 }, speed = 0.5, hp = 3, damage = 2, score = 30, knock = 2,
    },
    ogre = {
        w = 4, h = 4, right = sheet.ogre_run_r, left = sheet.ogre_run_l,
        hit = { 8, 10, 16, 20 }, speed = 0.33, hp = 14, damage = 3, score = 150, knock = 0.4,
    },
}

-- Enemies plus pending spawns. With every slot an orc and two ogres, the
-- enemies cost about 17k of the 65k per-frame draw budget.
local MAX = 40
local MAX_OGRES = 2
local WARNING_TICKS = 36
local MIN_SPAWN_DISTANCE = 72

-- Spawning parameters `elapsed` ticks into a run.
function enemies.difficulty(elapsed)
    local s = elapsed / 60
    local ogre_every = nil
    if s >= 50 then
        ogre_every = 20 * 60
    end
    return {
        interval = math.max(16, math.floor(80 - s * 0.9)),
        batch = 1 + (s >= 45 and 1 or 0) + (s >= 90 and 1 or 0),
        orc_chance = s < 15 and 0 or math.min(0.4, (s - 15) / 90),
        ogre_every = ogre_every,
        speed = 1 + math.min(0.6, s / 150),
    }
end

function enemies.new()
    return {
        list = {},
        -- Spawns announced by a warning marker; each one becomes an enemy
        -- when its timer runs out.
        pending = {},
        spawn_timer = 60,
        ogre_timer = 0,
    }
end

local function count_ogres(state)
    local n = 0
    for _, e in state.list do
        if e.kind == "ogre" then
            n += 1
        end
    end
    for _, s in state.pending do
        if s.kind == "ogre" then
            n += 1
        end
    end
    return n
end

-- A point just inside a random arena edge, at least `MIN_SPAWN_DISTANCE`
-- from `(px, py)` when a few tries allow it.
local function edge_point(rng, px, py)
    local x, y
    for _ = 1, 6 do
        local side = rng:int(1, 4)
        if side == 1 then
            x, y = arena.LEFT + 4, rng:int(arena.TOP + 16, arena.BOTTOM - 8)
        elseif side == 2 then
            x, y = arena.RIGHT - 4, rng:int(arena.TOP + 16, arena.BOTTOM - 8)
        elseif side == 3 then
            x, y = rng:int(arena.LEFT + 8, arena.RIGHT - 8), arena.TOP + 14
        else
            x, y = rng:int(arena.LEFT + 8, arena.RIGHT - 8), arena.BOTTOM - 4
        end
        local dx, dy = x - px, y - py
        if dx * dx + dy * dy >= MIN_SPAWN_DISTANCE * MIN_SPAWN_DISTANCE then
            break
        end
    end
    return x, y
end

local function announce(state, rng, kind, px, py)
    if #state.list + #state.pending >= MAX then
        return false
    end
    local x, y = edge_point(rng, px, py)
    table.insert(state.pending, { kind = kind, x = x, y = y, timer = WARNING_TICKS })
    return true
end

-- Runs the spawn schedule for one tick. Returns the kinds of the enemies
-- that appeared this tick, and whether a spawn was skipped because the
-- enemy cap was reached.
function enemies.spawn(state, rng, elapsed, px, py)
    local d = enemies.difficulty(elapsed)

    local throttled = false
    state.spawn_timer -= 1
    if state.spawn_timer <= 0 then
        state.spawn_timer = d.interval
        for _ = 1, d.batch do
            if not announce(state, rng, rng:chance(d.orc_chance) and "orc" or "goblin", px, py) then
                throttled = true
            end
        end
    end

    if d.ogre_every then
        state.ogre_timer -= 1
        if state.ogre_timer <= 0 then
            state.ogre_timer = d.ogre_every
            if count_ogres(state) < MAX_OGRES then
                announce(state, rng, "ogre", px, py)
            end
        end
    end

    local appeared = {}
    for _, s in state.pending do
        s.timer -= 1
        if s.timer <= 0 then
            local kind = enemies.KINDS[s.kind]
            table.insert(state.list, {
                kind = s.kind,
                -- `(s.x, s.y)` is where the warning was, the sprite's center.
                x = s.x - kind.w * 4,
                y = s.y - kind.h * 4,
                hp = kind.hp,
                speed = kind.speed * d.speed * (0.9 + rng:float() * 0.2),
                phase = rng:int(0, 3),
                flip = false,
                kx = 0,
                ky = 0,
                flash = 0,
            })
            table.insert(appeared, s.kind)
        end
    end
    util.sweep(state.pending, function(s)
        return s.timer <= 0
    end)
    return appeared, throttled
end

-- The enemy's body hitbox in screen coordinates.
function enemies.box(e)
    local hit = enemies.KINDS[e.kind].hit
    return e.x + hit[1], e.y + hit[2], hit[3], hit[4]
end

function enemies.center(e)
    local x, y, w, h = enemies.box(e)
    return x + w / 2, y + h / 2
end

-- Moves every enemy toward `(px, py)`, keeping them from stacking up.
function enemies.update(state, px, py)
    local list = state.list
    for i, e in list do
        local cx, cy = enemies.center(e)
        local dx, dy = util.normalize(px - cx, py - cy)
        local mx, my = dx * e.speed, dy * e.speed

        -- Separation: a gentle push away from close neighbours.
        for j = i + 1, #list do
            local other = list[j]
            local ox, oy = enemies.center(other)
            local sx, sy = cx - ox, cy - oy
            local d2 = sx * sx + sy * sy
            if d2 < 144 and d2 > 0 then
                local nx, ny = util.normalize(sx, sy)
                mx += nx * 0.3
                my += ny * 0.3
                other.x -= nx * 0.3
                other.y -= ny * 0.3
            end
        end

        e.x += mx + e.kx
        e.y += my + e.ky
        e.kx *= 0.75
        e.ky *= 0.75
        if math.abs(dx) > 0.2 then
            e.flip = dx < 0
        end
        if e.flash > 0 then
            e.flash -= 1
        end
    end
end

-- Deals one point of damage, pushing the enemy along `(dx, dy)`. Returns
-- whether it died.
function enemies.hit(e, dx, dy)
    local kind = enemies.KINDS[e.kind]
    e.hp -= 1
    e.flash = 6
    e.kx += dx * kind.knock
    e.ky += dy * kind.knock
    return e.hp <= 0
end

function enemies.remove_dead(state)
    util.sweep(state.list, function(e)
        return e.hp <= 0
    end)
end

-- Draws pending-spawn warnings: a pulsing ring of pixels.
local function draw_warning(s, tick)
    local radius = 2 + (s.timer // 3) % 5
    local color = (tick // 4) % 2 == 0 and 6 or 5
    for k = 0, 7 do
        local angle = k * math.pi / 4 + tick * 0.08
        pset(s.x + math.cos(angle) * radius, s.y + math.sin(angle) * radius, color)
    end
end

function enemies.draw(state, tick)
    for _, s in state.pending do
        draw_warning(s, tick)
    end
    for _, e in state.list do
        -- Skipping frames is the only hit flash available: sprites cannot be
        -- tinted without OBJ palette banks.
        if e.flash == 0 or (tick // 2) % 2 == 0 then
            local kind = enemies.KINDS[e.kind]
            local frames = e.flip and kind.left or kind.right
            spr(frames[((tick // 7) + e.phase) % #frames + 1], e.x, e.y, kind.w, kind.h)
        end
    end
end

return enemies
