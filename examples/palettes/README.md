# Palettes

A sprite sheet drawn in true color and imported with its colors intact. The
cartridge needs no palette setup: `spr()` draws each sprite with the palette
the importer chose for it, and an optional last argument draws it through
another palette instead.

## Files

| File                  | Written by                    | Contents                                                |
|-----------------------|-------------------------------|---------------------------------------------------------|
| `sprites.png`         | the artist                    | the sheet, RGBA, transparent pixels with alpha 0        |
| `sprites.sprites.ron` | the artist                    | sprite groups: tiles that must share one palette        |
| `sprites.bin`         | `glaux import sprites`        | 4 bpp object tiles                                      |
| `objattr.bin`         | `glaux import sprites`        | each tile's default object palette                      |
| `obp.bin`             | `glaux import sprites`        | the eight object palettes (OBP0-OBP7)                   |
| `palette.bin`         | `glaux import sprites`        | the cartridge's master palette, with the sheet's colors |
| `sprites.import.ron`  | `glaux import sprites`        | what the last import wrote, so a re-import is safe      |

## Re-importing

After editing the PNG or the groups, run from the repository root:

```sh
mise run glaux -- import sprites examples/palettes/sprites.png
```

The import replaces the cartridge's sprites and sprite palettes. It refuses
to overwrite any of those files that it did not write itself, or that changed
since it wrote them, unless `--replace` is given.

## Running it

Copy the cartridge into Glaux's cartridge directory, as for the other
examples, then `load palettes` and `run` in the shell.
