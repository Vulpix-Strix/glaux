-- The knight

local arena = require("arena")
local sheet = require("sheet")
local util = require("util")

local player = {}

player.WIDTH = 16
player.HEIGHT = 24
-- Body hitbox inside the 16x24 sprite, ignoring the plume and the empty
-- margins of the source art.
player.HIT_X, player.HIT_Y, player.HIT_W, player.HIT_H = 4, 10, 8, 13

player.MAX_HP = 10 -- half-hearts
local SPEED = 1.4
local DASH_SPEED = 4.2
local DASH_TICKS = 8
player.DASH_COOLDOWN = 75
local FIRE_COOLDOWN = 9
local HURT_TICKS = 60
local DIAGONAL = math.sqrt(0.5)

function player.new()
    return {
        x = 152,
        y = 130,
        -- Last aim direction, a unit vector; the knight starts facing right.
        aim_x = 1,
        aim_y = 0,
        flip = false,
        moving = false,
        hp = player.MAX_HP,
        dash = 0,
        dash_x = 0,
        dash_y = 0,
        dash_cooldown = 0,
        fire_cooldown = 0,
        hurt = 0,
    }
end

-- Direction held on the d-pad, normalized, or `(0, 0)`.
local function stick()
    local dx, dy = 0, 0
    if btn(Button.Left) then
        dx -= 1
    end
    if btn(Button.Right) then
        dx += 1
    end
    if btn(Button.Up) then
        dy -= 1
    end
    if btn(Button.Down) then
        dy += 1
    end
    if dx ~= 0 and dy ~= 0 then
        return dx * DIAGONAL, dy * DIAGONAL
    end
    return dx, dy
end

-- Applies one tick of input. Returns two flags: whether a dash started and
-- whether an axe should be thrown this tick.
function player.update(p)
    local dx, dy = stick()
    local firing = btn(Button.A)
    p.moving = dx ~= 0 or dy ~= 0

    -- Holding A locks the aim so the knight can back away while throwing.
    if p.moving and not firing then
        p.aim_x, p.aim_y = dx, dy
        if dx ~= 0 then
            p.flip = dx < 0
        end
    end

    local dashed = false
    if btnp(Button.B) and p.dash_cooldown == 0 then
        p.dash = DASH_TICKS
        p.dash_cooldown = player.DASH_COOLDOWN
        if p.moving then
            p.dash_x, p.dash_y = dx, dy
        else
            p.dash_x, p.dash_y = p.aim_x, p.aim_y
        end
        dashed = true
    end

    if p.dash > 0 then
        p.dash -= 1
        p.x += p.dash_x * DASH_SPEED
        p.y += p.dash_y * DASH_SPEED
    else
        p.x += dx * SPEED
        p.y += dy * SPEED
    end
    p.x = util.clamp(p.x, arena.LEFT, arena.RIGHT - player.WIDTH)
    p.y = util.clamp(p.y, arena.TOP, arena.BOTTOM - player.HEIGHT)

    if p.dash_cooldown > 0 then
        p.dash_cooldown -= 1
    end
    if p.hurt > 0 then
        p.hurt -= 1
    end

    local fire = false
    if p.fire_cooldown > 0 then
        p.fire_cooldown -= 1
    elseif firing then
        p.fire_cooldown = FIRE_COOLDOWN
        fire = true
    end
    return dashed, fire
end

-- Whether contact damage can land right now.
function player.vulnerable(p)
    return p.dash == 0 and p.hurt == 0
end

-- Applies `amount` half-hearts of damage and knocks the knight away from
-- `(from_x, from_y)`. Returns whether the knight died.
function player.damage(p, amount, from_x, from_y)
    p.hp = math.max(0, p.hp - amount)
    p.hurt = HURT_TICKS
    local kx, ky = util.normalize(p.x + 8 - from_x, p.y + 16 - from_y)
    p.x = util.clamp(p.x + kx * 10, arena.LEFT, arena.RIGHT - player.WIDTH)
    p.y = util.clamp(p.y + ky * 10, arena.TOP, arena.BOTTOM - player.HEIGHT)
    return p.hp == 0
end

function player.heal(p, amount)
    p.hp = math.min(player.MAX_HP, p.hp + amount)
end

-- Center of the body hitbox.
function player.center(p)
    return p.x + player.HIT_X + player.HIT_W / 2, p.y + player.HIT_Y + player.HIT_H / 2
end

function player.draw(p, tick)
    -- Blink while invulnerable after a hit.
    if p.hurt > 0 and (tick // 3) % 2 == 0 then
        return
    end
    local frames = (p.moving or p.dash > 0) and sheet.knight_run or sheet.knight_idle
    local speed = p.dash > 0 and 3 or 6
    spr(frames[(tick // speed) % #frames + 1], p.x, p.y, 2, 3, p.flip)
end

return player
