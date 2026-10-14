# import

Loads a cartridge module under a name. This is the Rhai way to share code between files, and it is syntax, not a function.

## Parameters

| Name   | Meaning                                                                       |
|--------|-------------------------------------------------------------------------------|
| `name` | A module name such as `"game.enemies"`, which loads `code/game/enemies.rhai`. |

## Notes

The module's top level runs at most once and its functions and exported variables are cached for every later `import`. A module that failed fails the same way every time, and importing a module that is still loading is a cyclic import. Module functions are called through the alias and cannot see `this`, so pass them the state they need and store what they return.

## Examples

### Rhai

```rhai
import "game.enemies" as enemies;

fn tick() {
    this.enemy = enemies::step(this.enemy);
}
```

## See also

- [require (Luau)](glaux://api/modules/require)
