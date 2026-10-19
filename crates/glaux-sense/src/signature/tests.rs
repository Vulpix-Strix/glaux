use ScriptLanguage::{Luau, Rhai};
use glaux_docs::{DocId, Library};
use glaux_syntax::Grammar;
use pretty_assertions::assert_eq;
use proptest::prelude::*;

use super::*;

const CARET: char = '¦';

fn grammar_of(language: ScriptLanguage) -> Grammar {
    grammar(language)
}

/// The help for the caret marked in `marked`.
fn help(language: ScriptLanguage, marked: &str) -> Option<SignatureHelp> {
    let caret = marked.find(CARET).expect("the text marks the caret");
    let text = marked.replacen(CARET, "", 1);
    let mut syntax = Highlighter::new(grammar_of(language)).expect("bundled query compiles");
    syntax.update(&text);
    let request = SignatureRequest {
        language,
        text: &text,
        caret,
        revision: 7,
    };
    signature_help(&request, Library::embedded().api(), &syntax)
}

fn names(signature: &Signature) -> Vec<&str> {
    signature.params.iter().map(|p| p.name.as_str()).collect()
}

#[test]
fn an_api_function_reports_its_parameters_and_active_one() {
    let found = help(Luau, "spr(1, ¦").expect("help");
    let signature = found.signature();
    assert_eq!(signature.name, "spr");
    assert_eq!(
        names(signature),
        ["tile", "x", "y", "w", "h", "flip_x", "flip_y", "palette"]
    );
    assert_eq!(found.active_param, Some(1));
    assert_eq!(found.revision, 7);
    assert_eq!(found.signatures.len(), 1);
    assert_eq!(
        signature.doc,
        Doc::Api(DocId::parse("api/graphics/spr").unwrap())
    );
}

#[test]
fn optional_parameters_and_types_come_from_the_catalog() {
    let found = help(Rhai, "spr(1, 2, 3, ¦").expect("help");
    let params = &found.signature().params;
    assert_eq!(
        params.iter().map(|p| p.optional).collect::<Vec<_>>(),
        [false, false, false, true, true, true, true, true]
    );
    assert_eq!(params[0].ty, Some(ValueType::Int));
    assert_eq!(params[1].ty, Some(ValueType::Number));
    assert_eq!(found.active_param, Some(3));
}

#[test]
fn the_return_type_comes_from_the_catalog_and_depends_on_the_language() {
    assert_eq!(
        help(Luau, "btnp(¦").expect("help").signature().returns,
        Some(ValueType::Bool)
    );
    assert_eq!(help(Luau, "spr(¦").expect("help").signature().returns, None);
    // `frame` returns a number in Luau and an integer in Rhai.
    assert_eq!(
        help(Luau, "frame(¦").expect("help").signature().returns,
        Some(ValueType::Number)
    );
    assert_eq!(
        help(Rhai, "frame(¦").expect("help").signature().returns,
        Some(ValueType::Int)
    );
}

#[test]
fn namespaced_functions_are_spelled_as_the_language_writes_them() {
    let luau = help(Luau, "audio.tone(1, ¦").expect("help");
    assert_eq!(luau.signature().name, "audio.tone");
    assert_eq!(luau.active_param, Some(1));
    let rhai = help(Rhai, "audio::tone(1, 2, ¦").expect("help");
    assert_eq!(rhai.signature().name, "audio::tone");
    assert_eq!(rhai.active_param, Some(2));
}

#[test]
fn functions_with_a_different_name_per_language_resolve_in_their_own() {
    assert_eq!(
        help(Luau, "print(¦").expect("help").signature().name,
        "print"
    );
    assert_eq!(help(Rhai, "text(¦").expect("help").signature().name, "text");
    assert_eq!(help(Luau, "text(¦"), None);
    assert_eq!(help(Rhai, "print(¦"), None);
}

