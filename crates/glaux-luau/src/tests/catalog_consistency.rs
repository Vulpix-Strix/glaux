//! The documented API against what the runtime installs.
//!
//! This checks that names exist and that nothing is installed without being
//! documented. It cannot check parameter names, types or arity, because the
//! functions take untyped values. See [`glaux_docs::catalog`].

use std::collections::BTreeSet;

use glaux_docs::{Kind, Library, ScriptLanguage};
use mlua::{Lua, Table, Value};
use pretty_assertions::assert_eq;

use super::*;

const LANGUAGE: ScriptLanguage = ScriptLanguage::Luau;

fn globals_of_a_plain_vm() -> BTreeSet<String> {
    Lua::new()
        .globals()
        .pairs::<String, Value>()
        .map(|pair| pair.unwrap().0)
        .collect()
}

fn walk(lua: &Lua, path: &[String]) -> Option<Table> {
    let mut table = lua.globals();
    for segment in path {
        table = table.raw_get::<Option<Table>>(segment.as_str()).unwrap()?;
    }
    Some(table)
}

fn member_names(table: &Table) -> BTreeSet<String> {
    table
        .pairs::<String, Value>()
        .map(|pair| pair.unwrap().0)
        .collect()
}

#[test]
fn every_catalog_entry_exists_with_the_documented_kind() {
    let runtime = started("");
    let library = Library::embedded();
    for entry in library.api().entries_for(LANGUAGE) {
        if matches!(entry.kind(), Kind::Keyword | Kind::Callback) {
            continue;
        }
        let binding = entry.binding(LANGUAGE).unwrap();
        let parent = walk(runtime.lua(), &binding.path)
            .unwrap_or_else(|| panic!("{}: namespace {:?} is missing", entry.id(), binding.path));
        let value: Value = parent.raw_get(binding.name.as_str()).unwrap();
        let found = match value {
            Value::Function(_) => Kind::Function,
            Value::Table(_) => match entry.kind() {
                Kind::ConstantGroup => Kind::ConstantGroup,
                _ => Kind::Namespace,
            },
            other => panic!("{}: unexpected {}", entry.id(), other.type_name()),
        };
        assert_eq!(
            found,
            entry.kind(),
            "{} is documented as {:?}",
            entry.id(),
            entry.kind()
        );
    }
}

#[test]
fn constant_groups_hold_exactly_their_source_members() {
    let runtime = started("");
    let library = Library::embedded();
    for entry in library.api().entries_for(LANGUAGE) {
        if entry.kind() != Kind::ConstantGroup || entry.constants().is_some() {
            continue;
        }
        let expected: BTreeSet<String> = match entry.source() {
            Some("glaux_input::Button") => {
                Button::ALL.iter().map(|b| b.name().to_owned()).collect()
            }
            Some("glaux_input::Key") => Key::ALL.iter().map(|k| k.name().to_owned()).collect(),
            other => panic!("{}: unknown constant source {other:?}", entry.id()),
        };
        let name = &entry.binding(LANGUAGE).unwrap().name;
        let table = walk(runtime.lua(), std::slice::from_ref(name)).unwrap();
        assert_eq!(member_names(&table), expected, "{}", entry.id());
    }
}

/// The names of `table`'s integer-valued string keys, and its integer keys.
/// Any other key or value fails the test.
fn integer_constants(table: &Table, id: &str) -> (BTreeSet<String>, BTreeSet<i64>) {
    let (mut names, mut indices) = (BTreeSet::new(), BTreeSet::new());
    for pair in table.pairs::<Value, Value>() {
        match pair.unwrap() {
            (Value::String(name), Value::Integer(_)) => {
                names.insert(name.to_str().unwrap().to_owned());
            }
            (Value::Integer(index), Value::Table(_)) => {
                indices.insert(index);
            }
            (key, value) => panic!("{id}: unexpected {key:?} = {value:?}"),
        }
    }
    (names, indices)
}

fn listed(names: &[String]) -> BTreeSet<String> {
    names.iter().cloned().collect()
}

#[test]
fn listed_constant_groups_match_the_read_only_runtime_tables() {
    let runtime = started("");
    let library = Library::embedded();
    let mut checked = 0;
    for entry in library.api().entries_for(LANGUAGE) {
        let Some(constants) = entry.constants() else {
            continue;
        };
        let id = entry.id().to_string();
        let binding = entry.binding(LANGUAGE).unwrap();
        let path: Vec<String> = binding
            .path
            .iter()
            .chain([&binding.name])
            .cloned()
            .collect();
        let table = walk(runtime.lua(), &path).unwrap_or_else(|| panic!("{id} is missing"));
        assert!(table.is_readonly(), "{id} can be written");

        let (names, indices) = integer_constants(&table, &id);
        assert_eq!(names, listed(&constants.names), "{id}");
        let expected: BTreeSet<i64> = entry
            .elements()
            .map(|elements| (elements.first..=elements.last).collect())
            .unwrap_or_default();
        assert_eq!(indices, expected, "{id}: indexed tables");

        for index in indices {
            let element: Table = table.raw_get(index).unwrap();
            assert!(element.is_readonly(), "{id}[{index}] can be written");
            let (names, inner) = integer_constants(&element, &format!("{id}[{index}]"));
            let elements = entry.elements().unwrap();
            assert_eq!(names, listed(&elements.constants.names), "{id}[{index}]");
            assert!(inner.is_empty(), "{id}[{index}]");
        }
        checked += 1;
    }
    assert!(checked > 0, "no constant group lists its constants");
}

#[test]
fn nothing_is_installed_without_being_documented() {
    let runtime = started("");
    let library = Library::embedded();
    let api = library.api();
    let plain = globals_of_a_plain_vm();
    let globals = runtime.lua().globals();

    let mut undocumented = Vec::new();
    for pair in globals.pairs::<String, Value>() {
        let (name, value) = pair.unwrap();
        if plain.contains(&name) {
            continue;
        }
        let Some(entry) = api.lookup(LANGUAGE, &[], &name) else {
            undocumented.push(name);
            continue;
        };
        if entry.kind() != Kind::Namespace {
            continue;
        }
        let Value::Table(table) = value else { continue };
        for member in member_names(&table) {
            if api.lookup(LANGUAGE, &[name.as_str()], &member).is_none() {
                undocumented.push(format!("{name}.{member}"));
            }
        }
    }
    assert_eq!(undocumented, Vec::<String>::new());
}

#[test]
fn callbacks_are_documented_but_never_installed() {
    let runtime = started("");
    let library = Library::embedded();
    let documented: BTreeSet<&str> = library
        .api()
        .entries_for(LANGUAGE)
        .filter(|entry| entry.kind() == Kind::Callback)
        .map(|entry| entry.binding(LANGUAGE).unwrap().name.as_str())
        .collect();
    let called: BTreeSet<&str> = [Phase::Boot, Phase::Tick, Phase::Draw]
        .into_iter()
        .filter_map(Phase::callback)
        .collect();
    assert_eq!(documented, called);

    let globals = runtime.lua().globals();
    for name in called {
        let value: Value = globals.raw_get(name).unwrap();
        assert!(matches!(value, Value::Nil), "the runtime installs {name}");
    }
}
