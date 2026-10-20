use glaux_input::InputEvent::{Break, Down, Enter, Function, Left, Right, Text};
use pretty_assertions::assert_eq;

use super::catalog::LicenseFilter;
use super::*;

fn down() -> InputEvent {
    Down { select: false }
}

fn left() -> InputEvent {
    Left { select: false }
}

fn right() -> InputEvent {
    Right { select: false }
}

fn press(view: &mut CreditsView, events: &[InputEvent]) -> bool {
    events.iter().any(|&event| view.handle_input(event))
}

#[test]
fn function_keys_go_to_their_page() {
    let mut view = CreditsView::open();
    assert_eq!(view.page(), Page::Credits);
    press(&mut view, &[Function(3)]);
    assert_eq!(view.page(), Page::Libraries);
    press(&mut view, &[Function(2)]);
    assert_eq!(view.page(), Page::System);
    press(&mut view, &[Function(4)]);
    assert_eq!(view.page(), Page::Licenses);
    press(&mut view, &[InputEvent::Docs]);
    assert_eq!(view.page(), Page::Credits);
}

#[test]
fn left_and_right_step_through_the_pages_and_stop_at_the_ends() {
    let mut view = CreditsView::open();
    press(&mut view, &[left()]);
    assert_eq!(view.page(), Page::Credits);
    press(&mut view, &[right(), right(), right(), right(), right()]);
    assert_eq!(view.page(), Page::Licenses);
}

#[test]
fn escape_leaves_from_a_main_page_only() {
    let mut view = CreditsView::open();
    press(&mut view, &[Function(3), Enter]);
    assert!(
        !press(&mut view, &[Break]),
        "the first Escape closes the card"
    );
    assert!(press(&mut view, &[Break]));
}

#[test]
fn escape_peels_off_the_search_then_the_license_filter_then_leaves() {
    let mut view = CreditsView::open();
    press(&mut view, &[Function(4), Enter]);
    assert_eq!(view.page(), Page::Libraries);
    let filtered = view.libraries.shown().len();
    assert!(filtered < view.catalog.entries().len());
    press(&mut view, &[Text('/'), Text('a'), Enter]);
    let searched = view.libraries.shown().len();
    assert!(searched <= filtered);

    assert!(!press(&mut view, &[Break]));
    assert_eq!(
        view.libraries.shown().len(),
        filtered,
        "the search went first"
    );
    assert!(!press(&mut view, &[Break]));
    assert_eq!(
        view.libraries.shown().len(),
        view.catalog.entries().len(),
        "then the license filter"
    );
    assert!(press(&mut view, &[Break]));
}

#[test]
fn a_license_chosen_on_the_licenses_page_filters_the_libraries() {
    let mut view = CreditsView::open();
    let first = view.catalog.summary()[0].id.clone();
    press(&mut view, &[Function(4), Enter]);
    assert_eq!(view.page(), Page::Libraries);
    let filter = LicenseFilter::Id(first);
    assert!(!view.libraries.shown().is_empty());
    for &at in view.libraries.shown() {
        assert!(view.catalog.matches(at, &filter));
    }
}

#[test]
fn each_page_keeps_its_place_while_the_view_stays_open() {
    let mut view = CreditsView::open();
    press(&mut view, &[Function(3), down(), down(), down()]);
    let selected = view.libraries.selected();
    assert_eq!(selected, 3);
    press(
        &mut view,
        &[Function(1), Function(2), Function(4), Function(3)],
    );
    assert_eq!(view.libraries.selected(), selected);
}

#[test]
fn the_roll_only_advances_on_its_own_page() {
    let mut view = CreditsView::open();
    press(&mut view, &[Function(2)]);
    for _ in 0..600 {
        view.tick();
    }
    assert!(view.roll.is_title_card(), "the title card waited");
}

#[test]
fn typing_a_search_does_not_trigger_page_keys() {
    let mut view = CreditsView::open();
    press(&mut view, &[Function(3), Text('/')]);
    assert!(!press(
        &mut view,
        &[Text('?'), Text('c'), Function(2), left(), Break]
    ));
    assert_eq!(view.page(), Page::Libraries);
    assert!(!view.keys_open);
}

#[test]
fn the_key_list_opens_with_a_question_mark_and_closes_with_escape() {
    let mut view = CreditsView::open();
    press(&mut view, &[Text('?')]);
    assert!(view.keys_open);
    assert!(!press(&mut view, &[Function(3), Break]));
    assert_eq!(
        view.page(),
        Page::Credits,
        "page keys wait while the list is open"
    );
    assert!(!view.keys_open);
}

#[test]
fn the_roll_lists_sections_with_items_and_credits_art_with_its_author() {
    let editorial = Editorial::strict(
        r#"(
            sections: [
                (title: "Empty"),
                (title: "Team", items: [Person(role: "Code", name: "Ada")]),
            ],
            authors: {"Zughy 32": "Zughy"},
        )"#,
    )
    .expect("valid credits");
    let catalog = Catalog::new(crate::licenses::third_party().expect("embedded licenses"));
    let lines = roll::lines(&editorial, &catalog);
    assert_eq!(lines[0], roll::RollLine::Heading("Team".to_owned()));
    assert_eq!(
        lines[1],
        roll::RollLine::Pair("Code".to_owned(), "Ada".to_owned())
    );
    assert!(!lines.contains(&roll::RollLine::Heading("Empty".to_owned())));
    assert!(lines.contains(&roll::RollLine::Pair(
        "Zughy 32".to_owned(),
        "Zughy".to_owned()
    )));
}

#[test]
fn every_embedded_entry_can_be_opened_and_read() {
    let catalog = Catalog::new(crate::licenses::third_party().expect("embedded licenses"));
    for at in 0..catalog.entries().len() {
        let reader = reader::Reader::open(&catalog, at, 0);
        assert!(reader.is_some(), "{}", catalog.entries()[at].name);
    }
}
