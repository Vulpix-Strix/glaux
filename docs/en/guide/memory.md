# Memory

A cartridge can read and write the machine's memory directly. This is how a script sets up tiles, sprites and registers, and how it works on pixels without calling a drawing function. The functions are Luau only: [peek](glaux://api/memory/peek), [poke](glaux://api/memory/poke), [peek16](glaux://api/memory/peek16), [poke16](glaux://api/memory/poke16), [peek32](glaux://api/memory/peek32), [poke32](glaux://api/memory/poke32), [memcpy](glaux://api/memory/memcpy) and [memset](glaux://api/memory/memset).

## The address space

Addresses are 20 bits, from 0 to 1048575 (`0xFFFFF`). Reading an address that no device uses gives 0, and writing to it does nothing. An address outside the space is an error, and so is a block or a 16 or 32-bit access that reaches past its end.

| Start     | Device                                   |
|-----------|------------------------------------------|
| `0x00000` | Work RAM, 64 KiB for the script's own use |
| `0x10000` | Framebuffer, one palette index per pixel, 320 by 240 |
| `0x24000` | Background tiles                         |
| `0x2C000` | Object tiles                             |
| `0x34000` | Background 0, 1 and 2 tilemaps           |
| `0x3A000` | Master palette, then the background and object palettes and object attributes |
| `0x3B000` | OAM, the sprite entries                  |
| `0x3C000` | Registers                                |

There is one memory. What a script reads is what the machine uses, and the drawing functions work on the same framebuffer.

## Reading and writing

Multi-byte values are little-endian: the lowest byte comes first. A 16 or 32-bit access may cross from one device into the next.

Values must be whole numbers in range. A value outside 0 to 255 for [poke](glaux://api/memory/poke), 0 to 65535 for [poke16](glaux://api/memory/poke16) or 0 to 4294967295 for [poke32](glaux://api/memory/poke32) is an error, and so is a negative number or a fraction. Nothing is rounded, clamped or wrapped. To keep only the low byte of a number, use `bit32.band(value, 0xFF)`.

Each device decides what a write does. Some bits are reserved and are dropped, so reading back can give a different byte than the one written. Entry 0 of each background and object palette is reserved, bit 15 of a tilemap entry is not stored, and registers keep only the bits they use.

[memcpy](glaux://api/memory/memcpy) copies as if the source were read completely first, so overlapping blocks are safe. A block whose range is not valid is rejected as a whole and nothing is written.

## Memory in each callback

Memory can be read and written at the top level, in `boot()`, in `tick()` and in `draw()`. What a script writes stays until it changes it again, with one exception.

The framebuffer is cleared to the backdrop color just before each `draw()`, and then the background layers and the objects are drawn on it. Pixels a script writes there at the top level, in `boot()` or in `tick()` are gone when `draw()` starts. During `draw()` memory writes and drawing functions share the framebuffer, and the later one wins.

Bits 0, 1 and 2 of the register at `0x3C000` switch background 0, 1 and 2 on, and bit 3 switches the objects on. All of them are off at power-on, so nothing is drawn until a script sets them. Layers and objects read their memory at the moment the frame is composed, after `tick()` and before `draw()`. Memory a script changes during `draw()` shows up in the next frame, not in the one being drawn.

The frame the player sees is published when `draw()` finishes without an error. It holds the framebuffer and the master palette as they were at that moment, so changing the palette in `draw()` shows up in that same frame. If `draw()` fails, the writes it made stay in memory, but the frame on screen stays the last one that was published.

## The cost of memory operations

Every call into a script function counts against the call's execution budget, as loop iterations do. Memory functions add to it so that a single call cannot do a lot of work for free.

| Function                                   | Budget units charged            |
|--------------------------------------------|---------------------------------|
| `peek`, `poke`, and the 16 and 32-bit forms | 1                               |
| `memcpy`                                   | 1 plus 1 per byte               |
| `memset`                                   | 1 plus 1 per 2 bytes, rounded up |

The default budget is 5 000 000 units per call to the top level, `boot()`, `tick()` or `draw()`. Clearing the whole framebuffer with `memset` costs about 38 000, less than 1 percent. Copying the entire address space with `memcpy` costs about a fifth of the budget.

A call that does not fit stops the script, as a loop that runs too long does, and `pcall` cannot catch it. The block is not written. A call with invalid arguments or an invalid range costs nothing. The budget keeps counting while a script is stopped at a breakpoint, and the time spent stopped is not counted.
