# require

Loads a cartridge module and returns its first result. This is the Luau way to share code between files.

## Parameters

| Name   | Meaning                                                                       |
|--------|-------------------------------------------------------------------------------|
| `name` | A module name such as `"game.enemies"`, which loads `code/game/enemies.luau`. |

## Returns

The module's first return value, or `true` if it returns nothing. The value is cached and every later `require` of the same module returns it.

## Notes

A module runs at most once, in the same VM and with the same globals as the entry chunk. One that failed raises the same error on every later `require`. Requiring a module that is still loading is a cyclic require and fails. The module's time counts against the call that first required it. Names use lowercase segments joined by dots, and only cartridge modules can be named.

## Examples

### Luau

```luau
local enemies = require("game.enemies")
```

## See also

- [import (Rhai)](glaux://api/modules/import)
