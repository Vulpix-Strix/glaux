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

local byte: number = peek(0)
poke(0, byte)
local word: number = peek16(0)
poke16(0, word)
local long: number = peek32(0)
poke32(0, long)
memcpy(0, 16, 4)
memset(0, 0, 4)

map.load("arena", 0)
map.load("arena", 1, 0, 0, 1)
map.copy("arena", 0, 10, 4, 2, 2)
map.copy("arena", 0, 10, 4, 2, 2, 1)
local width: number, height: number = map.size("arena")
local tile: number, entry: number = map.get("arena", 1, width - 1, height - 1)
poke16(hardware.bg[0].TILEMAP + tile * hardware.tile.ENTRY_BYTES, entry)
poke16(hardware.bg[2].SCROLL_X, hardware.bg.MAP_SIDE)
poke(hardware.addr.DISPLAY_ENABLE, bit32.bor(hardware.display.BG0, hardware.display.OBJ))
local oam = hardware.addr.OAM + hardware.oam.ENTRIES * hardware.oam.ENTRY_BYTES
local colors: number = hardware.palette.COLORS + hardware.obj.PALETTE_BANKS + hardware.size.OAM
log.info(oam, colors)
