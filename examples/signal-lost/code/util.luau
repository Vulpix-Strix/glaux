-- Small helpers shared by every module

local util = {}

-- xorshift32. Glaux has no random source, so every run with the same seed
-- and the same input sequence plays out identically.
local Rng = {}
Rng.__index = Rng

function util.rng(seed)
    -- A zero state would stay zero forever.
    local state = bit32.band(seed, 0xFFFFFFFF)
    if state == 0 then
        state = 0x9E3779B9
    end
    return setmetatable({ state = state }, Rng)
end

-- Next raw 32-bit value.
function Rng:next()
    local s = self.state
    s = bit32.bxor(s, bit32.lshift(s, 13))
    s = bit32.bxor(s, bit32.rshift(s, 17))
    s = bit32.bxor(s, bit32.lshift(s, 5))
    self.state = s
    return s
end

-- Uniform float in [0, 1).
function Rng:float()
    return self:next() / 4294967296
end

-- Uniform integer in [lo, hi].
function Rng:int(lo, hi)
    return lo + self:next() % (hi - lo + 1)
end

-- True with probability `p`.
function Rng:chance(p)
    return self:float() < p
end

function util.clamp(v, lo, hi)
    if v < lo then
        return lo
    elseif v > hi then
        return hi
    end
    return v
end

-- Whether the boxes at `(ax, ay)` sized `aw`x`ah` and at `(bx, by)` sized
-- `bw`x`bh` overlap.
function util.overlap(ax, ay, aw, ah, bx, by, bw, bh)
    return ax < bx + bw and bx < ax + aw and ay < by + bh and by < ay + ah
end

-- `(dx, dy)` scaled to length 1, or `(0, 0)` for a zero vector.
function util.normalize(dx, dy)
    local len = math.sqrt(dx * dx + dy * dy)
    if len == 0 then
        return 0, 0
    end
    return dx / len, dy / len
end

-- Removes the entries of `list` for which `dead(entry)` is true, keeping
-- order, without allocating a new table.
function util.sweep(list, dead)
    local kept = 0
    for i = 1, #list do
        local entry = list[i]
        if not dead(entry) then
            kept += 1
            list[kept] = entry
        end
    end
    for i = #list, kept + 1, -1 do
        list[i] = nil
    end
end

-- "MM:SS" for a duration in ticks.
function util.clock(ticks)
    local seconds = ticks // 60
    return string.format("%02d:%02d", seconds // 60, seconds % 60)
end

return util
