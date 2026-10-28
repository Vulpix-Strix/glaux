# Profiling Glaux

Glaux uses [hotpath](https://hotpath.rs) for development profiling. Every hotpath macro is a no-op unless a Cargo feature is on, so normal builds contain no profiling code and compile none of hotpath's dependencies. hotpath does not touch `tracing`: the log file, the log panel and `RUST_LOG` behave as usual.

## Running the profiler

```sh
# Wall-clock time per instrumented function
cargo run -p glaux --features hotpath

# Time plus allocations (installs a counting global allocator)
cargo run -p glaux --features hotpath-alloc
```

The report is printed when Glaux exits normally. For a fixed-length run, set a timeout and write the report to a file:

```sh
HOTPATH_SHUTDOWN_MS=10000 HOTPATH_OUTPUT_PATH=report.txt \
  cargo run -p glaux --features hotpath-alloc
```

Useful variables (all of them are listed in the [hotpath configuration page](https://hotpath.rs/configuration)):

| Variable | Effect |
|---|---|
| `HOTPATH_SHUTDOWN_MS` | Quit and print the report after this many milliseconds. |
| `HOTPATH_OUTPUT_PATH` | Write the report to a file instead of stdout. |
| `HOTPATH_OUTPUT_FORMAT` | `table` (default), `json`, `json-pretty` or `none`. |
| `HOTPATH_FOCUS` | Only report functions whose name contains this text, or matches `/regex/`. |
| `HOTPATH_ALLOC_METRIC` | `count` ranks allocations by number instead of bytes. |
| `HOTPATH_METRICS_SERVER_OFF` | `1` disables the local metrics server (port 6770). |

### Live view

```sh
cargo install hotpath --features=tui
hotpath console          # first terminal
cargo run -p glaux --features hotpath   # second terminal
```

### CPU samples

`hotpath-cpu` adds a sampling profile using [samply](https://github.com/mstange/samply). Setup (macOS `samply setup`, Linux kernel settings and `setsid -w`) is described in the [CPU profiling page](https://hotpath.rs/cpu_profiling). Build with symbols through the `profiling` profile:

```sh
cargo run -p glaux --profile profiling --features hotpath-alloc,hotpath-cpu
```

Only the app crate forwards `hotpath-cpu`, so samples are attributed to the instrumented functions that live in `apps/glaux`.

## What is instrumented

Labels are `area::name` and appear as such in the report.

| Label | Where | Notes |
|---|---|---|
| `app::frame`, `app::present`, `app::advance` | `apps/glaux/src/app.rs` | Once per displayed frame. |
| `system::update`, `system::render` | `System` | One call per tick and per frame. |
| `workspace::render`, `code_editor::render` | workspace | Drawing of the editor and its tabs. |
| `editing::syntax_refresh` | `DocumentSyntax` | Reparse and span update, called while rendering code. |
| `syntax::highlighter_update`, `syntax::highlighter_spans` | `glaux-syntax` | Tree-sitter incremental parse and highlight query. |
| `sense::complete`, `sense::signature_help`, `sense::hover`, `sense::definition` | `glaux-sense` | Only run on editor requests. |
| `running::advance`, `running::render` | `RunningCartridge` | Cartridge tick and frame replay. |
| `luau::tick`, `rhai::tick` | script runtimes | One script `tick()` plus `draw()`. |
| `cartridge::load`, `cartridge_fs::replace_file` | cartridge storage | Cold paths: opening and saving. |

Nothing below the tick level is instrumented (draw commands, pixels, glyphs, per-token work). Those run thousands of times per frame and the instrumentation would cost more than it measures. Use `hotpath-cpu` or a sampling profiler to look inside them.

`app::present` includes the host's `present` call, which blocks until the display refreshes. Most of its time is waiting for vsync, not work. Compare it with `system::render` to see the actual drawing cost.

## Reading a report

- `Calls` shows how often a function ran. A function with the expected count and a growing `Avg` is slowing down, one with an unexpected count is being called too often.
- A frame at 60 Hz has a budget of about 16.6 ms. Look at `P95`, not only `Avg`.
- With `hotpath-alloc`, the allocation table is exclusive by default (nested instrumented calls are not counted). Set `HOTPATH_ALLOC_CUMULATIVE=1` for inclusive numbers.

## Detecting regressions

Capture a JSON report from the same workload before and after a change and compare the functions that moved:

```sh
HOTPATH_SHUTDOWN_MS=10000 HOTPATH_OUTPUT_FORMAT=json-pretty \
  HOTPATH_OUTPUT_PATH=before.json \
  cargo run -p glaux --features hotpath-alloc
```

Keep the machine idle, use the same cartridge and the same actions, and compare `P95` and allocation bytes rather than single runs. Timing varies between runs, so treat only clear differences as regressions.

hotpath-cloud can compare pull requests automatically in CI. It is not configured here.