#[test]
fn more_arguments_than_parameters_highlight_none() {
    let found = help(Luau, "btnp(1, 2, ¦").expect("help");
    assert_eq!(found.active_param, None);
    assert_eq!(found.signature().params.len(), 1);
}

#[test]
fn nested_calls_describe_the_innermost() {
    let found = help(Luau, "spr(btnp(¦").expect("help");
    assert_eq!(found.signature().name, "btnp");
    let found = help(Luau, "spr(btnp(1), ¦").expect("help");
    assert_eq!(found.signature().name, "spr");
    assert_eq!(found.active_param, Some(1));
}

#[test]
fn open_points_at_the_parenthesis() {
    let marked = "local a = 1\nspr(1, ¦";
    let found = help(Luau, marked).expect("help");
    assert_eq!(found.open, marked.find('(').unwrap());
}

#[test]
fn a_script_function_lists_its_parameter_names() {
    let found = help(Luau, "local function f(a, b)\nend\nf(1, ¦").expect("help");
    let signature = found.signature();
    assert_eq!(signature.name, "f");
    assert_eq!(names(signature), ["a", "b"]);
    assert!(
        signature
            .params
            .iter()
            .all(|p| p.ty.is_none() && !p.optional)
    );
    assert_eq!(found.active_param, Some(1));
    assert!(matches!(signature.doc, Doc::Local(ref d) if d.name == "f"));

    let found = help(Rhai, "fn f(a, b) {}\nf(1, ¦").expect("help");
    assert_eq!(names(found.signature()), ["a", "b"]);
    assert_eq!(found.active_param, Some(1));
}

#[test]
fn a_script_function_hides_the_api_function_of_the_same_name() {
    let found = help(Luau, "local function spr(a)\nend\nspr(¦").expect("help");
    assert_eq!(names(found.signature()), ["a"]);
    assert!(matches!(found.signature().doc, Doc::Local(_)));
}

#[test]
fn varargs_take_every_remaining_argument() {
    let found = help(Luau, "local function log(fmt, ...)\nend\nlog(1, 2, 3, ¦").expect("help");
    let params = &found.signature().params;
    assert!(!params[0].variadic && params[1].variadic);
    assert_eq!(found.active_param, Some(1));
}

#[test]
fn rhai_overloads_pick_the_signature_that_fits_the_arguments() {
    let source = "fn f(a) {}\nfn f(a, b, c) {}\nfn f(a, b) {}\n";
    let at = |call: &str| help(Rhai, &format!("{source}{call}")).expect("help");

    let found = at("f(¦");
    assert_eq!(found.signatures.len(), 3);
    assert_eq!(found.active_signature, 0);
    assert_eq!(found.active_param, Some(0));

    let found = at("f(1, ¦");
    assert_eq!(names(found.signature()), ["a", "b"]);
    assert_eq!(found.active_param, Some(1));

    let found = at("f(1, 2, ¦");
    assert_eq!(names(found.signature()), ["a", "b", "c"]);
    assert_eq!(found.active_param, Some(2));

    let found = at("f(1, 2, 3, ¦");
    assert_eq!(names(found.signature()), ["a", "b", "c"]);
    assert_eq!(found.active_param, None);
}

#[test]
fn luau_functions_with_one_name_are_not_overloads() {
    let found = help(Luau, "function f(a) end\nfunction f(a, b) end\nf(¦").expect("help");
    assert_eq!(found.signatures.len(), 1);
}

#[test]
fn nothing_is_described_for_what_is_not_a_known_function() {
    for (language, marked) in [
        (Luau, "obj:method(1, ¦"),
        (Luau, "unknown(1, ¦"),
        (Rhai, "unknown(1, ¦"),
        (Luau, "audio.nope(¦"),
        (Luau, "local audio = {}\naudio.tone(¦"),
        (Luau, "local f = 1\nf(¦"),
        (Luau, "function f(a, ¦"),
        (Rhai, "fn f(a, ¦"),
        (Luau, "if (a ¦"),
        (Luau, "tick(¦"),
        (Luau, "spr(1) ¦"),
    ] {
        assert_eq!(help(language, marked), None, "{language:?}: {marked:?}");
    }
}

