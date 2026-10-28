--!strict
-- Every call here is valid in Glaux and must produce no type error.
local held: boolean = btn(Button.Left) or btnp(Button.A)
local pressed: boolean = key(Key.Space) or keyp(Key.PageUp)
local button: Button = Button.Up
local f: number = frame() + t()

cls(0)
pset(1.5, 2, 3)
rectfill(0, 0, 8, 8, 1)
print("score", 0, 0, 7)
print(42, 0, 0, 7)
spr(1, 0, 0)
spr(1, 0, 0, 2, 2, true, false, 3)

audio.tone(0, 440, 8, "square")
audio.env(0, 10, 10, 8, 100)
audio.gate(0, true)

log.trace()
log.info("one")
log.warn("a", 1, nil, {})
log.debug(held, pressed, button, f)

local util = require("util")

local doubled: number = util.double(2)
print(doubled, 0, 0, 0)
