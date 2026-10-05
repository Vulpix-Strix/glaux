local COLUMNS = 8
local ROWS = 5
local START_TILE = 0
local GRID_COLUMNS = 16

local CELL_WIDTH = 39
local CELL_HEIGHT = 39
local GRID_X = 4
local GRID_Y = 34

local selected_tile = START_TILE

local function clamp_tile(tile)
    return math.max(0, math.min(1023, tile))
end

local function draw_selection_border(x, y)
    for offset = -2, 9 do
        pset(x + offset, y - 2, 15)
        pset(x + offset, y + 9, 15)
    end
    for offset = -1, 8 do
        pset(x - 2, y + offset, 15)
        pset(x + 9, y + offset, 15)
    end
end

function tick()
    if btnp("left") then
        selected_tile = clamp_tile(selected_tile - 1)
    elseif btnp("right") then
        selected_tile = clamp_tile(selected_tile + 1)
    elseif btnp("up") then
        selected_tile = clamp_tile(selected_tile - GRID_COLUMNS)
    elseif btnp("down") then
        selected_tile = clamp_tile(selected_tile + GRID_COLUMNS)
    end
end

function draw()
    cls(0)
    print("SELECTED TILE " .. selected_tile, 6, 5, 7)
    print("LEFT/RIGHT: 1   UP/DOWN: " .. GRID_COLUMNS, 6, 17, 14)

    for offset = 0, COLUMNS * ROWS - 1 do
        local tile = selected_tile + offset
        if tile > 1023 then
            break
        end

        local column = offset % COLUMNS
        local row = math.floor(offset / COLUMNS)
        local cell_x = GRID_X + column * CELL_WIDTH
        local cell_y = GRID_Y + row * CELL_HEIGHT
        local sprite_x = cell_x + 14
        local sprite_y = cell_y + 2

        spr(tile, sprite_x, sprite_y)
        print(tile, cell_x + 5, cell_y + 14, 7)

        if offset == 0 then
            draw_selection_border(sprite_x, sprite_y)
        end
    end
end
