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

declare function btn(button: Button): boolean
declare function btnp(button: Button): boolean
declare function cls(color: number)
declare function frame(): number
declare function key(key: Key): boolean
declare function keyp(key: Key): boolean
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

declare log: {
	debug: (...any) -> (),
	info: (...any) -> (),
	trace: (...any) -> (),
	warn: (...any) -> (),
}
