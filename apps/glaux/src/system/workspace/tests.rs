use super::*;
use pretty_assertions::assert_eq;

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
    let [.., (_, last)] = layout.tabs();
    assert!(layout.hint_x() > last.right());
    let name_start = WIDTH as i32 - NAME_PAD - layout.name_cells() as i32 * layout.cell_w;
    for tab in Tab::ALL {
        let end = layout.hint_x() + text_width(tab.label(), layout.cell_w);
        assert!(end + NAME_GAP <= name_start, "{tab:?}");
    }
}

#[test]
fn long_names_are_cut_to_fit_and_tiny_space_hides_them() {
    assert_eq!(fit_name("Pong", 10).as_deref(), Some("Pong"));
    assert_eq!(fit_name("Glaux Owl", 9).as_deref(), Some("Glaux Owl"));
    assert_eq!(
        fit_name("Glaux Owl Adventure", 9).as_deref(),
        Some("Glaux O..")
    );
    assert_eq!(fit_name("Glaux Owl Adventure", 5), None);
    assert_eq!(fit_name("ÁÉÍÓÚáéíóú", 8).as_deref(), Some("ÁÉÍÓÚá.."));
}