#[test]
fn a_string_argument_still_gets_help_but_a_comment_does_not() {
    assert!(help(Luau, "spr(\"ab¦").is_some());
    assert!(help(Rhai, "spr(1, \"ab¦").is_some());
    assert_eq!(help(Luau, "-- spr(1, ¦"), None);
    assert_eq!(help(Luau, "--[[ spr(1, ¦"), None);
    assert_eq!(help(Rhai, "// spr(1, ¦"), None);
    assert_eq!(help(Rhai, "/* spr(1, ¦"), None);
}

#[test]
fn a_caret_after_a_closed_block_comment_still_gets_help() {
    assert!(help(Luau, "spr(--[[ ( ]] 1, ¦").is_some());
    assert!(help(Rhai, "spr(/* ( */ 1, ¦").is_some());
}

#[test]
fn text_the_tree_has_not_parsed_or_of_another_language_gets_none() {
    let library = Library::embedded();
    let text = "spr(1, ";
    let mut syntax = Highlighter::new(Grammar::Luau).unwrap();
    syntax.update(text);
    let request = |text, language| SignatureRequest {
        language,
        text,
        caret: text.len(),
        revision: 0,
    };
    assert!(signature_help(&request(text, Luau), library.api(), &syntax).is_some());
    assert_eq!(
        signature_help(&request("spr(1, 2, ", Luau), library.api(), &syntax),
        None
    );
    assert_eq!(
        signature_help(&request(text, Rhai), library.api(), &syntax),
        None
    );
}

#[test]
fn a_caret_inside_a_character_is_ignored() {
    let text = "spr(\"é";
    let mut syntax = Highlighter::new(Grammar::Luau).unwrap();
    syntax.update(text);
    let request = SignatureRequest {
        language: Luau,
        text,
        caret: text.len() - 1,
        revision: 0,
    };
    assert_eq!(
        signature_help(&request, Library::embedded().api(), &syntax),
        None
    );
}

#[test]
fn every_function_in_the_catalog_has_help_with_its_own_parameters() {
    let library = Library::embedded();
    for language in ScriptLanguage::ALL {
        for entry in library.api().entries_for(language) {
            if entry.kind() != Kind::Function {
                continue;
            }
            let binding = entry.binding(language).expect("entries_for filters");
            let name = language.qualified_name(binding);
            let found = help(language, &format!("{name}(¦")).expect("every function has help");
            let signature = found.signature();
            assert_eq!(signature.name, name);
            assert_eq!(signature.params.len(), entry.params().len(), "{name}");
            assert_eq!(
                found.active_param,
                (!entry.params().is_empty()).then_some(0)
            );
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn any_text_and_caret_give_consistent_help_without_panicking(
        text in "[a-zA-Z0-9_ .:,\n\t\"'`{}()\\[\\]#|/*é-]{0,80}",
        caret in 0usize..96,
        luau in any::<bool>(),
    ) {
        let language = if luau { Luau } else { Rhai };
        let mut caret = caret.min(text.len());
        while !text.is_char_boundary(caret) {
            caret -= 1;
        }
        let mut syntax = Highlighter::new(grammar_of(language)).expect("bundled query compiles");
        syntax.update(&text);
        let request = SignatureRequest { language, text: &text, caret, revision: 3 };
        if let Some(found) = signature_help(&request, Library::embedded().api(), &syntax) {
            prop_assert_eq!(found.revision, 3);
            prop_assert!(found.open < caret);
            prop_assert!(found.active_signature < found.signatures.len());
            if let Some(active) = found.active_param {
                prop_assert!(active < found.signature().params.len());
            }
        }
    }
}
