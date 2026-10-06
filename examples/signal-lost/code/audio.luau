-- Every sound effect in the game, played on the PSG through `audio.tone`,
-- `audio.env` and `audio.gate`.
--
-- Each effect owns one channel, retriggers it with a fresh envelope and
-- sweeps its pitch linearly, one step per tick, from `from` to `to` Hz. The
-- envelopes all have zero sustain, so a channel falls silent on its own even
-- if its gate stays open; `update` still closes the gate when the sweep ends
-- so no channel is left open between effects.

local sfx = {}

local FIRE, HIT, PICKUP, ALERT = 0, 1, 2, 3
local CHANNELS = 4

local EFFECTS = {
    fire = { channel = FIRE, wave = "square", from = 880, to = 420, volume = 7, ticks = 5, attack = 0, decay = 90, release = 20 },
    hit = { channel = HIT, wave = "noise", from = 3200, to = 1800, volume = 8, ticks = 4, attack = 0, decay = 70, release = 10 },
    kill = { channel = HIT, wave = "triangle", from = 320, to = 960, volume = 12, ticks = 7, attack = 0, decay = 140, release = 30 },
    coin = { channel = PICKUP, wave = "triangle", from = 1046, to = 1568, volume = 10, ticks = 6, attack = 0, decay = 120, release = 30 },
    heal = { channel = PICKUP, wave = "triangle", from = 392, to = 1175, volume = 11, ticks = 16, attack = 10, decay = 300, release = 40 },
    dash = { channel = PICKUP, wave = "noise", from = 7000, to = 1500, volume = 7, ticks = 9, attack = 0, decay = 160, release = 20 },
    hurt = { channel = ALERT, wave = "saw", from = 260, to = 70, volume = 13, ticks = 14, attack = 0, decay = 240, release = 40 },
    heavy = { channel = ALERT, wave = "square", from = 98, to = 82, volume = 9, ticks = 20, attack = 20, decay = 330, release = 60 },
    start = { channel = ALERT, wave = "triangle", from = 262, to = 1046, volume = 11, ticks = 20, attack = 0, decay = 350, release = 60 },
    gameover = { channel = ALERT, wave = "saw", from = 440, to = 55, volume = 13, ticks = 75, attack = 0, decay = 1300, release = 200 },
    crash = { channel = HIT, wave = "noise", from = 1200, to = 300, volume = 11, ticks = 30, attack = 0, decay = 520, release = 80 },
}

-- The sweep in progress on each channel, indexed by channel + 1.
local active = {}

-- Starts effect `name`, cutting off whatever its channel was playing.
function sfx.play(name)
    local effect = EFFECTS[name]
    local channel = effect.channel
    audio.env(channel, effect.attack, effect.decay, 0, effect.release)
    audio.tone(channel, effect.from, effect.volume, effect.wave)
    audio.gate(channel, true)
    active[channel + 1] = { effect = effect, step = 0 }
end

-- Advances every sweep by one tick and releases channels whose sweep ended.
function sfx.update()
    for slot = 1, CHANNELS do
        local sweep = active[slot]
        if sweep then
            local effect = sweep.effect
            sweep.step += 1
            if sweep.step >= effect.ticks then
                audio.gate(effect.channel, false)
                active[slot] = nil
            else
                local f = effect.from + (effect.to - effect.from) * sweep.step / effect.ticks
                audio.tone(effect.channel, math.floor(f), effect.volume, effect.wave)
            end
        end
    end
end

-- Closes every gate and mutes every oscillator, dropping all sweeps.
function sfx.silence()
    for channel = 0, CHANNELS - 1 do
        audio.gate(channel, false)
        audio.tone(channel, 0, 0, "square")
        active[channel + 1] = nil
    end
end

return sfx
