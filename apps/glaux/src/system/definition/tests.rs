use pretty_assertions::assert_eq;

use super::*;
use crate::cartridge_fs::fixture::Fixture;
use crate::system::editing::history::EditKind;
use crate::system::editing::session::tests::{cartridge, path};

const MAIN: &str = "local m = require(\"util\")\nm.clamp(1)\nm.hidden(2)\nm.nope(3)\n";
const UTIL: &str =
    "local M = {}\nfunction M.clamp(x) return x end\nlocal function hidden() end\nreturn M\n";

fn offset_of(text: &str, needle: &str) -> usize {
    text.find(needle).expect("needle in text")
}

fn setup(fixture: &Fixture, files: &[(&str, &str)]) -> (LoadedCartridge, CodeSession, DocumentId) {
    let bytes: Vec<(&str, &[u8])> = files.iter().map(|(p, t)| (*p, t.as_bytes())).collect();
    let cart = cartridge(fixture, &bytes);
    let mut code = CodeSession::default();
    let id = code.open(&cart, &path(files[0].0)).unwrap();
    (cart, code, id)
}

fn found(definition: Definition) -> (String, usize) {
    match definition {
        Definition::Found(target) => match target.at {
            Spot::Offset(at) => (target.path.as_str().to_owned(), at),
            Spot::Line(_) => panic!("a definition is an offset"),
        },
        other => panic!("expected a definition, got {other:?}"),
    }
}

#[test]
fn a_local_leads_to_its_declaration_in_the_same_file() {
    let fixture = Fixture::new("def-local");
    let text = "local speed = 1\nprint(speed)\n";
    let (cart, mut code, id) = setup(&fixture, &[("code/main.luau", text)]);
    let caret = text.rfind("speed").unwrap();
    assert_eq!(
        found(locate(&mut code, &cart, id, caret)),
        ("code/main.luau".to_owned(), 6)
    );
}

#[test]
fn a_member_of_a_required_module_leads_to_the_export_in_that_file() {
    let fixture = Fixture::new("def-member");
    let (cart, mut code, id) = setup(
        &fixture,
        &[("code/main.luau", MAIN), ("code/util.luau", UTIL)],
    );
    let caret = offset_of(MAIN, "clamp") + 1;
    assert_eq!(
        found(locate(&mut code, &cart, id, caret)),
        ("code/util.luau".to_owned(), offset_of(UTIL, "clamp"))
    );
}

#[test]
fn a_member_the_module_does_not_export_leads_nowhere() {
    let fixture = Fixture::new("def-not-exported");
    let (cart, mut code, id) = setup(
        &fixture,
        &[("code/main.luau", MAIN), ("code/util.luau", UTIL)],
    );
    // `hidden` is a local function of the module that `return M` never exposes.
    for member in ["hidden", "nope"] {
        let caret = offset_of(MAIN, member) + 1;
        assert_eq!(locate(&mut code, &cart, id, caret), Definition::Unresolved);
    }
}

#[test]
fn a_module_whose_exports_cannot_be_proven_leads_nowhere() {
    let fixture = Fixture::new("def-unknown-exports");
    let util = "return build()\n";
    let (cart, mut code, id) = setup(
        &fixture,
        &[("code/main.luau", MAIN), ("code/util.luau", util)],
    );
    let caret = offset_of(MAIN, "clamp") + 1;
    assert_eq!(locate(&mut code, &cart, id, caret), Definition::Unresolved);
}

#[test]
fn unsaved_edits_of_the_module_decide_where_the_member_is() {
    let fixture = Fixture::new("def-unsaved");
    let (cart, mut code, id) = setup(
        &fixture,
        &[("code/main.luau", MAIN), ("code/util.luau", UTIL)],
    );
    let util = code.open(&cart, &path("code/util.luau")).unwrap();
    code.edit(util, 0..0, "-- header\n", EditKind::Typing, None)
        .unwrap()
        .unwrap();
    let caret = offset_of(MAIN, "clamp") + 1;
    // The file on disk still has `clamp` at its old offset.
    assert_eq!(
        found(locate(&mut code, &cart, id, caret)),
        (
            "code/util.luau".to_owned(),
            offset_of(UTIL, "clamp") + "-- header\n".len()
        )
    );
}

#[test]
fn the_name_of_the_module_leads_to_its_file() {
    let fixture = Fixture::new("def-module-name");
    let (cart, mut code, id) = setup(
        &fixture,
        &[("code/main.luau", MAIN), ("code/util.luau", UTIL)],
    );
    let caret = offset_of(MAIN, "util") + 1;
    assert_eq!(
        found(locate(&mut code, &cart, id, caret)),
        ("code/util.luau".to_owned(), 0)
    );
}

#[test]
fn a_missing_module_is_reported_with_its_name() {
    let fixture = Fixture::new("def-missing-module");
    let (cart, mut code, id) = setup(&fixture, &[("code/main.luau", MAIN)]);
    let caret = offset_of(MAIN, "util") + 1;
    assert_eq!(
        locate(&mut code, &cart, id, caret),
        Definition::ModuleMissing("util".to_owned())
    );
}

#[test]
fn rhai_follows_public_functions_and_refuses_private_and_overloaded_ones() {
    let fixture = Fixture::new("def-rhai");
    let main = "import \"lib\" as lib;\nlib::open();\nlib::secret();\nlib::twice(1);\n";
    let lib = "fn open() {}\nprivate fn secret() {}\nfn twice(a) {}\nfn twice(a, b) {}\n";
    let (cart, mut code, id) = setup(
        &fixture,
        &[("code/main.rhai", main), ("code/lib.rhai", lib)],
    );
    let at = |word: &str| offset_of(main, word) + 1;
    assert_eq!(
        found(locate(&mut code, &cart, id, at("open"))),
        ("code/lib.rhai".to_owned(), offset_of(lib, "open"))
    );
    assert_eq!(
        locate(&mut code, &cart, id, at("secret")),
        Definition::Unresolved
    );
    assert_eq!(
        locate(&mut code, &cart, id, at("twice")),
        Definition::Ambiguous
    );
}
