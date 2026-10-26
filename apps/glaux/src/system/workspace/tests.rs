use super::tab::text_width;
use super::*;
use crate::cartridge::source::SourcePath;
use crate::cartridge::{Language, LoadedCartridge};
use crate::system::test_font::LAYOUT_FONT;
use glaux_input::{InputEvent, Point, PointerButton};
use glaux_machine::framebuffer::WIDTH;
use pretty_assertions::assert_eq;

mod cursor;
mod docs_button;

fn layout() -> Layout {
    Layout::new(FontMetrics::SYSTEM)
}

#[test]
fn tabs_cycle_both_ways_and_wrap() {
    let mut tab = Tab::Code;
    let mut seen = vec![tab];
    for _ in 0..Tab::ALL.len() {
        tab = tab.next();
        seen.push(tab);
    }
    assert_eq!(
        seen,
        [
            Tab::Code,
            Tab::Sprites,
            Tab::Map,
            Tab::Sfx,
            Tab::Music,
            Tab::Code
        ]
    );
    assert_eq!(Tab::Code.previous(), Tab::Music);
    for tab in Tab::ALL {
        assert_eq!(tab.next().previous(), tab);
    }
}

#[test]
fn tabs_sit_in_the_bar_left_to_right_without_overlapping() {
    let tabs = layout().tabs();
    for pair in tabs.windows(2) {
        let ((_, a), (_, b)) = (pair[0], pair[1]);
        assert!(a.x + a.width < b.x, "{a:?} {b:?}");
    }
    for (tab, rect) in tabs {
        assert_eq!((rect.y, rect.height), (0, layout().bar_height()), "{tab:?}");
        let centre = Point::new((rect.x + rect.width / 2) as u16, 5);
        assert_eq!(layout().tab_at(centre), Some(tab));
    }
}

#[test]
fn gaps_and_the_view_below_the_bar_hit_no_tab() {
    let tabs = layout().tabs();
    let (_, first) = tabs[0];
    let gap = Point::new((first.x + first.width) as u16, 2);
    assert_eq!(layout().tab_at(gap), None);
    let below = Point::new((first.x + 2) as u16, layout().bar_height() as u16);
    assert_eq!(layout().tab_at(below), None);
    assert_eq!(layout().tab_at(Point::new(318, 2)), None);
}

#[test]
fn every_hint_fits_between_the_tabs_and_the_name() {
    let layout = layout();
    let labels = tests_support::english_labels();
    let [.., (_, last)] = layout.tabs();
    assert!(layout.hint_x() > last.right());
    let name_start = layout.name_right(&labels, &LAYOUT_FONT)
        - layout.name_cells(&labels, &LAYOUT_FONT) as i32 * layout.cell_w;
    for tab in Tab::ALL {
        let end = layout.hint_x() + text_width(&LAYOUT_FONT, labels.get(tab));
        assert!(end + NAME_GAP <= name_start, "{tab:?}");
    }
}

#[test]
fn long_names_are_cut_to_fit_and_tiny_space_hides_them() {
    assert_eq!(fit_name(&LAYOUT_FONT, "Pong", 10).as_deref(), Some("Pong"));
    assert_eq!(
        fit_name(&LAYOUT_FONT, "Glaux Owl", 9).as_deref(),
        Some("Glaux Owl")
    );
    assert_eq!(
        fit_name(&LAYOUT_FONT, "Glaux Owl Adventure", 9).as_deref(),
        Some("Glaux O..")
    );
    assert_eq!(fit_name(&LAYOUT_FONT, "Glaux Owl Adventure", 5), None);
    assert_eq!(
        fit_name(&LAYOUT_FONT, "ÁÉÍÓÚáéíóú", 8).as_deref(),
        Some("ÁÉÍÓÚá..")
    );
    // Double-width characters take two of the cells.
    assert_eq!(
        fit_name(&LAYOUT_FONT, "✅✅✅✅✅", 8).as_deref(),
        Some("✅✅✅..")
    );
    assert_eq!(
        fit_name(&LAYOUT_FONT, "a✅✅✅✅", 7).as_deref(),
        Some("a✅✅..")
    );
}

#[test]
fn text_is_clipped_and_wrapped_by_cells() {
    assert_eq!(take_cells(&LAYOUT_FONT, "a✅b", 2), "a");
    assert_eq!(take_cells(&LAYOUT_FONT, "a✅b", 3), "a✅");
    assert_eq!(take_cells(&LAYOUT_FONT, "a✅b", 9), "a✅b");
    assert_eq!(
        wrap(&LAYOUT_FONT, "ok ✅✅ done", 5),
        ["ok", "✅✅", "done"]
    );
    // A word wider than the row is cut on character boundaries, keeping at
    // least one character per row.
    assert_eq!(wrap(&LAYOUT_FONT, "✅✅✅", 3), ["✅", "✅", "✅"]);
    assert_eq!(wrap(&LAYOUT_FONT, "✅", 1), ["✅"]);
}
