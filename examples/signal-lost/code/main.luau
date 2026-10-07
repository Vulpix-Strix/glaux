-- Signal Lost. survive the dungeon arena for as long as you can.
--
-- A restart rebuilds the run's state inside the same VM; only the
-- PRNG stream, the best score and the run counter carry over.

local arena = require("arena")
local enemies = require("enemies")
local particles = require("particles")
local pickups = require("pickups")
local player = require("player")
local projectiles = require("projectiles")
local sfx = require("audio")
local sheet = require("sheet")
local util = require("util")

local SEED = 0x5167A1
local MILESTONE_TICKS = 30 * 60
local GAMEOVER_LOCK = 60
local FLASK_CHANCE = 0.04

local WHITE, LIME, GRAY, DARK, ORANGE, PALE = 15, 7, 13, 11, 5, 6

local game

-- State that lives for one run.
local function new_run()
    return {
        player = player.new(),
        enemies = enemies.new(),
        axes = projectiles.new(),
        pickups = pickups.new(),
        sparks = particles.new(),
        remains = particles.new(),
        score = 0,
        kills = 0,
        start_frame = frame(),
        start_time = t(),
        elapsed = 0,
        survived = 0,
        level = 0,
        warned_cap = false,
    }
end

local function start_run()
    sfx.silence()
    game.runs += 1
    game.run = new_run()
    game.state = "playing"
    sfx.play("start")
    if game.runs == 1 then
        log.info("run 1 started")
    else
        log.info("restart: run " .. game.runs)
    end
    log.trace("rng state", game.rng.state)
end

local function game_over()
    local run = game.run
    game.state = "gameover"
    game.lock = GAMEOVER_LOCK
    game.best = math.max(game.best, run.score)
    sfx.silence()
    sfx.play("gameover")
    sfx.play("crash")
    local px, py = player.center(run.player)
    particles.burst(run.sparks, game.rng, px, py, 40, 2.5, 30, { WHITE, PALE, ORANGE })
    log.info(string.format("signal lost: score %d, %d kills, survived %s", run.score, run.kills, util.clock(run.elapsed)))
end

function boot()
    game = {
        rng = util.rng(SEED),
        state = "title",
        run = nil,
        best = 0,
        runs = 0,
        lock = 0,
    }
    sfx.silence()
    log.info("Signal Lost ready")
end

local function kill(run, e)
    local kind = enemies.KINDS[e.kind]
    local cx, cy = enemies.center(e)
    run.score += kind.score
    run.kills += 1
    particles.burst(run.sparks, game.rng, cx, cy, 14, 2, 14, { WHITE, PALE, LIME, 8 })
    particles.remains(run.remains, cx, cy)
    if e.kind == "ogre" then
        for i = 1, 5 do
            pickups.drop(run.pickups, "coin", cx + (i - 3) * 6, cy + game.rng:int(-6, 6))
        end
        pickups.drop(run.pickups, "flask", cx, cy + 10)
    elseif game.rng:chance(FLASK_CHANCE) then
        pickups.drop(run.pickups, "flask", cx, cy)
    elseif e.kind == "orc" or game.rng:chance(0.5) then
        pickups.drop(run.pickups, "coin", cx, cy)
    end
    sfx.play("kill")
end

-- Axes against enemies: each axe stops at the first enemy it touches.
local function resolve_axes(run)
    for _, axe in run.axes do
        if not axe.dead then
            local ax = axe.x + projectiles.HIT_X
            local ay = axe.y + projectiles.HIT_Y
            for _, e in run.enemies.list do
                if e.hp > 0 then
                    local ex, ey, ew, eh = enemies.box(e)
                    if util.overlap(ax, ay, projectiles.HIT_W, projectiles.HIT_H, ex, ey, ew, eh) then
                        axe.dead = true
                        local dx, dy = util.normalize(axe.vx, axe.vy)
                        if enemies.hit(e, dx, dy) then
                            kill(run, e)
                        else
                            particles.burst(run.sparks, game.rng, ax + 4, ay + 5, 5, 1.6, 8, { WHITE, ORANGE })
                            sfx.play("hit")
                        end
                        break
                    end
                end
            end
        end
    end
    enemies.remove_dead(run.enemies)
    util.sweep(run.axes, function(axe)
        return axe.dead
    end)
end

-- Enemies against the knight. Returns whether the knight died.
local function resolve_contact(run)
    local p = run.player
    if not player.vulnerable(p) then
        return false
    end
    local bx, by = p.x + player.HIT_X, p.y + player.HIT_Y
    for _, e in run.enemies.list do
        local ex, ey, ew, eh = enemies.box(e)
        if util.overlap(bx, by, player.HIT_W, player.HIT_H, ex, ey, ew, eh) then
            local cx, cy = enemies.center(e)
            local died = player.damage(p, enemies.KINDS[e.kind].damage, cx, cy)
            local px, py = player.center(p)
            particles.burst(run.sparks, game.rng, px, py, 10, 1.8, 12, { ORANGE, 4, 3 })
            sfx.play("hurt")
            return died
        end
    end
    return false
end

