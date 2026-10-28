--!strict
-- Each line marked `-- error` must produce a type error and no other line may.
local a = Button.LEFT -- error
local b = Key.NotAKey -- error
btn(1) -- error
btn("Left") -- error
btn({}) -- error
btn({ Up = 1 }) -- error
btn(Key.A) -- error
key(Button.A) -- error
local c: Button = 5 -- error
local d: Button = {} -- error
audio.tone(0, 440, 8, "sine") -- error
audio.tone(0, 440, 8, 1) -- error
audio.tone(0, 440, 8) -- error
audio.gate(0, 1) -- error
spr(1, 0) -- error
spr(1, 0, 0, "w") -- error
cls() -- error
text("rhai only", 0, 0, 1) -- error
local e: string = frame() -- error
tick() -- error
