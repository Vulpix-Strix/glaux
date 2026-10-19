use glaux_docs::Library;
use glaux_sense::{SignatureRequest, signature_help};
use glaux_syntax::{Grammar, Highlighter};
use pretty_assertions::assert_eq;

use super::*;

const CARET: char = '¦';

fn view_in(language: glaux_docs::ScriptLanguage, marked: &str) -> SignatureView {
    let caret = marked.find(CARET).unwrap();
    let text = marked.replacen(CARET, "", 1);
    let mut syntax = Highlighter::new(match language {
        glaux_docs::ScriptLanguage::Luau => Grammar::Luau,
        glaux_docs::ScriptLanguage::Rhai => Grammar::Rhai,
    })
    .unwrap();
    syntax.update(&text);
    let help = signature_help(
        &SignatureRequest {
            language,
            text: &text,
            caret,
            revision: 0,
        },
        Library::embedded().api(),
        &syntax,
    )
    .expect("a call");
    let meaning = |id: &DocId, param: &str| Some(format!("{param} of {id}."));
    let line_of = |byte: usize| byte / 10 + 1;
    view_for(
        &help,
        &ViewSource {
            meaning: &meaning,
            line_of: &line_of,
        },
    )
}

fn luau(marked: &str) -> SignatureView {
    view_in(glaux_docs::ScriptLanguage::Luau, marked)
}

fn rhai(marked: &str) -> SignatureView {
    view_in(glaux_docs::ScriptLanguage::Rhai, marked)
}

#[test]
fn an_api_function_lists_its_parameters_with_optional_ones_marked() {
    let view = luau("spr(1, ¦");
    assert_eq!(view.name, "spr");
    assert_eq!(
        view.params,
        [
            "tile", "x", "y", "w?", "h?", "flip_x?", "flip_y?", "palette?"
        ]
    );
    assert_eq!(view.active, Some(1));
    assert_eq!(view.returns, None);
    assert_eq!(view.overload, None);
}

#[test]
fn the_active_parameter_gets_its_type_and_what_the_page_says_about_it() {
    let view = luau("spr(1, 2, 3, ¦");
    assert_eq!(
        view.head,
        Some(Head {
            name: "w?".to_owned(),
            ty: Some("int")
        })
    );
    assert_eq!(view.note.as_deref(), Some("w of api/graphics/spr."));
}

#[test]
fn the_return_type_is_named_when_the_function_has_one() {
    assert_eq!(luau("btnp(¦").returns, Some("bool"));
}

#[test]
fn a_namespaced_function_is_written_in_the_language_asked() {
    assert_eq!(luau("audio.tone(¦").name, "audio.tone");
    assert_eq!(rhai("audio::tone(¦").name, "audio::tone");
}

#[test]
fn with_no_active_parameter_there_is_no_head_and_no_parameter_note() {
    let view = luau("btnp(1, 2, ¦");
    assert_eq!(view.active, None);
    assert_eq!(view.head, None);
    assert_eq!(view.note, None);
}

#[test]
fn a_script_function_says_where_it_is_declared() {
    let view = rhai("fn f(a, b) {}\nf(1, ¦");
    assert_eq!(view.params, ["a", "b"]);
    assert_eq!(view.head.as_ref().map(|h| h.name.as_str()), Some("b"));
    assert_eq!(view.head.as_ref().and_then(|h| h.ty), None);
    // Declared at byte 3, which the test counts as line 1.
    assert_eq!(view.note.as_deref(), Some("Declared on line 1."));
}

#[test]
fn overloads_say_which_one_is_shown() {
    let view = rhai("fn f(a) {}\nfn f(a, b) {}\nf(1, ¦");
    assert_eq!(view.params, ["a", "b"]);
    assert_eq!(view.overload, Some((2, 2)));
}
