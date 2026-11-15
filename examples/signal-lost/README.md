# Signal Lost

A single-screen arena survival shooter. A knight is trapped in a dungeon
arena while goblins, masked orcs and ogres pour in from the edges. Throw
axes, dash through gaps, grab the coins they drop, and see how long you last.

Goblins are worth 10 points, orcs 30 and ogres 150, and coins add 5. The
knight has five hearts, and a rare red flask restores two. Spawns speed up
over time, ogres arrive after the first minute, and the floor cracks as the
fight goes on. The best score is kept for as long as the cartridge runs.

## Controls

| Virtual button | Desktop key | Action                                                  |
|----------------|-------------|---------------------------------------------------------|
| D-pad          | Arrow keys  | Move in 8 directions                                    |
| A (hold)       | `Z`         | Throw axes. Holding it locks the aim so you can back off |
| B              | `X`         | Dash: a short invulnerable burst with a cooldown        |
| A on a screen  | `Z`         | Start or retry                                          |

`Esc` pauses the cartridge.

## Running it

From the repository root:

```sh
mise run sync-examples -- signal-lost
mise run glaux
```

Then type `load signal-lost` and `run` in the Glaux shell.

## Structure

`code/main.luau` runs the title, playing and game-over screens and handles
collisions and scoring. Each kind of thing in the game has its own module:
`player`, `enemies`, `projectiles`, `pickups`, `particles`, `audio` and
`arena`. `objects.luau` turns sprites into hardware objects, `util.luau` holds
the random number generator and small helpers, and `sheet.luau` names the
sprite tiles.

The arena is built from three maps in `maps/`:

- `arena`, with the floor, the walls and the HUD;
- `arena-worn`, the cracked floor that is revealed as the game goes on;
- `hud-hearts`, with the heart row for each amount of health.

Open them in the Map Editor, and the background tiles in the Sprite editor,
to change how the arena looks.

## What it shows

- Three background layers with their own scroll and priority: a floor, walls
  that shake when the knight is hit, and a HUD that stays in front.
- Maps loaded and partly copied into the background layers at run time.
- Every character and item drawn as a hardware object, with flipping and
  per-sprite palettes.
- Sound effects on the four PSG channels.
- A deterministic game: the same inputs always replay the same run.

## Credits

All art comes from **0x72's DungeonTileset II v1.7**
(<https://0x72.itch.io/dungeontileset-ii>), released under CC0. Files used,
from `0x72_DungeonTilesetII_v1.7/frames/`:

| Use             | Files                                                          | Adaptation                                |
|-----------------|----------------------------------------------------------------|-------------------------------------------|
| Knight (player) | `knight_m_idle_anim_f0-3.png`, `knight_m_run_anim_f0-3.png`    | Crop the empty top 4 rows (16×28 → 16×24) |
| Goblin          | `goblin_run_anim_f0-3.png`                                     | None                                      |
| Masked orc      | `masked_orc_run_anim_f0-3.png`                                 | Bottom-align 16×23 in 16×24               |
| Ogre            | `ogre_run_anim_f0-3.png`                                       | Crop the empty top 4 rows (32×36 → 32×32) |
| Axe             | `weapon_throwing_axe.png`                                      | Center in 16×16                           |
| Coin            | `coin_anim_f0-3.png`                                           | Center in 8×8                             |
| Skull           | `skull.png`                                                    | Crop to its 6×6 content, center in 8×8    |
| Flask           | `flask_big_red.png`                                            | None                                      |
| Hearts          | `ui_heart_full.png`, `ui_heart_half.png`, `ui_heart_empty.png` | Center 13×12 in 16×16                     |
| Wall            | `wall_mid.png`, `wall_top_mid.png`                             | None                                      |
| Floor           | `floor_1.png` … `floor_8.png`                                  | None                                      |

No pixels were redrawn and no colors were changed.

## Rebuilding the sprite sheet

The sprites come from `sprites.png`, which `tools/build_sheet.py` builds from
the source pack. To rebuild it and import it again, from the repository root:

```sh
mise run signal-lost-sheet -- "/path/to/0x72_DungeonTilesetII_v1.7/frames"
mise run glaux -- import sprites examples/signal-lost/sprites.png
```

The background tiles and maps are not rebuilt by this; edit them in Glaux.
