# System palettes

`system.hex` is the master palette: 256 RGB colors, one per line. Entries
0–31 are [Zughy 32](https://lospec.com/palette-list/zughy-32) by Zughy in its
published order; entries 32–255 are debug magenta (`#ff00ff`) so that stray
indices stand out.

`obp.txt` holds the object palette banks a cartridge starts with when it has
no `obp.bin`. Each line maps tile-local indices 1–15 of one bank to master
indices; local 0 is always transparent.

## Slot layout

Every bank follows the slot structure of OBP0, with similar lightness per
slot, so art drawn with one bank stays readable when drawn with another:

| Locals | Role                                                 | OBP0          |
|--------|------------------------------------------------------|---------------|
| 1–6    | main ramp, dark to light                             | skin and clay |
| 7–10   | secondary ramp, light to dark                        | greens        |
| 11     | outline, the darkest color (`#302c2e` in every bank) |               |
| 12–15  | neutrals or accents, dark to light                   | greys         |

Ramps are hue-shifted rather than plain light/dark steps, as in
[Nanner Pancakes](https://lospec.com/palette-list/nanner-pancakes): shadows
lean toward indigo and plum (21, 22, 0), highlights toward cream or cool white
(6, 16, 17). Every Zughy color appears in at least one bank.

## Banks

| Bank | Name    | Locals 1–15                                      | Purpose                                                                                                                                                                   |
|------|---------|--------------------------------------------------|---------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| OBP0 | Classic | 1 2 3 4 5 6 · 7 8 9 10 · 11 · 12 13 14 15        | Identity mapping: local *i* is master *i*. Skin, foliage and stone. Cartridges without `obp.bin` and sheets from `glaux convert sprites` rely on it, so it never changes. |
| OBP1 | Forest  | 21 10 9 8 7 6 · 5 4 3 2 · 11 · 20 19 18 17       | Foliage from indigo shade through teal and green to lime and cream, bark browns, sky and water blues.                                                                     |
| OBP2 | Sunset  | 1 29 28 27 26 6 · 25 24 23 22 · 11 · 12 13 14 15 | A fire ramp from maroon through red, orange and gold to cream; pink to purple dusk; warm greys for smoke and stone.                                                       |
| OBP3 | Ocean   | 21 20 19 18 17 16 · 7 8 9 10 · 11 · 22 31 30 15  | The full blue ramp up to ice white, seaweed greens, cool slate greys. Water, ice, glass.                                                                                  |
| OBP4 | Night   | 21 22 23 30 17 16 · 26 27 28 29 · 11 · 0 1 12 13 | Low key: indigo, purple and lavender lit by a moonlight cyan and white; warm lantern and fire glow; dim warm neutrals.                                                    |
| OBP5 | Candy   | 21 22 23 24 25 16 · 17 18 19 20 · 11 · 9 8 27 26 | Saturated: purple through magenta and pink to white, a cyan complement, green, orange and gold pops. Pickups, effects, UI.                                                |
| OBP6 | Steel   | 21 31 13 14 15 16 · 5 4 3 2 · 11 · 20 19 28 18   | Neutral: cool greys up to white, brass and copper, blue panels with red and cyan indicator lights. Machines, armor, UI.                                                   |
| OBP7 | Pastel  | 22 30 24 25 6 16 · 17 7 14 13 · 11 · 31 19 18 26 | High key, the counterpart of Night: lavender, pink and cream, mint and lime, soft accents. Ghosts, daylight, cute characters.                                             |

All banks were checked by drawing the same sprites (the `examples/sprites`
sheet and a figure using all fifteen locals) with each of them.
