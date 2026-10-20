use glaux_input::InputEvent::{Backspace, Break, Down, Enter, Function, Left, Right, Text};
use pretty_assertions::assert_eq;

use super::*;

fn catalog() -> Catalog {
    let entry = |name: &str, category: &str, license: &str| {
        serde_json::json!({
            "name": name, "version": "1.0.0", "license": license,
            "repository": null, "category": category, "text_ids": [],
        })
    };
    let licenses = serde_json::from_value(serde_json::json!({
        "texts": [],
        "entries": [
            entry("serde", "library", "MIT OR Apache-2.0"),
            entry("Anyhow", "library", "MIT"),
            entry("raylib", "library", "Zlib"),
            entry("Luau", "native", "MIT"),
            entry("Palette", "art", ""),
        ],
    }))
    .expect("the fixture matches the inventory format");
    Catalog::new(licenses)
}

fn press(page: &mut Libraries, catalog: &Catalog, events: &[InputEvent]) -> Vec<Reaction> {
    events
        .iter()
        .map(|&event| page.handle_input(event, catalog))
        .collect()
}

fn names<'a>(page: &Libraries, catalog: &'a Catalog) -> Vec<&'a str> {
    page.shown()
        .iter()
        .map(|&at| catalog.entries()[at].name.as_str())
        .collect()
}

fn down() -> InputEvent {
    Down { select: false }
}

#[test]
fn entries_are_sorted_by_name_ignoring_case() {
    let catalog = catalog();
    let page = Libraries::new(&catalog);
    assert_eq!(
        names(&page, &catalog),
        ["Anyhow", "Luau", "Palette", "raylib", "serde"]
    );
}

#[test]
fn s_sorts_by_license_with_undeclared_ones_last() {
    let catalog = catalog();
    let mut page = Libraries::new(&catalog);
    press(&mut page, &catalog, &[Text('s')]);
    assert_eq!(
        names(&page, &catalog),
        ["Anyhow", "Luau", "serde", "raylib", "Palette"]
    );
}

#[test]
fn c_cycles_through_the_categories() {
    let catalog = catalog();
    let mut page = Libraries::new(&catalog);
    let mut seen = Vec::new();
    for _ in 0..4 {
        press(&mut page, &catalog, &[Text('c')]);
        seen.push(page.shown().len());
    }
    assert_eq!(seen, [3, 1, 1, 5]);
}

#[test]
fn a_search_filters_as_it_is_typed_and_enter_keeps_it() {
    let catalog = catalog();
    let mut page = Libraries::new(&catalog);
    press(&mut page, &catalog, &[Text('/'), Text('A'), Text('y')]);
    assert!(page.is_typing());
    assert_eq!(names(&page, &catalog), ["raylib"]);
    press(&mut page, &catalog, &[Backspace]);
    assert_eq!(
        names(&page, &catalog),
        ["Anyhow", "Luau", "Palette", "raylib"]
    );
    press(&mut page, &catalog, &[Enter]);
    assert!(!page.is_typing());
    assert_eq!(
        names(&page, &catalog),
        ["Anyhow", "Luau", "Palette", "raylib"]
    );
}

#[test]
fn escape_while_typing_goes_back_to_the_kept_search() {
    let catalog = catalog();
    let mut page = Libraries::new(&catalog);
    press(&mut page, &catalog, &[Text('/'), Text('r'), Enter]);
    let kept = names(&page, &catalog).len();
    press(&mut page, &catalog, &[Text('/'), Text('z'), Break]);
    assert!(!page.is_typing());
    assert_eq!(names(&page, &catalog).len(), kept);
}

#[test]
fn typed_keys_are_text_while_searching() {
    let catalog = catalog();
    let mut page = Libraries::new(&catalog);
    press(&mut page, &catalog, &[Text('/'), Text('c'), Text('s')]);
    assert!(page.is_typing());
    assert_eq!(page.category, None);
    assert_eq!(page.sort, Sort::Name);
}

#[test]
fn the_selection_stays_on_the_same_entry_when_the_filter_changes() {
    let catalog = catalog();
    let mut page = Libraries::new(&catalog);
    press(&mut page, &catalog, &[down(), down(), down()]);
    assert_eq!(names(&page, &catalog)[page.selected()], "raylib");
    press(&mut page, &catalog, &[Text('/'), Text('r')]);
    assert_eq!(names(&page, &catalog)[page.selected()], "raylib");
}

#[test]
fn the_card_browses_neighbors_and_escape_closes_it() {
    let catalog = catalog();
    let mut page = Libraries::new(&catalog);
    press(&mut page, &catalog, &[Enter]);
    assert!(matches!(page.level, Level::Card));
    press(
        &mut page,
        &catalog,
        &[Right { select: false }, Right { select: false }],
    );
    assert_eq!(page.selected(), 2);
    press(&mut page, &catalog, &[Left { select: false }]);
    assert_eq!(page.selected(), 1);
    press(&mut page, &catalog, &[Break]);
    assert!(matches!(page.level, Level::List));
}

#[test]
fn an_empty_list_has_nothing_to_open() {
    let catalog = catalog();
    let mut page = Libraries::new(&catalog);
    press(
        &mut page,
        &catalog,
        &[Text('/'), Text('q'), Text('q'), Enter, Enter],
    );
    assert!(page.shown().is_empty());
    assert!(matches!(page.level, Level::List));
}

#[test]
fn the_page_passes_on_what_it_does_not_use() {
    let catalog = catalog();
    let mut page = Libraries::new(&catalog);
    assert_eq!(
        press(
            &mut page,
            &catalog,
            &[Left { select: false }, Function(2), Break]
        ),
        [Reaction::Pass; 3]
    );
}

#[test]
fn showing_a_license_drops_the_other_filters() {
    let catalog = catalog();
    let mut page = Libraries::new(&catalog);
    press(
        &mut page,
        &catalog,
        &[Text('c'), Text('/'), Text('z'), Enter],
    );
    page.show_license(LicenseFilter::Id("MIT".to_owned()), &catalog);
    assert_eq!(names(&page, &catalog), ["Anyhow", "Luau", "serde"]);
    assert_eq!(page.selected(), 0);
}
