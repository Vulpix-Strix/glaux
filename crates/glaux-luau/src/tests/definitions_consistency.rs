//! The generated definitions against what the runtime installs.

use std::collections::BTreeSet;

use glaux_audio::Waveform;
use glaux_docs::{Kind, Library, ScriptLanguage};
use mlua::{Table, Value};
use pretty_assertions::assert_eq;

use super::*;

fn generated() -> String {
    crate::definitions(Library::embedded().api()).unwrap()
}

/// The lines between `declare <name>: {` and the closing brace.
fn members_of(text: &str, name: &str) -> BTreeSet<String> {
    let start = format!("declare {name}: {{");
    text.lines()
        .skip_while(|line| *line != start)
        .skip(1)
        .take_while(|line| *line != "}")
        .map(|line| line.trim().split(':').next().unwrap().to_owned())
        .collect()
}

fn table_names(table: &Table) -> BTreeSet<String> {
    table
        .pairs::<String, Value>()
        .map(|pair| pair.unwrap().0)
        .collect()
}

#[test]
fn button_and_key_members_are_the_ones_the_runtime_installs() {
    let runtime = started("");
    let text = generated();
    for group in ["Button", "Key"] {
        let table: Table = runtime.lua().globals().raw_get(group).unwrap();
        assert_eq!(members_of(&text, group), table_names(&table), "{group}");
    }
}

#[test]
fn waveform_names_are_the_ones_the_runtime_accepts() {
    let text = generated();
    let line = text
        .lines()
        .find(|l| l.starts_with("type Waveform"))
        .unwrap();
    let declared: BTreeSet<&str> = line
        .split('=')
        .nth(1)
        .unwrap()
        .split('|')
        .map(|name| name.trim().trim_matches('"'))
        .collect();
    let accepted: BTreeSet<&str> = Waveform::ALL.iter().map(|w| w.name()).collect();
    assert_eq!(declared, accepted);
    for name in declared {
        assert!(glaux_script::validate::waveform(name).is_ok(), "{name}");
    }
}

#[test]
fn every_luau_function_is_declared_where_the_runtime_installs_it() {
    let runtime = started("");
    let library = Library::embedded();
    let text = generated();
    for entry in library.api().entries_for(ScriptLanguage::Luau) {
        let binding = entry.binding(ScriptLanguage::Luau).unwrap();
        match entry.kind() {
            Kind::Function if binding.path.is_empty() => {
                let declaration = format!("declare function {}(", binding.name);
                let analyzer_owned =
                    crate::definitions::ANALYZER_GLOBALS.contains(&binding.name.as_str());
                assert_eq!(
                    text.contains(&declaration),
                    !analyzer_owned,
                    "{} is declared or missing against the analyzer globals",
                    entry.id()
                );
            }
            Kind::Function => {
                let namespace = &binding.path[0];
                assert!(
                    members_of(&text, namespace).contains(&binding.name),
                    "{} is not declared",
                    entry.id()
                );
            }
            Kind::Namespace => {
                let table: Table = runtime.lua().globals().raw_get(&*binding.name).unwrap();
                assert_eq!(
                    members_of(&text, &binding.name),
                    table_names(&table),
                    "{}",
                    entry.id()
                );
            }
            Kind::ConstantGroup | Kind::Keyword | Kind::Callback => {}
        }
    }
}
