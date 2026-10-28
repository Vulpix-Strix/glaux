# Profiling Glaux

Glaux uses [hotpath](https://hotpath.rs) for development profiling. Every hotpath macro is a no-op unless a Cargo feature is on, so normal builds contain no profiling code and compile none of hotpath's dependencies. hotpath does not touch `tracing`: the log file, the log panel and `RUST_LOG` behave as usual with a hotpath feature on.

## Features

| Feature | Adds | Needs |
|---|---|---|
| `hotpath` | Wall-clock time per instrumented function, and per-thread CPU use. | Nothing. |
| `hotpath-alloc` | Allocated bytes per function. Installs a counting global allocator. | Nothing. |
| `hotpath-cpu` | CPU samples per function, from [samply](https://github.com/mstange/samply). | `samply` and `hotpath-samply`, see below. |

None of them is in `default`. Build with `--profile profiling` (release plus symbols) for numbers that mean something. The dev profile compiles `glaux` itself without optimizations and measured about eight times slower for `system::render`.

## Two ways to run it

### The workloads (repeatable, no window)

`apps/glaux/src/system/tests/profiling_workload.rs` is an ignored test that drives the real `System` and the real System ROM through the same `render`, `update` and audio calls the application makes every frame, without a window. It is the way to get numbers that can be compared between commits. It copies the example cartridge to a scratch directory, never saves, and asserts nothing about time.

| `GLAUX_WORKLOAD` | What it does |
|---|---|
| `luau` (default) | Plays Signal Lost for 3600 ticks, one minute of game time, with a fixed pattern of buttons. |
| `rhai` | The same for the Rhai port, `signal-lost-rhai`. |
| `editor` | In the Code view of Signal Lost: scrolling, navigating, typing with completion and signature help, hovering a grid of places, go to definition and back, opening every module and undoing. |

```sh
mise run profile -- my-label            # all three, timing and allocations
mise run profile-compare -- before after
```

`mise run profile` writes JSON reports and the per-phase frame times to `target/profiling/<label>/`, which is not tracked. `profile-compare` lists the functions whose p95 time or total allocated bytes moved by more than 25 % (`--threshold` changes it) and exits with 1 when any got worse. To run one scenario by hand, with the test's own per-phase table printed:

```sh
GLAUX_WORKLOAD=editor HOTPATH_METRICS_SERVER_OFF=1 \
  cargo nextest run --cargo-profile profiling -p glaux --features hotpath-alloc \
  --run-ignored only --no-capture -E 'test(profiling_workload::workload)'
```

The same command without `--features` runs the workload with hotpath fully off and still prints the per-phase table, which is the way to check what profiling costs.

### The application (real window)

```sh
cargo run -p glaux --profile profiling --features hotpath-alloc
```

The report is printed when Glaux exits normally. Add `HOTPATH_SHUTDOWN_MS=20000 HOTPATH_OUTPUT_PATH=report.txt` for a fixed-length run. There is no launch argument that opens a cartridge, so a windowed run only covers what you do by hand. Use it for what the workloads cannot see: presentation, vsync and the host.

Useful variables (all are listed in the [configuration page](https://hotpath.rs/configuration)):

| Variable | Effect |
|---|---|
| `HOTPATH_SHUTDOWN_MS` | Quit and print the report after this many milliseconds. |
| `HOTPATH_OUTPUT_PATH` | Write the report to a file instead of stdout. |
| `HOTPATH_OUTPUT_FORMAT` | `table` (default), `json`, `json-pretty` or `none`. |
| `HOTPATH_FOCUS` | Only report functions whose name contains this text, or matches `/regex/`. |
| `HOTPATH_ALLOC_METRIC` | `count` ranks allocations by number instead of bytes. |
| `HOTPATH_METRICS_SERVER_OFF` | `1` disables the local metrics server (port 6770). The workloads set it. |

### Live view

```sh
cargo install hotpath --features=tui
hotpath console                                     # first terminal
cargo run -p glaux --profile profiling --features hotpath   # second terminal
```

### CPU samples

```sh
cargo install samply --locked
cargo install hotpath --bin hotpath-samply --version '^0.30'
samply setup                  # macOS: signs samply so it can attach
```

On Linux, follow the kernel settings in the [CPU profiling page](https://hotpath.rs/cpu_profiling). Run the test binary directly. Under nextest the sampler is interrupted and the report says `cpu - report unavailable`.

```sh
GLAUX_WORKLOAD=editor HOTPATH_METRICS_SERVER_OFF=1 \
  cargo test --profile profiling -p glaux --features hotpath-alloc,hotpath-cpu \
  -- --ignored --nocapture profiling_workload
```

Only the app crate defines the `hotpath-cpu` feature. Instrumented functions in the other crates were still attributed (`luau::tick`, `sense::complete`).

## What is instrumented

Labels are `area::name` and appear as such in the report.

| Label | Where | Notes |
|---|---|---|
| `app::frame`, `app::present`, `app::advance` | `apps/glaux/src/app.rs` | Once per displayed frame. |
| `system::update`, `system::render` | `System` | One call per tick and per frame. |
| `workspace::render`, `code_editor::render` | workspace | Drawing of the editor and its tabs. |
| `editing::syntax_refresh` | `DocumentSyntax` | Reparse and span update, called more than once per frame. |
| `syntax::highlighter_update`, `syntax::highlighter_spans` | `glaux-syntax` | Tree-sitter incremental parse and highlight query. |
| `sense::complete`, `sense::signature_help`, `sense::hover`, `sense::definition` | `glaux-sense` | Run on editor requests, and completion also while typing. |
| `running::advance`, `running::render` | `RunningCartridge` | Cartridge tick and frame replay. |
| `luau::tick`, `rhai::tick` | script runtimes | One script `tick()` plus `draw()`. |
| `cartridge::load`, `cartridge_fs::replace_file` | cartridge storage | Cold paths: opening and saving. |
| `workload::frame` | the workload test only | `render` plus `update` plus audio, one application frame without the window. |

Nothing below the tick level is instrumented (draw commands, pixels, glyphs, per-token work). Those run thousands of times per frame and the instrumentation would cost more than it measures.

## Reading a report

Times nest, so never add them up.

- `workload::frame` contains `system::update` and `system::render`. `system::update` contains `running::advance`, which contains `luau::tick` or `rhai::tick`. `system::render` contains `workspace::render`, which contains `code_editor::render`, which contains `editing::syntax_refresh` and so on. `% Total` is relative to the outermost function.
- In a window, `app::frame` contains `app::present`, which contains `system::render` and the host's `present`.
- With `hotpath-alloc`, bytes are exclusive by default: a function does not include the instrumented functions it calls. Set `HOTPATH_ALLOC_CUMULATIVE=1` for inclusive numbers.
- A script runtime's allocations are the cartridge's own: the VM allocates through Rust's allocator, so `luau::tick` and `rhai::tick` count the garbage a script makes.

### Wall time, CPU time and waiting

- **Wall time** is what `Avg` and `P95` show. It includes any time the thread spent blocked.
- **CPU time** is the `cpu_percent` of a thread in the `threads` section, or the samples of `hotpath-cpu`. A thread at 9 % that takes 13 ms of wall time per frame used about 1.2 ms of CPU and waited for the rest.
- **Waiting** shows up as wall time with little CPU. In the application, `app::present` is almost all waiting: `host.present` blocks until the display refreshes. It is not rendering work, so never compare it with the budget. The work of drawing is `system::render`.
- The frame budget is 1000 / 60 = **16.67 ms** for the work of one tick plus one draw. The presented frame rate follows the host's refresh rate, and only ticks run at 60 Hz, so a window can present faster than 60 frames per second without the budget changing.
- A function with the expected `Calls` and a growing `Avg` is slowing down. One with an unexpected `Calls` is being called too often.

## Baseline

Taken on 2026-10-28 on top of the commit that adds the workloads. The medians of three runs with hotpath off, in milliseconds per application frame (`render` plus `update` plus audio, no vsync, no window).

| | |
|---|---|
| Machine | Apple M5 Max, macOS 27.0.1, other applications open |
| Toolchain | rustc 1.99.1, `profiling` profile, hotpath 0.30.0, samply 0.13.1 |
| Glaux | the Luau and Rhai ports of `examples/signal-lost`, 3600 ticks |

| Scenario | Frames | p50 | p95 | p99 | max | Budget used at p95 |
|---|---|---|---|---|---|---|
| Signal Lost, Luau | 3600 | 0.078 | 0.097 | 0.113 | 0.210 | 0.6 % |
| Signal Lost, Rhai | 3600 | 0.284 | 0.351 | 0.433 | 0.684 | 2.1 % |

Where the time goes (average per tick, microseconds, from `hotpath`):

| | Luau | Rhai |
|---|---|---|
| `luau::tick` / `rhai::tick` (script `tick` and `draw`) | 52 | 249 |
| `running::render` (replaying the draw commands) | 21 | 20 |
| Allocated per tick by the script | 74.9 KB | 307.8 KB |
| Peak resident memory of the test process | 24 MB | 24 MB |

The editor scenario, per phase (milliseconds per frame, medians of three runs with hotpath off):

| Phase | Frames | p50 | p95 | max |
|---|---|---|---|---|
| editor idle | 480 | 0.078 | 0.088 | 0.141 |
| scroll | 24 | 0.102 | 0.125 | 0.136 |
| navigate | 300 | 0.082 | 0.132 | 0.183 |
| typing (with completion popups) | 627 | 0.216 | 0.323 | 0.851 |
| complete | 36 | 0.148 | 0.278 | 0.295 |
| signature | 12 | 0.041 | 0.048 | 0.059 |
| hover | 1440 | 0.038 | 0.046 | 0.100 |
| definition and back | 241 | 0.080 | 0.355 | 3.402 |
| open file | 9 | 0.963 | 1.517 | 1.517 |
| tabs | 74 | 0.078 | 0.123 | 0.217 |
| undo | 40 | 0.051 | 0.057 | 0.063 |

Editor functions (`hotpath`, average per call): `syntax::highlighter_update` 113 µs (664 calls, only when the text changed), `syntax::highlighter_spans` 18 µs, `sense::complete` 37 µs (470 calls), `sense::definition` 262 µs (60 calls), `sense::signature_help` 28 µs, `sense::hover` 33 µs. Allocations per call: `sense::complete` 44.6 KB, `sense::definition` 134.8 KB, `syntax::highlighter_spans` 20.8 KB. Peak resident memory of the test process: 31 MB.

In a window, with the shell idle for 16 seconds after the boot animation (20 s run, `profiling` profile): 1545 frames, so the display took one about every 12.95 ms. `app::frame` averaged 12.78 ms (p95 17.86 ms), of which `system::render` was 23 µs on average and `app::advance` 14 µs. The main thread used 9.1 % of one core, about 1.2 ms of CPU per frame. The rest of each frame is the wait inside `host.present`. Ticks ran at 59.2 per second. Resident memory was 118 MB, most of it raylib and the graphics driver.

CPU samples (`hotpath-cpu`, exclusive): Luau, `luau::tick` 54 % and `running::render` 26 %. Rhai, `rhai::tick` 90 % and `running::render` 6 %.

### What the baseline says

- Every frame of every scenario stays far below 16.67 ms. The slowest one measured is 3.4 ms, a go to definition that opened another module. Nothing is a bottleneck on this machine.
- Rhai takes about five times the time and four times the memory of Luau for the same game, still at 2 % of the budget.
- The allocation figures are the place to look for regressions. They are identical between runs, unlike times.
- `sense::complete` and `sense::definition` allocate tens of kilobytes per call. That is cheap at the sizes measured here (the largest script has a few hundred lines). Nobody has measured how they scale with the size of a script.

### Reproducing

Allocations are exact between runs, and the times of three runs of `mise run profile` differed by less than 12 % at p95 and by about 1 % at p50 with other applications open. Treat a change as real when it is clearly beyond that, and repeat it. Compare machines with care: the numbers are for the machine above, and a slower one has less headroom.

## Costs of profiling

- Timing: the same workloads with the features off, with `hotpath` and with `hotpath-alloc` give the same p50 and p95 within the spread between runs (for example Luau p50 0.078, 0.076, 0.077 ms). The instrumentation is not measurable at this granularity.
- Normal builds: `target/release/glaux` is 96 bytes larger than before hotpath was added (13,488,256 against 13,488,160). `cargo tree -p glaux` adds `hotpath` and `hotpath-macros` and no other crate. The only "hotpath" text in the binary is the third-party license data, which lists the two crates.
- The metrics server (`localhost:6770`) starts with any hotpath feature in a windowed run. Set `HOTPATH_METRICS_SERVER_OFF=1` when you do not need `hotpath console`.

## Limitations

- The workloads do not include the host: no window, no presentation, no vsync, no GPU, no audio device. `app::present` was measured only in the windowed idle run, and there is no windowed measurement of a running cartridge or of editing.
- `hotpath-cpu` does not attribute the samples of `system::render`, `workspace::render` and `code_editor::render`. They are credited to the caller (`workload::frame`, 48 % of the samples in the editor scenario). Methods defined in an `impl` block in a different module from their type probably do not match the symbol hotpath expects. That is not verified, and fixing it is open.
- The editor scenario uses Signal Lost, scripts of up to a few hundred lines. Larger scripts are not measured.
- Linux and Windows are not measured.
- The workload drives the system with scripted input and the ROM's real font. It does not reproduce the timing of a person typing.

hotpath-cloud can compare pull requests in CI. It is not configured here.
