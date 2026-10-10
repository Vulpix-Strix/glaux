## Comments & documentation

- Write all comments and doc comments in English, regardless of the language used in conversation with the user.
- Use `///` doc comments on public items to document what they do now: behavior, invariants, non-obvious parameters, or return values.
- Do not narrate project phases, roadmap, or future plans in comments.
- Do comment on:
  - non-obvious invariants;
  - reasoning behind hacks or workarounds;
  - safety justifications for `unsafe` blocks;
  - anything a reader could not reasonably infer from the code itself.
- Do not add comments that simply restate the code.

**Bad:**

```rust
// For now the framebuffer is indexed, but later the renderer will support
// tilemaps and objects.
fn clear(&mut self, color: u8) {
    self.pixels.fill(color);
}
````

**Good:**

```rust
/// Fills the framebuffer with a master-palette index.
fn clear(&mut self, color: u8) {
    self.pixels.fill(color);
}
```

**Bad:**

```rust
// This is temporary and will probably be redesigned later.
```

**Good:**

```rust
// Keep the conversion at the host boundary so the machine remains
// independent of the host's native pixel format.
```

## Tooling

This project uses `mise` to manage the Rust toolchain, development tools, and common project commands.

Do not bypass `mise` for normal project workflows when an equivalent task already exists.

Before working with the project tools, ensure the configured environment is available:

```sh
mise install
```

Do not install project tools manually unless there is a concrete reason to do so.

## Testing

This project uses `cargo-nextest`, not `cargo test`.

Run tests through mise:

```sh
mise run test
```

Do not fall back to `cargo test`.

If `mise run test` fails because a configured tool is unavailable, run:

```sh
mise install
```

and retry.

Do not introduce custom nextest configuration unless there is an actual need for it.

## Preferred dependencies

Use existing workspace dependencies when they fit the task instead of introducing overlapping alternatives:

* `anyhow`: application-level error propagation and executable boundaries.
* `thiserror`: typed library and internal errors.
* `tracing` / `tracing-subscriber`: instrumentation, logging, and debugging.
* `serde` / `serde_json`: structured data and JSON serialization.
* `bitflags`: typed views over bit-based flags and registers where appropriate.
* `raylib`: desktop host integration.
* `mlua`: Luau runtime integration.
* `pretty_assertions`: clearer assertion output in tests.

Do not add another crate that overlaps with one of these without a concrete reason.

## Errors

* Use `thiserror` for structured errors exposed by library crates.
* Use `anyhow` primarily at application boundaries where preserving a concrete public error type is unnecessary.
* Do not use panics to report ordinary invalid input, malformed cartridge data, runtime errors, or recoverable host failures.
* `panic!`, `unwrap()`, and `expect()` are acceptable only for genuine invariants or test code.
* Error messages should explain what failed and provide useful context where possible.

## Commands

Use the project's mise tasks rather than invoking the underlying Cargo commands directly.

Check the workspace:

```sh
mise run check
```

Format check:

```sh
mise run fmt
```

Lint:

```sh
mise run clippy
```

Tests:

```sh
mise run test
```

Before submitting changes, run the relevant validation commands. For normal code changes, this generally means:

```sh
mise run fmt
mise run clippy
mise run test
```

Use `mise run check` when a full test run is unnecessary or while iterating on compilation errors.

Mutation testing is optional and not part of `mise run test`:

```sh
mise run mutants                                      # glaux-machine
mise run mutants -- --file crates/glaux-video/src/draw.rs
```

It can consume several GB of memory, especially when a mutation allocates inside frequently executed code. Apply an OS-level memory limit where available (for example `systemd-run --user --scope -p MemoryMax=8G` on Linux).

## Tests

Tests must protect meaningful behavior, not merely increase test count or coverage.

Before writing a test, ask: **What plausible regression would make this test fail?** If there is no clear answer, reconsider its value.

### Quality

- Test observable behavior, contracts, invariants, and known regressions rather than implementation details.
- Prefer precise assertions with independently derived expected results.
- Do not mirror production algorithms, verify test setup, or test behavior guaranteed by Rust's type system.
- Ensure invalid inputs fail for the intended reason, not an unrelated error.
- Cover relevant boundaries, state transitions, and failure paths, including unchanged state after rejected operations.
- Avoid redundant tests unless they protect distinct scenarios or architectural boundaries.
- Prefer strengthening existing tests over adding superficial ones.
- Keep simple tests when they protect meaningful contracts.

### Organization

- Keep tests in dedicated files, using colocated `#[cfg(test)] mod tests;` modules when private access is needed.
- Use crate-level `tests/` for integration tests exercising public APIs.
- Keep fixtures close to their consumers and share them only when useful.
- Do not widen production visibility solely for testing.