local function tick_playing()
    local run = game.run
    local p = run.player
    run.elapsed = frame() - run.start_frame
    run.survived = t() - run.start_time

    local dashed, fire = player.update(p)
    local px, py = player.center(p)
    if dashed then
        sfx.play("dash")
        particles.burst(run.sparks, game.rng, px, py + 8, 8, 1.2, 10, { GRAY, 14 })
    end
    if p.dash > 0 then
        particles.add(run.sparks, px + game.rng:int(-3, 3), py + 10, 0, 0, 8, 14)
    end
    if fire and projectiles.throw(run.axes, px, py, p.aim_x, p.aim_y) then
        sfx.play("fire")
    end

    local appeared, throttled = enemies.spawn(run.enemies, game.rng, run.elapsed, px, py)
    for _, kind in appeared do
        if kind == "ogre" then
            sfx.play("heavy")
            log.debug("an ogre entered at " .. util.clock(run.elapsed))
        end
    end
    if throttled and not run.warned_cap then
        run.warned_cap = true
        log.warn("enemy cap reached at " .. util.clock(run.elapsed) .. "; spawns are being skipped")
    end

    enemies.update(run.enemies, px, py)
    projectiles.update(run.axes)
    resolve_axes(run)

    local collected = pickups.update(run.pickups, px, py, p.x + player.HIT_X, p.y + player.HIT_Y, player.HIT_W, player.HIT_H)
    for _, kind in collected do
        if kind == "coin" then
            run.score += pickups.COIN_SCORE
            sfx.play("coin")
        else
            player.heal(p, pickups.FLASK_HEAL)
            sfx.play("heal")
        end
        particles.burst(run.sparks, game.rng, px, py, 6, 1.4, 10, { PALE, LIME })
    end

    particles.update(run.sparks)
    particles.update_remains(run.remains)

    local level = run.elapsed // MILESTONE_TICKS
    if level > run.level then
        run.level = level
        local d = enemies.difficulty(run.elapsed)
        log.info(string.format("difficulty %d at %s: spawn every %d ticks, %d per wave", level, util.clock(run.elapsed), d.interval, d.batch))
    end

    if resolve_contact(run) then
        game_over()
    end
end

function tick()
    sfx.update()
    if game.state == "title" then
        if btnp("a") then
            start_run()
        end
    elseif game.state == "playing" then
        tick_playing()
    else
        particles.update(game.run.sparks)
        if game.lock > 0 then
            game.lock -= 1
        elseif btnp("a") then
            start_run()
        end
    end
end

-- Text with a one-pixel drop shadow, readable over sprites and bricks.
local function label(text, x, y, color)
    print(text, x + 1, y + 1, DARK)
    print(text, x, y, color)
end

local function centered(text, y, color)
    label(text, (320 - #text * 6) // 2, y, color)
end

local function draw_hud(run)
    local hp = run.player.hp
    for i = 1, player.MAX_HP // 2 do
        local tile = sheet.heart_empty
        if hp >= i * 2 then
            tile = sheet.heart_full
        elseif hp == i * 2 - 1 then
            tile = sheet.heart_half
        end
        spr(tile, 2 + (i - 1) * 14, 0, 2, 2)
    end
    label(string.format("SCORE %05d", run.score), 84, 3, WHITE)
    label("TIME " .. util.clock(math.floor(run.survived * 60)), 168, 3, WHITE)

    local p = run.player
    if p.dash_cooldown == 0 then
        label("DASH", 276, 3, LIME)
    else
        label("DASH", 276, 3, GRAY)
        -- Recharge bar under the label.
        local filled = math.floor(24 * (1 - p.dash_cooldown / player.DASH_COOLDOWN))
        for x = 0, filled - 1 do
            pset(276 + x, 13, LIME)
        end
    end
end

local function draw_world(run, tick)
    particles.draw_remains(run.remains, tick)
    pickups.draw(run.pickups, tick)
    enemies.draw(run.enemies, tick)
    if game.state == "playing" then
        player.draw(run.player, tick)
    end
    projectiles.draw(run.axes, tick)
    particles.draw(run.sparks)
    draw_hud(run)
end

local function draw_title(tick)
    -- A small cast lineup: the knight between the three enemy kinds.
    local step = (tick // 7) % 4 + 1
    spr(sheet.ogre_run_r[step], 64, 108, 4, 4)
    spr(sheet.orc_run_r[step], 108, 116, 2, 3)
    spr(sheet.goblin_run_r[step], 132, 124, 2, 2)
    spr(sheet.knight_idle_l[(tick // 8) % 4 + 1], 196, 116, 2, 3)
    spr(sheet.coin[(tick // 6) % 4 + 1], 176, 128)

    centered("SIGNAL LOST", 64, ORANGE)
    centered("SURVIVE THE ARENA", 80, PALE)
    -- Prompts name the default desktop keys (Z = A, X = B), not the
    -- virtual buttons the code reads.
    if (tick // 30) % 2 == 0 then
        centered("PRESS Z TO START", 164, WHITE)
    end
    centered("ARROWS MOVE   Z THROW   X DASH", 190, GRAY)
    centered("HOLD Z TO LOCK YOUR AIM", 202, GRAY)
    if game.best > 0 then
        centered("BEST " .. game.best, 220, LIME)
    end
end

local function draw_gameover(run, tick)
    centered("SIGNAL LOST", 92, ORANGE)
    centered(string.format("SCORE %d   KILLS %d   TIME %s", run.score, run.kills, util.clock(run.elapsed)), 110, WHITE)
    centered("BEST " .. game.best, 124, LIME)
    if game.lock == 0 and (tick // 30) % 2 == 0 then
        centered("PRESS Z TO RETRY", 146, WHITE)
    end
end

function draw()
    local tick = frame()
    arena.draw()
    if game.state == "title" then
        draw_title(tick)
    else
        draw_world(game.run, tick)
        if game.state == "gameover" then
            draw_gameover(game.run, tick)
        end
    end
end
