//! Compiles the vendored Rhai parser, which has no published Rust bindings.

fn main() {
    let src = "vendor/tree-sitter-rhai/src";
    cc::Build::new()
        .include(src)
        .file(format!("{src}/parser.c"))
        .std("c11")
        .warnings(false)
        .compile("tree-sitter-rhai");
    println!("cargo:rerun-if-changed={src}");
}
