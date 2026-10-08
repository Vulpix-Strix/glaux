-- Draws a sprite sheet imported with `glaux import sprites`, first with the
-- colors it was drawn in, then recolored by passing spr() another palette.
--
-- The importer put the slime, flame and gem in OBP0 and the ghost in OBP1;
-- sprites.import.ron lists each sprite group's palette.

local SLIME, FLAME, GEM, GHOST = 0, 2, 4, 20
local GHOST_PALETTE = 1

local TEXT = 16

function draw()
    cls(0)

    print("THE WHOLE SHEET, AS DRAWN", 8, 8, TEXT)
    spr(0, 8, 24, 6, 2)

    print("ONE SPRITE AT A TIME", 8, 56, TEXT)
    spr(SLIME, 8, 72, 2, 2)
    spr(FLAME, 32, 72, 2, 2)
    spr(GEM, 56, 76)
    spr(GHOST, 72, 76, 2, 1)

    print("SLIME AND GEM IN THE GHOST'S PALETTE", 8, 104, TEXT)
    spr(SLIME, 8, 120, 2, 2, false, false, GHOST_PALETTE)
    spr(GEM, 32, 124, 1, 1, false, false, GHOST_PALETTE)
end
