# System UI images

Pixel art the system draws its UI with. Each image is decoded into ink
slots (transparent, outline, shade or body, light), and the system picks the
palette index of each slot when it draws, so the files keep their original
colors.

## Cursors

The mouse pointers come from
[Simple Pixel Cursors](https://gabl18.itch.io/simple-pixel-cursors) by
**Gabl** (<https://gabl18.itch.io/>), the light set, copied unmodified.
Glaux trims their transparent edges and shrinks the ones drawn at twice
their native size. Hotspots are given in pixels of one frame as stored.

| File                    | Source file           | Cursor | Stored as                          | Hotspot  |
|-------------------------|-----------------------|--------|------------------------------------|----------|
| `cursors/pointer.png`   | `light/pointer.png`   | Arrow  | 2×                                 | (0, 0)   |
| `cursors/beam.png`      | `light/beam.png`      | Text   | 2×                                 | (8, 15)  |
| `cursors/link3.png`     | `light/link3.png`     | Hand   | 2×                                 | (8, 0)   |
| `cursors/grab.png`      | `light/grab.png`      | Grab   | 2×                                 | (14, 16) |
| `cursors/hourglass.png` | `light/hourglass.png` | Busy   | 1×, five 16 px frames side by side | (7, 7)   |

The arrow halves to 12×16 with its tip as hotspot. The hourglass shows each
frame for 150 ms, as the pack's `hourglass.gif` does.

Licensed under CC0 1.0 Universal. Credit is not required but is given here
gladly. The pack's license and credits are in `LICENSE-Gabl.txt`, copied
unmodified. That file also asks that its resources not be used to feed
LLM/AI models; please respect that request.

## Icons

The workspace tab icons and the sprite editor's tool icons come from
[1-bit Pixel Icons](https://nikoichu.itch.io/pixel-icons) by **Nikoichu**,
copied unmodified and renamed. Each is 16×16 with a black outline
(`#000000`) and white fill (`#ffffff`).

| File                   | Source file                                                                  |
|------------------------|------------------------------------------------------------------------------|
| `icons/code.png`       | `Software_Terminal_Window_CMD_Command_Line_Development_Code_Programming.png` |
| `icons/sprites.png`    | `Tools_Crafting_Graphic_Design_Pencil.png`                                   |
| `icons/map.png`        | `Map_Markers_Travel_Map_Folded.png`                                          |
| `icons/sfx.png`        | `Media_Audio_Sound_Volume_3.png`                                             |
| `icons/music.png`      | `Media_Musical_Note_Beam.png`                                                |
| `icons/eraser.png`     | `Tools_Crafting_Graphic_Design_Eraser.png`                                   |
| `icons/eyedropper.png` | `Tools_Crafting_Graphic_Design_Eyedropper_Color_Picker.png`                  |
| `icons/fill.png`       | `Tools_Crafting_Graphic_Design_Paint_Bucket_Fill.png`                        |
| `icons/settings.png`   | `Software_Options_Settings_Cogwheel_Gear_Mechanics.png`                      |

The sprite editor's pencil tool reuses `icons/sprites.png`. The settings
cogwheel's art fills only the central 12×12 pixels of its canvas, so it fits
the Code view's options bar.

The author's page states: "All of these icons are licensed under CC0, which
means you have absolutely no restrictions on how you can use them. I would
still appreciate it if you gave me attribution for my work, but it's not
strictly required." Thanks, Nikoichu :D!

## Logo

`logo.png` is the official Glaux logo, 32x32 pixels, shown above the name on
the credits title card. The credits view draws it as large as fits a 32 pixel
square, so a replacement of that size needs no other change.

Unlike the icons it keeps its colors: every opaque pixel must be exactly one
of the first 32 colors of the system palette (`../palette/system.hex`), and
transparent pixels are left alone. Loading fails with the offending color
otherwise.
