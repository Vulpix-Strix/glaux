# System UI images

Pixel art the system draws its UI with. Each image is decoded into ink
slots (transparent, outline, shade or body, light), and the system picks the
palette index of each slot when it draws, so the files keep their original
colors.

## Pointer

`cursors/pointer.png` is `cursors/light/pointer.png` from
[Simple Pixel Cursors](https://gabl18.itch.io/simple-pixel-cursors) by
**Gabl** (<https://gabl18.itch.io/>), copied unmodified. It is drawn at twice
its native size; Glaux halves it to a 12×16 arrow whose hotspot is its
top-left pixel.

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

The sprite editor's pencil tool reuses `icons/sprites.png`.

The author's page states: "All of these icons are licensed under CC0, which
means you have absolutely no restrictions on how you can use them. I would
still appreciate it if you gave me attribution for my work, but it's not
strictly required." Thanks, Nikoichu :D!
