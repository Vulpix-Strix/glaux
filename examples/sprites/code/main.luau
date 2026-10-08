function draw()
    cls(0)

    -- Single 8x8 tiles: sheet rows 2 and 3 sit at grid rows 2 and 3.
    for row = 0, 1 do
        for col = 0, 7 do
            spr(32 + row * 16 + col, 120 + col * 10, 90 + row * 10)
        end
    end

    -- A 16x16 sprite composed from tiles 36, 37 / 52, 53.
    spr(36, 120, 130, 2, 2)

    -- The same sprite mirrored horizontally, vertically and on both axes.
    -- The whole 2x2 block flips, so tile order reverses along with pixels.
    spr(36, 120, 158, 2, 2, true)
    spr(36, 140, 158, 2, 2, false, true)
    spr(36, 160, 158, 2, 2, true, true)

    -- A 32x32 sprite composed from the sheet's top-left 4x4 block.
    spr(0, 156, 122, 4, 4)
end
