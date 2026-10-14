//! The documentation is embedded from the repository's `docs/` directory.
//! Cargo does not watch a directory that a derive only lists, so name it
//! here to rebuild when a page is added or removed.

fn main() {
    println!("cargo:rerun-if-changed=../../docs");
}
