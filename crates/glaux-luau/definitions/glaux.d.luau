-- Glaux API definitions for Luau.
-- Generated from docs/api/*.ron by `mise run luau-defs`. Do not edit.

type Waveform = "square" | "triangle" | "saw" | "noise"

declare extern type Button with
end

declare extern type Key with
end

declare Button: {
	Up: Button,
	Down: Button,
	Left: Button,
	Right: Button,
	A: Button,
	B: Button,
}

declare Key: {
	A: Key,
	B: Key,
	C: Key,
	D: Key,
	E: Key,
	F: Key,
	G: Key,
	H: Key,
	I: Key,
	J: Key,
	K: Key,
	L: Key,
	M: Key,
	N: Key,
	O: Key,
	P: Key,
	Q: Key,
	R: Key,
	S: Key,
	T: Key,
	U: Key,
	V: Key,
	W: Key,
	X: Key,
	Y: Key,
	Z: Key,
	Num0: Key,
	Num1: Key,
	Num2: Key,
	Num3: Key,
	Num4: Key,
	Num5: Key,
	Num6: Key,
	Num7: Key,
	Num8: Key,
	Num9: Key,
	F1: Key,
	F2: Key,
	F3: Key,
	F4: Key,
	F5: Key,
	F6: Key,
	F7: Key,
	F8: Key,
	F9: Key,
	F10: Key,
	F11: Key,
	F12: Key,
	Space: Key,
	Enter: Key,
	Tab: Key,
	Backspace: Key,
	Insert: Key,
	Delete: Key,
	Home: Key,
	End: Key,
	PageUp: Key,
	PageDown: Key,
	LeftShift: Key,
	RightShift: Key,
	LeftControl: Key,
	RightControl: Key,
	LeftAlt: Key,
	RightAlt: Key,
	Up: Key,
	Down: Key,
	Left: Key,
	Right: Key,
}

declare extern type HardwareAddr with
	WORK_RAM: number
	FRAMEBUFFER: number
	BG_TILES: number
	OBJ_TILES: number
	MASTER_PALETTE: number
	BG_PALETTES: number
	OBJ_PALETTES: number
	OBJ_ATTRIBUTES: number
	OAM: number
	DISPLAY_ENABLE: number
	BACKDROP_COLOR: number
end

declare extern type HardwareBgElement with
	TILEMAP: number
	SCROLL_X: number
	SCROLL_Y: number
	PRIORITY: number
end

declare extern type HardwareBg with
	LAYERS: number
	MAP_SIDE: number
	TILE_BYTES: number
	TILE_CAPACITY: number
	PALETTE_BANKS: number
	PRIORITY_MASK: number
	[number]: HardwareBgElement
end

declare extern type HardwareDisplay with
	WIDTH: number
	HEIGHT: number
	BG0: number
	BG1: number
	BG2: number
	OBJ: number
	MASK: number
end

declare extern type HardwareOam with
	ENTRIES: number
	ENTRY_BYTES: number
	FIELD_X: number
	FIELD_Y: number
	FIELD_TILE: number
	FIELD_ATTRIBUTES: number
	FIELD_SIZE: number
	TILE_MASK: number
	ATTR_BANK_MASK: number
	ATTR_FLIP_X: number
	ATTR_FLIP_Y: number
	ATTR_PRIORITY_SHIFT: number
	ATTR_PRIORITY_MASK: number
	SIZE_MASK: number
	ENABLED: number
	SIZE_8X8: number
	SIZE_16X16: number
	SIZE_32X32: number
	SIZE_8X16: number
	SIZE_16X8: number
	SIZE_16X32: number
	SIZE_32X16: number
	SIZE_RESERVED: number
end

declare extern type HardwareObj with
	TILE_WIDTH: number
	TILE_HEIGHT: number
	TILE_BYTES: number
	TILE_CAPACITY: number
	GRID_COLUMNS: number
	PALETTE_BANKS: number
end

declare extern type HardwarePalette with
	COLORS: number
	BANK_COLORS: number
end

declare extern type HardwareSize with
	ADDRESS_SPACE: number
	WORK_RAM: number
	FRAMEBUFFER: number
	BG_TILES: number
	OBJ_TILES: number
	TILEMAP: number
	MASTER_PALETTE: number
	BG_PALETTES: number
	OBJ_PALETTES: number
	OBJ_ATTRIBUTES: number
	OAM: number
end

declare extern type HardwareTile with
	ENTRY_BYTES: number
	INDEX_MASK: number
	FLIP_X: number
	FLIP_Y: number
	PRIORITY: number
	BANK_SHIFT: number
	BANK_MASK: number
end

declare function btn(button: Button): boolean
declare function btnp(button: Button): boolean
declare function cls(color: number)
declare function frame(): number
declare function key(key: Key): boolean
declare function keyp(key: Key): boolean
declare function memcpy(destination: number, source: number, length: number)
declare function memset(destination: number, value: number, length: number)
declare function peek(address: number): number
declare function peek16(address: number): number
declare function peek32(address: number): number
declare function poke(address: number, value: number)
declare function poke16(address: number, value: number)
declare function poke32(address: number, value: number)
declare function print(value: any, x: number, y: number, color: number)
declare function pset(x: number, y: number, color: number)
declare function rectfill(x: number, y: number, width: number, height: number, color: number)
declare function spr(tile: number, x: number, y: number, w: number?, h: number?, flip_x: boolean?, flip_y: boolean?, palette: number?)
declare function t(): number

declare audio: {
	env: (channel: number, attack_ms: number, decay_ms: number, sustain: number, release_ms: number) -> (),
	gate: (channel: number, enabled: boolean) -> (),
	tone: (channel: number, frequency: number, volume: number, waveform: Waveform) -> (),
}

declare hardware: {
	addr: HardwareAddr,
	bg: HardwareBg,
	display: HardwareDisplay,
	oam: HardwareOam,
	obj: HardwareObj,
	palette: HardwarePalette,
	size: HardwareSize,
	tile: HardwareTile,
}

declare log: {
	debug: (...any) -> (),
	info: (...any) -> (),
	trace: (...any) -> (),
	warn: (...any) -> (),
}

declare map: {
	copy: (id: string, window: number, x: number, y: number, width: number, height: number, source_layer: number?) -> (),
	get: (id: string, layer: number, x: number, y: number) -> (number, number),
	load: (id: string, window: number, x: number?, y: number?, source_layer: number?) -> (),
	size: (id: string) -> (number, number),
}
