use std::fs;
use std::path::Path;

/// A v1 manifest for a cartridge named `name` whose entry is `entry`.
pub(crate) fn manifest_ron(name: &str, entry: &str) -> String {
    format!(
        r#"(
    format: 1,
    cartridge: (name: "{name}", version: "0.1.0", author: None),
    runtime: (language: Luau, entry: "{entry}"),
    hardware: (model: G1, graphics: G1_4bpp),
    compatibility: (min_glaux: "0.1.0"),
)"#
    )
}

/// Writes a cartridge directory at `dir` named `name`, version `0.1.0`,
/// with an empty entry at `code/main.luau`.
pub(crate) fn write_cartridge(dir: &Path, name: &str) {
    write_manifest(dir, &manifest_ron(name, "code/main.luau"));
    fs::create_dir_all(dir.join("code")).unwrap();
    fs::write(dir.join("code/main.luau"), "").unwrap();
}

/// A valid v1 Rhai manifest for a cartridge named `name` with entry `entry`.
pub(crate) fn rhai_manifest_ron(name: &str, entry: &str) -> String {
    manifest_ron(name, entry).replace("language: Luau", "language: Rhai")
}

/// Writes a Rhai cartridge directory at `dir` named `name`, version
/// `0.1.0`, with `script` as its entry at `code/main.rhai`.
pub(crate) fn write_rhai_cartridge(dir: &Path, name: &str, script: &str) {
    write_manifest(dir, &rhai_manifest_ron(name, "code/main.rhai"));
    fs::create_dir_all(dir.join("code")).unwrap();
    fs::write(dir.join("code/main.rhai"), script).unwrap();
}

/// Writes `cartridge.ron` with `ron` into `dir`, creating `dir` if needed.
pub(crate) fn write_manifest(dir: &Path, ron: &str) {
    fs::create_dir_all(dir).unwrap();
    fs::write(dir.join("cartridge.ron"), ron).unwrap();
}
