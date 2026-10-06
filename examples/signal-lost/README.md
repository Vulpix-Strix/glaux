# Signal Lost

A single-screen arena survival shooter, and the first Glaux cartridge built as
a game rather than a demo. A knight is trapped in a dungeon arena. Goblins,
then masked orcs, then ogres pour in from the edges. Throw axes, dash through
gaps, grab the coins they drop, and see how long you last.

## Controls

| Button | Action |
|---|---|
| D-pad | Move (8 directions) |
| A (hold) | Throw axes in the facing direction. While A is held, the aim stays locked, so you can back away while throwing. |
| B | Dash: a short invulnerable burst, then a cooldown (shown under `DASH` in the HUD) |
| A on the title / game-over screen | Start / retry |

## Objective

Survive. Enemies score 10 (goblin), 30 (orc) or 150 (ogre); coins add 5.
The knight has five hearts. Goblins take half a heart, orcs a full heart and
ogres one and a half. A rare red flask restores two hearts. Spawns speed up
continuously: orcs join after 15 s, enemies arrive in pairs after 45 s, ogres
appear every 20 s after 50 s, and at about 70 s the spawn rate peaks. The best
score is kept across retries for as long as the cartridge runs.

## Running it

Glaux loads cartridges from its data directory (`~/.local/share/glaux/cartridges`
on Linux). Copy the cartridge there. Symlinks that leave the cartridge
directory are not followed.

```sh
mkdir -p ~/.local/share/glaux/cartridges/signal-lost
cp -r examples/signal-lost/{cartridge.ron,code,sprites.bin} ~/.local/share/glaux/cartridges/signal-lost/
mise run glaux
```

Then, in the Glaux shell:

```text
load signal-lost
run
```

## Code structure

`cartridge.ron` (format v1) names `code/main.luau` as the entry. Everything
else is loaded with `require`:

| Module | Role |
|---|---|
| `main.luau` | Game state and the title → playing → game over cycle. Owns `boot`/`tick`/`draw`, collision resolution, scoring, milestones and logging. |
| `player.luau` | Movement, aim lock, dash and cooldown, fire cadence, damage and healing. |
| `enemies.luau` | Enemy kinds, the difficulty curve, edge spawns with a warning marker, chasing with separation. |
| `projectiles.luau` | Thrown axes. |
| `pickups.luau` | Coins and flasks, with lifetime, blink and magnet. |
| `particles.luau` | `pset` sparks and dust, and the skulls dead enemies leave. |
| `arena.luau` | Play-field bounds and the background. |
| `audio.luau` | Every sound effect, on the PSG. |
| `util.luau` | xorshift32 PRNG, AABB overlap, vector helpers, clock formatting. |
| `sheet.luau` | **Generated** object-tile indices for every sprite (see below). |

Modules are plain tables of functions. Run state is one table that
`main.luau` creates in `start_run()`. A retry discards it and builds a fresh
one inside the same VM, without restarting the runtime. Three things carry
over between runs: the PRNG stream, the best score and the run counter.

## Glaux APIs exercised

- **Lifecycle:** `boot()` seeds the PRNG and enters the title screen. `tick()` runs the simulation and `draw()` only draws.
- **Modules:** ten Luau files joined with `require()`, some of them shared (e.g. `util`, `sheet`).
- **Input:** `btn()` for held movement and fire, `btnp("b")` for the dash, `btnp("a")` to start and retry.
- **Time:** `frame()` drives animation, spawn timing and the difficulty curve. `t()` drives the HUD clock. No host clock is used.
- **Drawing:**
  - `cls()` for the floor;
  - `spr()` single-tile for coins and skulls;
  - `spr(..., w, h)` multi-tile at 2×2 (goblins, axes, hearts, walls, floor), 2×3 (knight, orcs) and 4×4 (ogres);
  - `pset()` for particles, spawn warnings, floor joints, the arena edge and the dash bar;
  - `print()` for the HUD and screen text.
