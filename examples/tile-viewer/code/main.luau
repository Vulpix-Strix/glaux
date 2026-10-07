local grid = require("grid")

-- A module runs once; requiring it again returns the same table.
assert(rawequal(require("grid"), grid))

local selected_tile = grid.START_TILE

function tick()
    if btnp(Button.Left) then
        selected_tile = grid.clamp_tile(selected_tile - 1)
    elseif btnp(Button.Right) then
        selected_tile = grid.clamp_tile(selected_tile + 1)
    elseif btnp(Button.Up) then
        selected_tile = grid.clamp_tile(selected_tile - grid.TILE_GRID_COLUMNS)
    elseif btnp(Button.Down) then
        selected_tile = grid.clamp_tile(selected_tile + grid.TILE_GRID_COLUMNS)
    end
end

function draw()
    cls(0)
    print("SELECTED TILE " .. selected_tile, 6, 5, 7)
    print("LEFT/RIGHT: 1   UP/DOWN: " .. grid.TILE_GRID_COLUMNS, 6, 17, 14)

    for offset = 0, grid.COLUMNS * grid.ROWS - 1 do
        local tile = selected_tile + offset
        if tile > 1023 then
            break
        end

        local column = offset % grid.COLUMNS
        local row = math.floor(offset / grid.COLUMNS)
        local cell_x = grid.X + column * grid.CELL_WIDTH
        local cell_y = grid.Y + row * grid.CELL_HEIGHT
        local sprite_x = cell_x + 14
        local sprite_y = cell_y + 2

        spr(tile, sprite_x, sprite_y)
        print(tile, cell_x + 5, cell_y + 14, 7)

        if offset == 0 then
            grid.draw_selection_border(sprite_x, sprite_y)
        end
    end
end
