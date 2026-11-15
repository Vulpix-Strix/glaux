# Loot Rain

A small catch-the-loot game. Move the knight along the floor and collect what
falls from the sky.

| Item  | Effect                        |
|-------|-------------------------------|
| Coin  | Points, boosted by your combo |
| Skull | Costs one heart               |
| Flask | Restores health               |

Speed goes up as your score grows. The game ends when the hearts run out.

## Controls

| Virtual button | Desktop key | Action             |
|----------------|-------------|--------------------|
| D-pad          | Arrow keys  | Move left or right |
| A              | `Z`         | Start / retry      |

## Running it

```sh
mise run sync-examples -- loot-rain
mise run glaux
```

Then type `load loot-rain` and `run` in the Glaux shell.

## Structure

`code/main.luau` runs the screens and draws the game. `items.luau` spawns and
moves the falling loot, `score.luau` keeps the score, combo and speed, and
`sheet.luau` names the sprite tiles.

## What it shows

Sprites drawn with `spr()`, flipped to face either way, a small game split
into modules, and one-shot sound effects on the PSG.

## Credits

Sprites are from 0x72's DungeonTileset II v1.7
(<https://0x72.itch.io/dungeontileset-ii>, CC0), through the same sheet
Signal Lost uses.
