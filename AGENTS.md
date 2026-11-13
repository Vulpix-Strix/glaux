## General

- Keep changes focused on the requested task. Avoid unrelated refactors and speculative abstractions.
- Follow existing project conventions and inspect the relevant code before introducing new patterns.
- Do not implement future features or placeholders unless the current task requires them.
- Preserve architectural boundaries and avoid unnecessary dependencies.

## Comments and documentation

- Write comments and rustdocs in clear, natural English.
- Keep documentation concise without losing important technical details.
- Avoid repetitive `Concept: description` headings, excessive semicolons, overly compressed sentences, and other mechanical writing patterns.
- Do not narrate obvious code, project phases, implementation history, or future plans.
- Explain non-obvious behavior, invariants, contracts, design decisions, and workarounds.
- Document public APIs when their behavior or contract needs explanation. Preserve useful rustdoc links and examples.
- Do not add comments merely to increase documentation coverage.
- Avoid em dashes in comments and documentation.

## Architecture

- Keep the fictional machine independent of operating systems, rendering libraries, and scripting runtimes.
- Keep host-specific behavior in host adapters, such as `glaux-host-raylib`.
- Keep language-specific behavior in the corresponding runtime crates, such as `glaux-luau` and `glaux-rhai`.
- Keep machine-level graphics semantics out of the host. The host receives indexed framebuffer data and its resolved palette.
- Do not leak host filesystem paths or asset-loading concerns into machine-level crates.
- Keep application entry points focused on wiring components together.
- Avoid dependency cycles between architectural layers.

## Machine representation

- Use explicit byte-oriented representations for hardware-visible state where appropriate.
- Never rely on Rust struct layout, padding, alignment, or native endianness to define machine formats.
- Keep host-native representations separate from machine representations.
- Treat externally observable byte layouts as part of Glaux's machine contract.

## Dependencies and errors

- Prefer existing workspace dependencies and the standard library when appropriate.
- Add dependencies only when they solve a concrete problem. Avoid introducing overlapping frameworks or unnecessary infrastructure.
- Use `thiserror` for structured library errors and `anyhow` primarily at application boundaries.
- Handle invalid input and recoverable failures without panicking.
- Reserve `unwrap()`, `expect()`, and `panic!` for genuine invariants and tests.

## Unsafe Rust

- Avoid `unsafe` unless required for FFI or justified low-level integration.
- Every `unsafe` block must have a nearby `// SAFETY:` comment explaining its soundness.
- Keep unsafe code narrowly scoped and encapsulated.

## Testing

- Write tests that protect observable behavior, contracts, invariants, or meaningful regressions.
- Prefer precise, independently derived assertions over implementation-dependent expectations.
- Cover relevant boundaries and failure paths without creating redundant tests.
- Keep tests deterministic and avoid unnecessary fixtures.
- Use property-based, snapshot, or mutation testing only when they provide meaningful value.
- Never weaken assertions merely to make tests pass.
- Do not add tests solely to increase coverage or test count.
- Place unit tests by size. Keep a short set (roughly under 60 lines) in an inline `#[cfg(test)] mod tests { ... }`. Use `foo/tests.rs` for longer ones, or `foo/tests/` split by topic when the tests are large. Do not use `#[path]` for tests.
- Move tests to a crate's `tests/` directory only when they use nothing but its public API and need no `cfg(test)` fixtures. `apps/glaux` is a binary crate, so its tests stay inside the crate.
- Do not widen visibility, or add a `lib.rs`, only to move tests.

## Tooling and validation

- Use `mise` for project tools and existing tasks. Consult `mise.toml` for available commands.
- Run `mise install` when configured tools are missing.
- Use `mise run test` for tests. Do not substitute `cargo test` for the project's nextest workflow.
- While iterating, use `mise run check` (`cargo check --workspace --all-targets`, tests included) and focused tests: `mise run test -- <name filter>` or `mise run test -- -p <crate>`. Do not run the whole suite after every small fix. A warm full run takes about 20 s and an incremental `mise run verify` 55 to 95 s, so run `verify` once, when a complete change is done.
- Run validation appropriate to the changes, rather than every available command.
- For code changes, use relevant format, compilation, lint, and test checks.
- For documentation-only changes, validate documentation and diffs without running unrelated tests.
- After changing dependencies or `Cargo.lock`, run `mise run licenses` and commit `apps/glaux/licenses/third-party.json`. `mise run verify` runs the routine checks, including `licenses-check`. See `apps/glaux/licenses/README.md`.
- Do not run mutation testing routinely.
- Profile with hotpath through opt-in Cargo features, never by default. See the Profiling section below.
- Report validation failures and distinguish existing problems from those introduced by the changes.

## Profiling

- `mise run profile -- <label>` runs every workload with timing and then with allocations, and writes JSON reports and per-phase frame times to `target/profiling/<label>/`, which is not tracked. `mise run profile-compare -- <before> <after>` lists the functions whose p95 time or allocated bytes moved by more than 25 % (`--threshold` changes it) and exits with 1 when any got worse.
- The workloads are scenarios of the ignored test `apps/glaux/src/system/tests/profiling_workload.rs`, selected with `GLAUX_WORKLOAD`: `luau` (default) and `rhai` play Signal Lost for 3600 ticks, `bg` scrolls three full opaque background layers for the same time, `editor` exercises the Code view, `analyzer` adds `luau-lsp` (needs `LUAU_LSP`, skipped without it) and `paced` is `editor` without the analyzer. They drive the real `System` without a window and assert nothing about time.
- Use the workloads to compare commits. Profile the real application (`cargo run -p glaux --profile profiling --features hotpath-alloc`) only for what they cannot see: presentation, vsync and the host. Its report is printed on exit, and `HOTPATH_SHUTDOWN_MS` and `HOTPATH_OUTPUT_PATH` give it a fixed length and a file.
- For comparable numbers, use `--profile profiling` (release with symbols), never dev, and repeat a run before trusting a change. Allocation figures are exact between runs, while times vary by around 10 % at p95. Times nest, so never add them up, and `app::present` is mostly waiting for the display.
- The features `hotpath`, `hotpath-alloc` and `hotpath-cpu` are off by default, and no hotpath macro does anything without one. Never add them to `default`. `hotpath-cpu` needs `samply` and `hotpath-samply`, and must run the test binary directly, not under nextest.
- Past measurements live only in the Git history of the former `PROFILING.md`. Do not keep a log of results in the repository.
