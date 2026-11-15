# map

Reads the cartridge's maps and copies regions of them into the background tilemap windows.

## Notes

A map is made in the Map Editor and saved under `maps/<id>/`. Its id is the name of that directory. A map can be up to 256 by 256 tiles and has one layer for each background, BG0 to BG2.

Each background shows a window of 64 by 64 tiles that wraps on both sides. [load](glaux://api/map/load) and [copy](glaux://api/map/copy) write cells of a map into a window: the cell at world position `(x, y)` lands at `(x mod 64, y mod 64)`, so a map larger than the window can be streamed in as the view scrolls. [size](glaux://api/map/size) and [get](glaux://api/map/get) read a map without touching the hardware.

The windows start empty, even when the cartridge has maps. The maps are a read-only copy: writing a window, with these functions or with [poke16](glaux://api/memory/poke16), never changes them. The functions work in any phase, and each copied cell costs as much as copying two bytes with [memcpy](glaux://api/memory/memcpy).

### Luau

The functions are fields of the `map` table: `map.load(...)`.

## See also

- [load](glaux://api/map/load)
- [copy](glaux://api/map/copy)
- [size](glaux://api/map/size)
- [get](glaux://api/map/get)
- [hardware](glaux://api/hardware/index)
