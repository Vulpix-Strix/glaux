# log

Records messages in the cartridge log. The functions work in any phase.

## Notes

Levels from quietest to loudest are `trace`, `debug`, `info` and `warn`.

Glaux keeps a bounded log. A tick keeps a few hundred messages, a few more for `warn` so a flood of quieter ones cannot hide it, and a very long message is cut. What does not fit is dropped, and the log panel (F12 while the cartridge runs) says how many.

### Luau

Functions are fields of the `log` table, such as `log.info(...)`. Each accepts any number of values and joins them with tabs.

### Rhai

Functions live in the `log` module, such as `log::info(value)`, and take one value. There is no `log::debug`, because `debug` is a Rhai keyword. Rhai's own `print(value)` logs at info level and `debug(value)` at debug level.

## See also

- [trace](glaux://api/logging/trace)
- [debug](glaux://api/logging/debug)
- [info](glaux://api/logging/info)
- [warn](glaux://api/logging/warn)
