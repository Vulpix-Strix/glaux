# Vendored tree-sitter-rhai

Generated parser from <https://github.com/elkowar/tree-sitter-rhai>, commit
`4ac7384d487ffcb54e746ef1569585a749370c5b` (2026-07-09). Only `src/parser.c` and
the headers it includes are kept. The grammar declares the MIT license in its
`tree-sitter.json` but ships no license file.

The upstream project has no Rust bindings, no external scanner, and parser ABI
14. Glaux compiles `parser.c` from `build.rs` and uses its own highlight query
instead of the upstream one.
