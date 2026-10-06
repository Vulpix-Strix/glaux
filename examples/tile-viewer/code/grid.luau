-- Layout of the tile grid and the helpers that draw it.

log.info("grid module loaded")

local grid = {
    COLUMNS = 8,
    ROWS = 5,
    START_TILE = 0,
    -- Width of G1's object-tile grid, in tiles.
    TILE_GRID_COLUMNS = 16,

    CELL_WIDTH = 39,
    CELL_HEIGHT = 39,
    X = 4,
    Y = 34,
}

function grid.clamp_tile(tile)
    return math.max(0, math.min(1023, tile))
end

function grid.draw_selection_border(x, y)
    for offset = -2, 9 do
        pset(x + offset, y - 2, 15)
        pset(x + offset, y + 9, 15)
    end
    for offset = -1, 8 do
        pset(x - 2, y + offset, 15)
        pset(x + 9, y + offset, 15)
    end
end

return grid
