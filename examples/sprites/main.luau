function draw()
    cls(0)

    local start = 16
    local n = 0
    for row = 0, 1 do
        for col = 0, 7 do
            spr(start + n, 120 + col * 10, 110 + row * 10)
            n = n + 1
        end
    end
end