-- Falling loot: spawning, movement, collisions and cleanup.

local score = require("score")
local sheet = require("sheet")

local items = {}

items.MAX_LIVES = 5

local SEED = 0x2F6E2B1
local GROUND = 224
local FIRST_SPAWN = 30

local KINDS = {
    coin = { size = 8 },
    skull = { size = 8 },
    flask = { size = 16 },
}

local state = SEED
local spawn_timer = FIRST_SPAWN

local function random()
    local s = state
    s = bit32.bxor(s, bit32.lshift(s, 13))
    s = bit32.bxor(s, bit32.rshift(s, 17))
    s = bit32.bxor(s, bit32.lshift(s, 5))
    state = s
    return s / 4294967296
end

local function pick_kind()
    local roll = random()
    if roll < 0.64 then
        return "coin"
    elseif roll < 0.88 then
        return "skull"
    end
    return "flask"
end

function items.reset()
    state = SEED
    spawn_timer = FIRST_SPAWN
end

local function spawn(list)
    local kind = pick_kind()
    local size = KINDS[kind].size
    list[#list + 1] = {
        kind = kind,
        x = math.floor(random() * (320 - size)),
        y = 16 - size,
        w = size,
        h = size,
        dead = false,
    }
end

local function touching(item, player)
    local left = player.x + player.w
    local right = left + player.w
    local top = player.y
    local bottom = player.y + player.h
    return item.x < right and item.x + item.w > left
        and item.y < bottom and item.y + item.h > top
end

local function collect(run, item)
    if item.kind == "coin" then
        score.catch()
        return "coin"
    elseif item.kind == "flask" then
        run.lives = math.max(run.lives + 1, items.MAX_LIVES)
        return "heal"
    end
    run.lives -= 1
    run.hurt = 40
    return "hurt"
end

-- Advances every item one tick and returns the events that happened, so the
-- caller can react with sound.
function items.update(run)
    local events = {}
    local list = run.items
    local fall = 1.2 + score.level * 0.35

    spawn_timer -= 1
    if spawn_timer <= 0 then
        spawn(list)
        spawn_timer = math.max(46 - score.level * 4, 16)
    end

    for _, item in ipairs(list) do
        item.y += fall
        if touching(item, run.player) then
            events[#events + 1] = collect(run, item)
            item.dead = true
        elseif item.y >= GROUND - item.h then
            if item.kind == "coin" then
                score.miss()
                events[#events + 1] = "miss"
            end
            item.dead = true
        end
    end

    local kept = 0
    for i = 1, #list do
        local item = list[i]
        if not item.dead then
            kept += 1
            list[kept] = item
        end
    end
    for i = #list, kept + 1, -1 do
        list[i] = nil
    end

    return events
end

function items.draw(list)
    local anim = (frame() // 6) % 4 + 1
    for _, item in ipairs(list) do
        local y = math.floor(item.y)
        if item.kind == "coin" then
            spr(sheet.coin[anim], item.x, y)
        elseif item.kind == "skull" then
            spr(sheet.skull, item.x, y)
        else
            spr(sheet.flask, item.x, y, 2, 2)
        end
    end
end

return items
