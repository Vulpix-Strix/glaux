use glaux_docs::{Library, ScriptLanguage};
use pretty_assertions::assert_eq;

use super::*;

fn view_of(subject: &Subject<'_>) -> Option<(HoverView, Option<DocTarget>)> {
    let summary = |_: &DocId| Some("A summary.".to_owned());
    let meaning = |_: &DocId, param: &str| (param == "x").then(|| "Horizontal.".to_owned());
    let line_of = |byte: usize| byte + 1;
    view_for(
        subject,
        &ViewSource {
            language: ScriptLanguage::Luau,
            summary: &summary,
            meaning: &meaning,
            line_of: &line_of,
        },
    )
}

fn signature(view: &HoverView) -> String {
    view.signature
        .iter()
        .map(|(text, _)| text.as_str())
        .collect()
}

#[test]
fn a_function_lists_its_parameters_and_what_the_page_says_of_them() {
    let library = Library::embedded();
    let entry = library
        .api()
        .lookup(ScriptLanguage::Luau, &[], "spr")
        .expect("spr is in the catalog");
    let (view, target) = view_of(&Subject::Entry(entry)).expect("a view");

    assert!(signature(&view).starts_with("spr("));
    assert_eq!(view.params.len(), entry.params().len());
    let x = view.params.iter().find(|row| row.name == "x").unwrap();
    assert_eq!((x.ty, x.meaning.as_str()), ("number", "Horizontal."));
    assert_eq!(view.summary.as_deref(), Some("A summary."));
    assert_eq!(view.tag.as_deref(), Some(entry.category()));
    assert!(view.opens_page);
    assert_eq!(target, Some(DocTarget::page(entry.id().clone())));
}

#[test]
fn optional_parameters_are_marked_and_a_missing_meaning_is_empty() {
    let library = Library::embedded();
    let (entry, optional) = library
        .api()
        .entries_for(ScriptLanguage::Luau)
        .filter(|entry| entry.kind() == glaux_docs::Kind::Function)
        .find_map(|entry| {
            let optional = entry.params().iter().find(|param| param.optional)?;
            Some((entry, optional))
        })
        .expect("some function has an optional parameter");
    let (view, _) = view_of(&Subject::Entry(entry)).expect("a view");
    let row = view
        .params
        .iter()
        .find(|row| row.name == format!("{}?", optional.name))
        .expect("the optional row");
    assert_eq!(row.meaning, "");
}

#[test]
fn only_functions_have_a_return_type() {
    let library = Library::embedded();
    let btnp = library
        .api()
        .lookup(ScriptLanguage::Luau, &[], "btnp")
        .expect("btnp is in the catalog");
    let (view, _) = view_of(&Subject::Entry(btnp)).expect("a view");
    assert_eq!(view.returns, Some("bool"));
    let audio = library
        .api()
        .lookup(ScriptLanguage::Luau, &[], "audio")
        .expect("audio is in the catalog");
    let (view, _) = view_of(&Subject::Entry(audio)).expect("a view");
    assert_eq!(view.returns, None);
    assert!(view.params.is_empty());
}

#[test]
fn a_constant_member_is_named_under_its_group_and_opens_the_group_page() {
    let library = Library::embedded();
    let group = library
        .api()
        .lookup(ScriptLanguage::Luau, &[], "Key")
        .expect("Key is in the catalog");
    let (view, target) = view_of(&Subject::Constant { group, member: "A" }).expect("a view");
    assert_eq!(signature(&view), "Key.A");
    assert!(view.params.is_empty() && view.returns.is_none());
    assert_eq!(target, Some(DocTarget::page(group.id().clone())));
}

#[test]
fn a_local_says_where_it_is_declared_and_has_no_page() {
    let definition = glaux_syntax::Definition {
        name: "mine".to_owned(),
        kind: glaux_syntax::DefinitionKind::Variable,
        declared: 6..10,
        visible_from: 20,
        params: Vec::new(),
    };
    let (view, target) = view_of(&Subject::Local(definition)).expect("a view");
    assert_eq!(signature(&view), "mine");
    assert_eq!(
        view.summary.as_deref(),
        Some("Local variable declared on line 7.")
    );
    assert!(!view.opens_page);
    assert_eq!(target, None);
}

#[test]
fn names_being_declared_and_unknown_words_have_nothing_to_say() {
    assert_eq!(view_of(&Subject::Declared("x".to_owned())), None);
    assert_eq!(view_of(&Subject::Unknown), None);
}