### Tools and strategies

- Use unit tests for isolated behavior and integration tests for interactions between components.
- Use `proptest` selectively for meaningful invariants over large input spaces. Keep failures reproducible.
- Use `cargo-mutants` to investigate test effectiveness in critical code. Do not run it routinely or chase mutation scores.
- Use `insta` only when snapshots improve verification of complex outputs. Review snapshot changes carefully.
- Prefer deterministic framebuffer and palette assertions for graphics. Isolate real Raylib/GPU/windowing tests from the normal suite.
- Use minimal fixtures unless real assets or formats are part of the behavior being tested.

### Maintenance

- Add regression tests for bugs when they can reproduce the failure.
- Update tests when intended behavior changes; remove obsolete tests and unused helpers.
- Never weaken assertions merely to make tests pass.
- Do not require a new test for every code change when existing coverage is sufficient.

**Every test should have a clear purpose and detect a meaningful regression.**

## Dependencies

Prefer the standard library when it keeps the implementation clear.

Before adding a new dependency:

1. Check whether the workspace already has a crate for the same purpose.
2. Prefer a focused, mature dependency over a large framework.
3. Keep host-specific dependencies out of machine-level crates.
4. Do not add game engines, ECS frameworks, async runtimes, dependency-injection frameworks, or similar infrastructure unless the project has a concrete need for them.

Dependencies should remove incidental work, not replace the parts of Glaux that are themselves the subject of the project.

## Architecture boundaries

Keep Glaux's conceptual machine independent from host implementation details.

* `glaux-machine` must not depend on raylib, operating-system windowing APIs, or a specific scripting runtime.
* Host-specific code belongs in host adapter crates such as `glaux-host-raylib`.
* `glaux-video` must remain independent of raylib and other host presentation libraries.
* The host should receive presentation-ready logical data rather than access the full machine state.
* Luau-specific behavior belongs in the Luau integration crate when it is introduced.
* Filesystem paths and host asset loading must not leak into the machine model.
* System ROM data should be injected into the machine rather than loaded by `glaux-machine` from repository-relative files.
* Keep application entry points focused on wiring components together rather than implementing domain logic.

Do not introduce dependencies that create cycles between machine, runtime, language, video, or host layers.

## Machine representation

Glaux models a fictional machine, so representation choices are part of the design.

* Hardware-visible state should use explicit byte-oriented representations where appropriate.
* Do not let Rust struct layout, padding, alignment, or endianness accidentally define Glaux's machine format.
* Use accessors to interpret raw state rather than relying on direct struct-to-byte casts.
* Keep host-native representations separate from machine representations.

When a byte layout becomes externally observable or addressable, treat changes to it as machine-format changes rather than ordinary refactors.

## Graphics boundaries

The logical framebuffer is indexed.

A framebuffer byte represents an index into the 256-entry palette supplied alongside that frame.

The host should not need to understand:

* tile formats;
* BGP or OBP banks;
* OAM;
* tilemaps;
* cartridge graphics rules.

Those concepts must be resolved before presentation reaches the host boundary.

Do not move palette-bank resolution or other machine-level graphics semantics into `glaux-host-raylib`.

## Unsafe Rust

Avoid `unsafe` unless it is required for FFI or demonstrably necessary low-level integration.

Every `unsafe` block must have a nearby `// SAFETY:` comment describing the invariant that makes it sound.

Keep unsafe code narrowly scoped.

If an FFI integration requires unsafe state or pointers, encapsulate that boundary instead of allowing unsafe assumptions to spread through the workspace.

## External libraries and native tooling

Do not assume native dependencies or development tools are available outside what the project declares through mise and Cargo.

When a task depends on an external executable or native library:

* verify that it is available;
* report clearly when it is required;
* do not silently replace it with another implementation.

Raylib and Luau are implementation dependencies of specific adapters/runtimes, not concepts that should leak into unrelated crates.

## Don't

* Don't write out the full project/crate structure in comments or documentation merely to mirror the filesystem. Check the repository instead.
* Don't add abstractions solely because they might be useful later.
* Don't introduce a trait before there is a real consumer that needs the abstraction.
* Don't put host-specific behavior into the machine.
* Don't make the machine load repository-relative assets itself.
* Don't use `cargo test` instead of the project's nextest workflow.
* Don't bypass existing mise tasks for routine validation.
* Don't add new dependencies without checking the workspace dependencies first.
* Don't turn `main.rs` into the place where domain logic lives.
* Don't implement future systems as placeholders unless the current task requires them.

