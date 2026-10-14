//! The documented API against what the runtime registers.
//!
//! This checks that names exist, that every documented arity is registered
//! and no other, and that nothing is registered without being documented.
//! It cannot check parameter names or types, because the functions take
//! untyped values. See [`glaux_docs::catalog`].
//!
//! Rhai cannot list the members of a constant module, so a documented member
//! is checked to exist but an extra undocumented one would go unnoticed.

use std::collections::{BTreeMap, BTreeSet};

use glaux_docs::{Kind, Library, ScriptLanguage};
use pretty_assertions::assert_eq;
use rhai::Dynamic;

use super::*;

const LANGUAGE: ScriptLanguage = ScriptLanguage::Rhai;

/// Functions the type registrations add to `Button` and `Key`, which are not
/// part of the script API.
const INTERNAL: [&str; 2] = ["to_string", "to_debug"];

/// The arities registered under each qualified name.
fn registered() -> BTreeMap<String, BTreeSet<usize>> {
    let runtime = started("");
    let mut registered: BTreeMap<String, BTreeSet<usize>> = BTreeMap::new();
    for signature in runtime.engine().gen_fn_signatures(false) {
        let Some((name, rest)) = signature.split_once('(') else {
            continue;
        };
        let is_function = name.chars().next().is_some_and(|c| c.is_ascii_alphabetic());
        if !is_function || INTERNAL.contains(&name) {
            continue;
        }
        registered
            .entry(name.to_owned())
            .or_default()
            .insert(arity(rest));
    }
    registered
}

/// The number of parameters in `rest`, which starts just after the opening
/// parenthesis of a signature.
fn arity(rest: &str) -> usize {
    let mut depth = 0usize;
    let mut parameters = 0usize;
    let mut seen_any = false;
    for c in rest.chars() {
        match c {
            '(' | '<' => depth += 1,
            ')' if depth == 0 => break,
            ')' | '>' => depth -= 1,
            ',' if depth == 0 => parameters += 1,
            c if !c.is_whitespace() => seen_any = true,
            _ => {}
        }
    }
    if seen_any { parameters + 1 } else { 0 }
}

#[test]
fn every_documented_function_is_registered_with_exactly_the_documented_arities() {
    let registered = registered();
    let library = Library::embedded();
    for entry in library.api().entries_for(LANGUAGE) {
        if entry.kind() != Kind::Function {
            continue;
        }
        let name = LANGUAGE.qualified_name(entry.binding(LANGUAGE).unwrap());
        let expected: BTreeSet<usize> = (entry.required_params()..=entry.params().len()).collect();
        assert_eq!(
            registered.get(&name),
            Some(&expected),
            "{} ({name}): registered arities against documented ones",
            entry.id()
        );
    }
}

#[test]
fn every_documented_namespace_has_functions() {
    let registered = registered();
    let library = Library::embedded();
    for entry in library.api().entries_for(LANGUAGE) {
        if entry.kind() != Kind::Namespace {
            continue;
        }
        let name = &entry.binding(LANGUAGE).unwrap().name;
        let prefix = format!("{name}::");
        assert!(
            registered.keys().any(|key| key.starts_with(&prefix)),
            "{}: nothing is registered under {prefix}",
            entry.id()
        );
    }
}

#[test]
fn documented_constants_exist() {
    let runtime = started("");
    let library = Library::embedded();
    let mut checked = 0;
    for entry in library.api().entries_for(LANGUAGE) {
        if entry.kind() != Kind::ConstantGroup {
            continue;
        }
        let members: Vec<&str> = match entry.source() {
            Some("glaux_input::Button") => Button::ALL.iter().map(|b| b.name()).collect(),
            Some("glaux_input::Key") => Key::ALL.iter().map(|k| k.name()).collect(),
            other => panic!("{}: unknown constant source {other:?}", entry.id()),
        };
        let group = &entry.binding(LANGUAGE).unwrap().name;
        for member in members {
            let expression = format!("{group}::{member}");
            let _constant: Dynamic = runtime
                .engine()
                .eval_expression(&expression)
                .unwrap_or_else(|error| panic!("{expression}: {error}"));
        }
        checked += 1;
    }
    assert_eq!(checked, 2, "Button and Key are the constant groups");
}

#[test]
fn nothing_is_registered_without_being_documented() {
    let library = Library::embedded();
    let documented: BTreeSet<String> = library
        .api()
        .entries_for(LANGUAGE)
        .filter(|entry| entry.kind() == Kind::Function)
        .map(|entry| LANGUAGE.qualified_name(entry.binding(LANGUAGE).unwrap()))
        .collect();
    let undocumented: Vec<String> = registered()
        .into_keys()
        .filter(|name| !documented.contains(name))
        .collect();
    assert_eq!(undocumented, Vec::<String>::new());
}

#[test]
fn arity_counting_handles_nested_types_and_no_parameters() {
    assert_eq!(arity(") -> i64"), 0);
    assert_eq!(arity("_) -> bool"), 1);
    assert_eq!(arity("_: A, _: B<C, D>) -> E"), 2);
    assert_eq!(arity("_: (), _: &mut X, _: Y) -> Z"), 3);
    assert_eq!(arity("_, _, _, _)"), 4);
}