- **Logging:**
  - `log.info` on boot, run start/restart, every 30-second difficulty milestone and game over;
  - `log.debug` when an ogre enters;
  - `log.warn` once per run if the enemy cap ever throttles spawning;
  - `log.trace` for the PRNG state at run start.
- **Audio:**
  - `audio.env`, `audio.tone` and `audio.gate` on all four channels, using square, triangle, saw and noise;
  - every effect retriggers with a zero-sustain envelope and a per-tick pitch sweep, then closes its gate when the sweep ends;
  - game over and restart call `silence()` first, so no stale sound survives a transition.

The same seed and the same input sequence replay the same game (the tests
check this).

## Assets

All art comes from **0x72's DungeonTileset II v1.7**
(<https://0x72.itch.io/dungeontileset-ii>), which the author publishes under
CC0. The repository keeps only the derived sheet, never the source pack.

Source files used, all from `0x72_DungeonTilesetII_v1.7/frames/`:

| Use | Files | Adaptation |
|---|---|---|
| Knight (player) | `knight_m_idle_anim_f0-3.png`, `knight_m_run_anim_f0-3.png` | Crop the empty top 4 rows (16×28 → 16×24); add a mirrored copy |
| Goblin | `goblin_run_anim_f0-3.png` | Mirrored copy |
| Masked orc | `masked_orc_run_anim_f0-3.png` | Bottom-align 16×23 in 16×24; mirrored copy |
| Ogre | `ogre_run_anim_f0-3.png` | Crop the empty top 4 rows (32×36 → 32×32); mirrored copy |
| Axe | `weapon_throwing_axe.png` | Center in 16×16; mirrored copy |
| Coin | `coin_anim_f0-3.png` | Center in 8×8 |
| Skull | `skull.png` | Crop to its 6×6 content, center in 8×8 |
| Flask | `flask_big_red.png` | — |
| Hearts | `ui_heart_full.png`, `ui_heart_half.png`, `ui_heart_empty.png` | Center 13×12 in 16×16 |
| Wall | `wall_mid.png`, `wall_top_mid.png` | — |
| Floor | `floor_1.png` … `floor_8.png` | — |

No pixels were redrawn. The only edits are cropping, padding, mirroring and
the palette remap below.

### Palette adaptation

On G1, object-tile index 0 is transparent and indices 1–15 are written
straight to the framebuffer as master-palette indices: there are no OBJ
palette banks yet, and the PNG's own palette is ignored by the converter.
Every sprite is therefore drawn with Zughy 32 entries 1–15 (browns, skin
tones, greens, grays). The art was chosen to survive that: the wizard, demons
and blue flasks were rejected because they lose their identity without blue
or a saturated red.

`tools/build_sheet.py` maps each source color to the nearest of those 15
entries (redmean-weighted RGB distance). Three overrides are pinned by hand
because nearest-match collapses them:

| Source | Nearest | Pinned to | Why |
|---|---|---|---|
| `#5c7a56` | 9 | 8 `#71aa34` | the goblin's two greens merged |
| `#facb3e` | 5 | 6 `#f4cca1` | the coin's highlight merged with its body |
| `#da4e38` | 3 | 5 `#eea160` | the full heart was nearly the same as the empty one |

The full source → index table is printed every time the sheet is built.

### Rebuilding the sheet and `sprites.bin`

`tools/build_sheet.py` packs every sprite on the 16-column object-tile grid,
in bands of equal height so that no multi-tile sprite crosses a grid row. It
writes:

- `sprites.png`: a 4-bit indexed PNG whose palette is Zughy entries 0–15, so it previews correctly, with index 0 transparent;
- `code/sheet.luau`: each sprite's top-left tile index and size.

Python and `uv` are declared in `mise.toml`, and `uv` installs Pillow from the
script's inline metadata. From the repository root:

```sh
mise install
mise run signal-lost-sheet -- "/path/to/0x72_DungeonTilesetII_v1.7/frames"
mise run glaux -- convert sprites examples/signal-lost/sprites.png
```

The last command writes `examples/signal-lost/sprites.bin` (384 tiles,
12 288 bytes). Both steps are deterministic: rebuilding from the same pack
reproduces all three files byte for byte.

## Development notes

Signal Lost was written against the APIs as they stand, with no engine
changes. These are the notes it produced.

### What worked well

- **`cartridge.ron` + `require`.** Splitting the game into ten files took no
  ceremony. Module caching meant shared modules (`util`, `sheet`) loaded once,
  and error locations name cartridge-relative paths.
- **Logical time.** `frame()` and `t()` advance only with successful ticks.
  Combined with a script-side PRNG, the whole game replays exactly from an
  input script, which made the integration tests straightforward.
- **One VM for the whole session.** Restarting means rebuilding a Lua table;
  nothing in Glaux needed resetting.
- **The PSG API.** It is small but sufficient. Zero-sustain envelopes plus
  explicit `gate(true)` retriggers give crisp one-shot effects, and per-tick
  `audio.tone` updates give pitch sweeps without any sequencer.
- **`glaux convert sprites`.** Its contract is simple and strict (indexed
  PNG, multiple of 8, ≤ 16×64 tiles), which made it easy to target from an
  external script.
- **`RunningCartridge` as a test harness.** It exercises the real loader,
  module source and object-tile validation with no window.

### What felt awkward

- **The per-frame draw budget shapes the art direction.** A floor tiled with
  sprites would cost about 76 800 against a 65 536 budget, so the floor is
  `cls()` plus a handful of cracked slabs and marked joints. Overrunning the
  budget kills the cartridge, so every entity list has a hard cap sized to
  keep the worst-case frame near 50 k. There is no way to query the remaining
  budget.
- **`pset` is the only fill.** The first HUD was a dark 320×16 band, which
  took 5 120 `pset` calls per frame. With the dotted floor joints that was
  about 12 000 script calls, and a debug-build tick took about 8 ms of the
  16.7 ms frame. The HUD moved onto the wall sprites and the joints became
  corner marks (about 2 600 calls). A filled-rectangle primitive would have
  removed the problem outright.
- **Tile indices are manual bookkeeping.** Nothing ties a name to a tile
  range, so the sheet builder generates `sheet.luau` to keep code and sheet in
  sync. Without that step every layout change would mean editing numbers by
  hand.
- **Hit feedback has to blink.** Enemies cannot flash white or red when hit,
  because sprite colors cannot be remapped; skipping frames is the only
  feedback available.
- **Audio writes must be committed per tick.** Envelope and gate writes stay
  pending until the host commits them, up to 256. A test harness that ticks
  without committing (as the first draft of these tests did) eventually fails
  inside `audio.gate`. The host commits after every tick, so the game is
  fine, but the coupling is easy to miss.
- **Tests can only observe draw commands and logs.** Assertions read
  `print` text and sprite counts because there is no way to inspect script
  state from Rust. That is workable, and it keeps tests on observable
  behavior, but it is indirect.

### What API is clearly missing

These limitations were demonstrated while building the game:

- **OBJ palette banks.** Sprites are fixed to master indices 1–15, which
  forced the palette remap above, ruled out half the asset pack, and makes
  hit flashes impossible.
- **Sprite flip.** Every character and the axe are stored twice, mirrored.
  That is 156 of the sheet's 384 tiles.
- **A filled-rectangle primitive.** See the HUD band above: it would turn
  thousands of `pset` calls into one command.

### What should NOT be added yet

- **Tilemaps / background layers.** A single fixed screen did not need
  scrolling, and the floor was solved within the budget. A tilemap would make
  the floor nicer, but nothing here *needed* one.
- **Camera.** Nothing scrolls.
- **Collision API.** AABB overlap is a four-comparison function in
  `util.luau`. Collision policy (which pairs, what happens) is game logic.
- **Animation system.** `frame() // n % #frames` covered every animation in
  the game.
- **Music / SFX sequencer for cartridges.** One-shot sweeps were enough for
  every effect. A tracker is a separate project.
- **A game framework or entity system.** Plain tables and module functions
  stayed readable at this size.
