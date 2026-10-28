local score = {}

local MAX_MULTIPLIER = 5
local MAX_LEVEL = 8

score.points = 0
score.streak = 0
score.missed = 0
score.level = 1

function score.reset()
    score.points = 0
    score.streak = 0
    score.missed = 0
    score.level = 1
end

function score.multiplier()
    return math.min(1 + score.streak // 4, MAX_MULTIPLIER)
end

function score.catch()
    score.streak += 1
    score.points += 10 * score.multiplier()
    score.level = math.min(1 + score.points // 100, MAX_LEVEL)
end

function score.miss()
    local streak = score.streak
    streak = 0
    score.missed += 1
end

return score
