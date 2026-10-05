# Sprite example

`sprites.png` comes from FisherG's
[Micro Asset Pack (8x8, 4 Colors)](https://fisherg.itch.io/micro-asset-pack), which the author
releases for free use in the public domain. Regenerate the native asset from the repository root
with:

```sh
mise run glaux -- import sprites examples/sprites/sprites.png
```

Tiles are indexed left-to-right and then top-to-bottom. The cartridge draws
tile 7, the player in the top-right cell, at the center of the logical screen.
