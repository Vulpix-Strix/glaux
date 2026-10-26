use super::*;
use pretty_assertions::assert_eq;

fn id(raw: u64) -> DocumentId {
    DocumentId::from_raw(raw)
}

/// Tabs opened for documents `0..count`, in that order, the last active.
fn opened(count: u64) -> DocumentTabs {
    let mut tabs = DocumentTabs::default();
    for raw in 0..count {
        tabs.open(id(raw));
        // Each opens after the active one, so keep appending at the end.
        assert_eq!(tabs.index_of(id(raw)), Some(raw as usize));
    }
    tabs
}

fn order(tabs: &DocumentTabs) -> Vec<DocumentId> {
    tabs.ids().collect()
}

#[test]
fn opening_adds_a_tab_after_the_active_one_and_activates_it() {
    let mut tabs = opened(3);
    tabs.activate(id(0));
    tabs.open(id(9));
    assert_eq!(order(&tabs), [id(0), id(9), id(1), id(2)]);
    assert_eq!(tabs.active(), Some(id(9)));
}

#[test]
fn opening_a_document_again_only_activates_its_tab() {
    let mut tabs = opened(3);
    tabs.view_mut(id(0)).unwrap().top_line = 7;
    tabs.open(id(0));
    assert_eq!(order(&tabs), [id(0), id(1), id(2)]);
    assert_eq!(tabs.active(), Some(id(0)));
    assert_eq!(tabs.view(id(0)).unwrap().top_line, 7);
}

#[test]
fn activating_an_unknown_document_changes_nothing() {
    let mut tabs = opened(2);
    assert!(!tabs.activate(id(5)));
    assert_eq!(tabs.active(), Some(id(1)));
}

#[test]
fn closing_the_active_tab_activates_the_one_that_took_its_place() {
    let mut tabs = opened(3);
    tabs.activate(id(1));
    assert!(tabs.close(id(1)));
    assert_eq!(order(&tabs), [id(0), id(2)]);
    assert_eq!(tabs.active(), Some(id(2)));
}

#[test]
fn closing_the_last_active_tab_falls_back_to_the_one_before() {
    let mut tabs = opened(3);
    tabs.close(id(2));
    assert_eq!(tabs.active(), Some(id(1)));
    tabs.close(id(1));
    tabs.close(id(0));
    assert_eq!(tabs.active(), None);
    assert_eq!(tabs.len(), 0);
}

#[test]
fn closing_another_tab_keeps_the_active_one() {
    let mut tabs = opened(3);
    tabs.close(id(0));
    assert_eq!(tabs.active(), Some(id(2)));
    assert!(!tabs.close(id(0)), "a closed tab can't close again");
    assert_eq!(order(&tabs), [id(1), id(2)]);
}

#[test]
fn reordering_moves_one_tab_and_keeps_the_active_one() {
    let mut tabs = opened(4);
    tabs.activate(id(1));
    tabs.move_tab(0, 3);
    assert_eq!(order(&tabs), [id(1), id(2), id(3), id(0)]);
    tabs.move_tab(3, 1);
    assert_eq!(order(&tabs), [id(1), id(0), id(2), id(3)]);
    assert_eq!(tabs.active(), Some(id(1)));

    tabs.move_tab(0, 4);
    tabs.move_tab(9, 0);
    assert_eq!(order(&tabs), [id(1), id(0), id(2), id(3)]);
}

#[test]
fn each_view_follows_its_document_through_reorders_and_closes() {
    let mut tabs = opened(3);
    for raw in 0..3 {
        tabs.view_mut(id(raw)).unwrap().top_line = 10 * raw as usize;
    }
    tabs.move_tab(2, 0);
    tabs.close(id(1));
    assert_eq!(tabs.view(id(0)).unwrap().top_line, 0);
    assert_eq!(tabs.view(id(2)).unwrap().top_line, 20);
    assert_eq!(tabs.view(id(1)), None);

    // A document opened again starts from the top.
    tabs.open(id(1));
    assert_eq!(tabs.view(id(1)), Some(&DocumentView::default()));
}

#[test]
fn cycling_wraps_both_ways() {
    let mut tabs = opened(3);
    tabs.cycle(1);
    assert_eq!(tabs.active(), Some(id(0)));
    tabs.cycle(-1);
    assert_eq!(tabs.active(), Some(id(2)));
    tabs.cycle(-1);
    assert_eq!(tabs.active(), Some(id(1)));

    let mut empty = DocumentTabs::default();
    empty.cycle(1);
    assert_eq!(empty.active(), None);
}

#[test]
fn scroll_stays_on_an_existing_tab() {
    let mut tabs = opened(3);
    tabs.set_scroll(10);
    assert_eq!(tabs.scroll(), 2);
    tabs.close(id(2));
    assert_eq!(tabs.scroll(), 1);
}
