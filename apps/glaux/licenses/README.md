# Third-party licenses

Glaux embeds the licenses of the software it ships, so credits need no files at run time.
`glaux licenses` prints everything that is embedded.

| File               | Content                                                               | Edited by              |
|--------------------|-----------------------------------------------------------------------|------------------------|
| `third-party.json` | Rust crates in the dependency graph of the `glaux` binary             | `mise run licenses`    |
| `extra.json`       | What Cargo cannot see: vendored native code and bundled art           | Hand                   |

Both files share one shape. `texts` stores each distinct text once and every entry lists the
indexes of its texts in `text_ids`. An entry has a `category`: `library` for crates, `native`
for vendored or compiled native code, and `art` for fonts, icons and palettes.

Each text has a `kind`, a `title`, its original `content` and, when it can be identified, an
`spdx` identifier:

- `license`: a license text as the component distributes it.
- `template`: a license text that still has unresolved placeholders such as `<year>`.
- `notice`: credits or a note that is not itself a license.

An entry's `license` is the SPDX expression the component declares, with its `OR`, `AND` and
`WITH` operators intact. The texts do not always cover every part of it.

## Updating

Run `mise run licenses` after any change to dependencies or `Cargo.lock`, and commit the result.
Never edit `third-party.json` by hand.

`mise run licenses-check` regenerates the data in memory and fails when `third-party.json` is out
of date, when `extra.json` is malformed, or when it repeats a crate that cargo-about already
reports. It is part of `mise run verify`. Neither task runs during a normal build.

The inventory covers Linux, macOS and Windows at once (see `targets` in `about.toml`). It is the
set of dependencies of all supported platforms, not necessarily those of one executable.

Dev and build dependencies are excluded because they are not distributed. The code that build
dependencies compile into the binary is not: raylib's C sources and the vendored tree-sitter
parser are listed in `extra.json`.

## Licenses

`accepted` in `about.toml` is the list of licenses we have reviewed. When generation fails on a
new license, read its obligations before adding it. Do not extend the list just to make the
command pass.

## Editing `extra.json`

Edit it only when a component Cargo cannot detect is added, updated or removed: native libraries
bundled by `-sys` crates, vendored sources, fonts, icons, palettes. Copy the license text from
the component's own source. Do not add anything cargo-about already reports.

## Limitations

- cargo-about picks one license per crate. For `MIT OR Apache-2.0` it supplies only the first
  alternative in `accepted`, usually Apache-2.0, so the other texts are absent. A viewer can
  find which alternatives have a text by comparing `license` with the `spdx` of the entry's texts.
- `spdx` is the identifier cargo-about assigns to the text (it identifies it with askalono), not
  what the crate declares. The one known misidentification is handled in `scripts/licenses.py`:
  a file that includes the LLVM exception is reported as plain Apache-2.0, and is labelled
  `Apache-2.0 WITH LLVM-exception` here. A text with no identification has no `spdx`.
- When a crate ships no license file, cargo-about falls back to its generic SPDX text. Such a text
  may lack the author's own copyright notice. Only the ones with unresolved placeholders are
  marked `template`, so a generic text without placeholders still shows up as `license`.
- cargo-about also reads the license headers of source files, so `raylib-sys` carries many copies
  of the raylib and GLFW notices. Distinct notices are kept as they are.
- `extra.json` entries whose text holds two alternatives (miniaudio, dr_libs, stb) keep the whole
  block and have no `spdx`.

## Where it is shown

The `credits` command in the shell browses this inventory in its Libraries and
Licenses pages. The editorial credits, such as who made Glaux, are a separate
hand-written file, `rom/credits.md` explains it.
