-- Welcome to your new cartridge!
-- A little owl waits under the night sky. Change anything you like,
-- then type 'run' in the shell to see what you made.

-- Colors are numbers from the 32-color system palette.
local SKY, STAR, MOON = 21, 15, 6
local HILLS, GRASS = 10, 9
local FEATHER, SHADE, BELLY = 4, 3, 6
local EYE, PUPIL, BEAK, FEET = 16, 11, 27, 26
local TITLE, TEXT, HINT = 26, 16, 14

local SCREEN_W, SCREEN_H = 320, 240
local GROUND_Y = 212
local CHAR_W = 6 -- width of one character of text

-- The owl is drawn on a 16x18 grid of big 2x2 pixels.
local SCALE = 2
local OWL_W, OWL_H = 16 * SCALE, 18 * SCALE
local SPEED = 2

local owl = { x = 144, y = GROUND_Y - OWL_H, facing = 1 }
local hoots = 0
local hop_timer = 0 -- ticks left in the current hop
local bubble_timer = 0 -- ticks left to show "Hoot!"

local messages = {
    "Press Z to say hello!",
    "Hoot! Nice to meet you.",
    "Hoot hoot! Let's make a game.",
    "Every pixel here is yours to change.",
}

local stars = {}

function boot()
    -- Spread the stars over the sky. The same formula gives the same sky
    -- every time the cartridge runs.
    for i = 1, 40 do
        stars[i] = { x = (i * 73) % SCREEN_W, y = 36 + (i * 37) % 120 }
    end
end

function tick()
    if btn(Button.Left) then
        owl.x -= SPEED
        owl.facing = -1
    end
    if btn(Button.Right) then
        owl.x += SPEED
        owl.facing = 1
    end
    if btn(Button.Up) then
        owl.y -= SPEED
    end
    if btn(Button.Down) then
        owl.y += SPEED
    end

    -- Keep the whole owl on screen, between the title and the grass.
    owl.x = math.clamp(owl.x, 0, SCREEN_W - OWL_W)
    owl.y = math.clamp(owl.y, 40, GROUND_Y - OWL_H)

    if btnp(Button.A) then
        hoots += 1
        hop_timer = 12
        bubble_timer = 60
    end
    hop_timer = math.max(hop_timer - 1, 0)
    bubble_timer = math.max(bubble_timer - 1, 0)
end

-- A filled circle, drawn one row at a time.
local function disc(cx, cy, r, color)
    for dy = -r, r do
        local half = math.floor(math.sqrt(r * r - dy * dy))
        rectfill(cx - half, cy + dy, half * 2 + 1, 1, color)
    end
end

local function print_centered(text, y, color)
    print(text, (SCREEN_W - #text * CHAR_W) // 2, y, color)
end

local function draw_sky()
    cls(SKY)
    for i, star in stars do
        -- Each star hides for a few ticks now and then, so the sky twinkles.
        if (frame() + i * 13) % 90 > 5 then
            pset(star.x, star.y, STAR)
        end
    end
    disc(266, 56, 14, MOON)
    disc(272, 52, 12, SKY) -- covers part of the moon to make a crescent
    disc(60, GROUND_Y + 70, 110, HILLS)
    disc(250, GROUND_Y + 90, 130, HILLS)
    rectfill(0, GROUND_Y, SCREEN_W, SCREEN_H - GROUND_Y, GRASS)
end

local function draw_owl(x, y)
    -- Draws one block of the owl's 16x18 grid.
    local function block(bx, by, w, h, color)
        rectfill(x + bx * SCALE, y + by * SCALE, w * SCALE, h * SCALE, color)
    end

    block(1, 0, 3, 3, SHADE) -- ear tufts
    block(12, 0, 3, 3, SHADE)
    block(0, 2, 16, 14, FEATHER) -- head and body
    block(0, 9, 2, 6, SHADE) -- wings
    block(14, 9, 2, 6, SHADE)
    block(4, 10, 8, 6, BELLY)
    block(7, 8, 2, 3, BEAK)
    block(4, 16, 3, 2, FEET)
    block(9, 16, 3, 2, FEET)

    -- Blink for a moment every few seconds.
    if frame() % 180 < 6 then
        block(2, 6, 5, 1, PUPIL)
        block(9, 6, 5, 1, PUPIL)
    else
        block(2, 4, 5, 5, EYE)
        block(9, 4, 5, 5, EYE)
        -- The pupils look the way the owl is facing.
        local look = owl.facing > 0 and 2 or 1
        block(2 + look, 5, 2, 2, PUPIL)
        block(9 + look, 5, 2, 2, PUPIL)
    end
end

local function draw_bubble(x, y)
    local w, h = 36, 13
    local bx = math.clamp(x + OWL_W // 2 - w // 2, 0, SCREEN_W - w)
    local by = y - h - 6
    rectfill(bx, by, w, h, EYE)
    rectfill(x + OWL_W // 2 - 1, by + h, 3, 3, EYE) -- the bubble's tail
    print("Hoot!", bx + 3, by + 2, PUPIL)
end

function draw()
    draw_sky()

    -- Hop in a smooth arc while the hop timer runs.
    local hop = math.floor(math.sin(hop_timer / 12 * math.pi) * 8)
    draw_owl(owl.x, owl.y - hop)
    if bubble_timer > 0 then
        draw_bubble(owl.x, owl.y - hop)
    end

    print_centered("Welcome to Glaux!", 8, TITLE)
    print_centered(messages[hoots % #messages + 1], 22, TEXT)
    print_centered("Arrows: move   Z: hoot", 224, HINT)
end
