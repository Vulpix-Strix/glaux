local items = require("items")
local score = require("score")
local sheet = require("sheet")

local WHITE, GOLD, RED, DARK, PALE = 15, 26, 28, 11, 16
local SKY = { 21, 22, 23 }

local SPEED = 3
local KNIGHT_W, KNIGHT_H = 16, 24
local GROUND = 224

local STARS = {}
for i = 1, 40 do
    STARS[i] = { x = (i * 79) % 320, y = 18 + (i * 37) % 120 }
end

local game = { state = "title", best = 0 }

local SOUNDS = {
    coin = { channel = 2, wave = "triangle", pitch = 1318, volume = 10, decay = 110 },
    heal = { channel = 2, wave = "triangle", pitch = 880, volume = 11, decay = 300 },
    hurt = { channel = 3, wave = "saw", pitch = 180, volume = 13, decay = 240 },
    miss = { channel = 1, wave = "square", pitch = 110, volume = 6, decay = 120 },
    start = { channel = 3, wave = "triangle", pitch = 660, volume = 11, decay = 300 },
    over = { channel = 3, wave = "saw", pitch = 220, volume = 13, decay = 900 },
}

local function play(name)
    local s = SOUNDS[name]
    audio.env(s.channel, 0, s.decay, 0, 30)
    audio.tone(s.channel, s.pitch, s.volume, s.wave)
    audio.gate(s.channel, true)
end

local function new_run()
    score.reset()
    items.reset()
    return {
        player = { x = 152, y = GROUND - KNIGHT_H, w = KNIGHT_W, h = KNIGHT_H, flip = false, moving = false },
        items = {},
        lives = items.MAX_LIVES,
        hurt = 0,
    }
end

function boot()
    log.info("Loot Rain ready")
end

local function move(player)
    local dx = 0
    if btn(Button.Left) then
        dx -= 1
    end
    if btn(Button.Right) then
        dx += 1
    end
    player.moving = dx ~= 0
    if dx ~= 0 then
        player.flip = dx < 0
    end
    player.x = math.clamp(player.x + dx * SPEED, 0, 320 - player.w)
end

local function play_run()
    local run = game.run
    move(run.player)
    if run.hurt > 0 then
        run.hurt -= 1
    end

    for _, event in ipairs(items.update(run)) do
        play(event)
    end

    if run.lives <= 0 then
        game.best = math.max(game.best, score.points)
        game.state = "gameover"
        play("over")
        log.info("game over", score.points)
    end
end

function tick()
    if game.state == "playing" then
        play_run()
    elseif btnp(Button.A) then
        game.run = new_run()
        game.state = "playing"
        play("start")
    end
end

local function label(text, x, y, color)
    print(text, x + 1, y + 1, DARK)
    print(text, x, y, color)
end

local function centered(text, y, color)
    label(text, (320 - #text * 6) // 2, y, color)
end

local function draw_background()
    cls(SKY[1])
    rectfill(0, 112, 320, 56, SKY[2])
    rectfill(0, 168, 320, 56, SKY[3])
    for _, star in ipairs(STARS) do
        pset(star.x, star.y, PALE)
    end
    for column = 0, 19 do
        spr(sheet.floor[column % 8 + 1], column * 16, GROUND, 2, 2)
    end
end

local function draw_hud(run)
    rectfill(0, 0, 320, 16, DARK)
    for i = 1, items.MAX_LIVES do
        local tile = i <= run.lives and sheet.heart_full or sheet.heart_empty
        spr(tile, 2 + (i - 1) * 14, 0, 2, 2)
    end
    if run.lives > items.MAX_LIVES then
        spr(sheet.heart_full, 2 + items.MAX_LIVES * 14, 0, 2, 2)
    end
    label(string.format("SCORE %05d", score.points), 100, 3, WHITE)
    label("COMBO x" .. score.multiplier(), 192, 3, GOLD)
    label("LV " .. score.level, 282, 3, WHITE)
end

local function draw_knight(run)
    local p = run.player
    if run.hurt > 0 and (run.hurt // 4) % 2 == 0 then
        return
    end
    local frames = p.moving and sheet.knight_run or sheet.knight_idle
    spr(frames[(frame() // 6) % 4 + 1], p.x, p.y, 2, 3, p.flip)
end

function draw()
    draw_background()
    if game.state == "title" then
        spr(sheet.knight_idle[(frame() // 8) % 4 + 1], 152, GROUND - KNIGHT_H, 2, 3)
        centered("LOOT RAIN", 60, GOLD)
        centered("CATCH COINS  DODGE SKULLS", 84, WHITE)
        centered("GRAB FLASKS TO HEAL", 98, WHITE)
        if (frame() // 30) % 2 == 0 then
            centered("PRESS Z TO START", 140, PALE)
        end
        return
    end

    local run = game.run
    items.draw(run.items)
    draw_knight(run)
    draw_hud(run)

    if game.state == "gameover" then
        rectfill(60, 80, 200, 70, DARK)
        centered("GAME OVER", 90, RED)
        centered(string.format("SCORE %05d", score.points), 108, WHITE)
        centered(string.format("BEST  %05d", game.best), 122, GOLD)
        if (frame() // 30) % 2 == 0 then
            centered("PRESS Z TO RETRY", 138, PALE)
        end
    end
end
