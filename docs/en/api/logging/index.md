# log

Records messages in the cartridge log. The functions work in any phase.

## Notes

Levels from quietest to loudest are `trace`, `debug`, `info` and `warn`.

### Luau

Functions are fields of the `log` table, such as `log.info(...)`. Each accepts any number of values and joins them with tabs.

### Rhai

Functions live in the `log` module, such as `log::info(value)`, and take one value. There is no `log::debug`, because `debug` is a Rhai keyword. Rhai's own `print(value)` logs at info level and `debug(value)` at debug level.

## See also

- [trace](glaux://api/logging/trace)
- [debug](glaux://api/logging/debug)
- [info](glaux://api/logging/info)
- [warn](glaux://api/logging/warn)
