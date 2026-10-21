-- Logs a fixed number of messages on every tick, to measure what logging
-- costs a running cartridge with the log panel closed and open.
--
-- Up and Down change the rate between the steps below. Hold A to log a
-- warning on every tick as well.

local RATES = { 0, 10, 100, 1000, 10000 }
local step = 3
local sent = 0

function tick()
    if btnp(Button.Up) and step < #RATES then
        step += 1
    end
    if btnp(Button.Down) and step > 1 then
        step -= 1
    end
    local count = RATES[step]
    for i = 1, count do
        log.info("message", sent + i, "of the tick", frame())
    end
    sent += count
    if btn(Button.A) then
        log.warn("warning at frame", frame())
    end
end

function draw()
    cls(1)
    print("LOG STRESS", 8, 8, 7)
    print(RATES[step] .. " MESSAGES PER TICK", 8, 24, 7)
    print(sent .. " SENT", 8, 40, 7)
    print("UP/DOWN: RATE   A: WARN", 8, 56, 6)
end
