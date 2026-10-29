use super::*;
use crate::system::test_font::LAYOUT_FONT;
use pretty_assertions::assert_eq;

fn layout() -> CodeLayout {
    CodeLayout::new(
        crate::system::workspace::tests_support::CODE_METRICS,
        LAYOUT_FONT.clone(),
    )
}

fn labels(count: usize) -> Vec<String> {
    (0..count)
        .map(|index| format!("file{index:02}.luau"))
        .collect()
}

fn paths(paths: &[&str]) -> Vec<SourcePath> {
    paths
        .iter()
        .map(|path| SourcePath::parse(path).unwrap())
        .collect()
}

#[test]
fn the_rows_stack_from_the_bar_to_the_bottom_without_gaps() {
    let layout = layout();
    assert_eq!(layout.strip.y, 16);
    assert_eq!(layout.options.y, layout.strip.bottom());
    assert_eq!(layout.text.y, layout.options.bottom());
    assert_eq!(layout.diag.y, layout.text.bottom());
    assert_eq!(layout.status.y, layout.diag.bottom());
    assert_eq!(layout.status.bottom(), HEIGHT as i32);
}

#[test]
fn the_problem_list_takes_one_row_and_three_while_stopped() {
    let free = layout();
    let stopped = free.resized_diag(DIAG_ROWS_STOPPED);
    assert_eq!((free.diag_rows(), stopped.diag_rows()), (1, 3));
    assert_eq!(stopped.diag_row(2).bottom(), stopped.status.y);
    assert_eq!(
        free.text.height - stopped.text.height,
        2 * free.metrics.cell_h
    );
}

#[test]
fn tabs_that_fit_all_show_left_to_right_inside_the_strip() {
    let layout = layout();
    let labels = labels(3);
    let tabs = layout.strip_tabs(&labels, 0);
    assert_eq!(
        tabs.iter().map(|tab| tab.index).collect::<Vec<_>>(),
        [0, 1, 2]
    );
    for pair in tabs.windows(2) {
        assert!(pair[0].rect.right() < pair[1].rect.x);
    }
    for tab in &tabs {
        assert!(tab.rect.x >= 0 && tab.rect.bottom() <= layout.strip.bottom());
        assert!(tab.rect.contains_xy(tab.close.x, tab.close.y));
        assert!(
            tab.rect
                .contains_xy(tab.close.right() - 1, tab.close.bottom() - 1)
        );
    }
}

#[test]
fn overflowing_tabs_stop_before_the_more_button() {
    let layout = layout();
    let labels = labels(20);
    let more = layout.more_button();
    assert!(more.right() <= WIDTH as i32);
    for scroll in 0..labels.len() {
        let tabs = layout.strip_tabs(&labels, scroll);
        assert!(!tabs.is_empty(), "scroll {scroll}");
        assert_eq!(tabs[0].index, scroll);
        for tab in &tabs {
            assert!(!tab.rect.overlaps(more), "scroll {scroll}: {tab:?}");
        }
    }
    assert!(layout.strip_tabs(&labels, 0).len() < labels.len());
}

#[test]
fn revealing_scrolls_just_far_enough_to_show_the_active_tab() {
    let layout = layout();
    let labels = labels(20);
    let shown = layout.strip_tabs(&labels, 0).len();

    // Already visible: unchanged.
    assert_eq!(layout.reveal(&labels, 0, shown - 1), 0);
    // Right of the strip: scrolled until it is the last one shown.
    let scroll = layout.reveal(&labels, 0, shown);
    let tabs = layout.strip_tabs(&labels, scroll);
    assert_eq!(tabs.last().unwrap().index, shown);
    assert_eq!(scroll, 1);
    // Left of the strip: becomes the first one shown.
    assert_eq!(layout.reveal(&labels, 10, 4), 4);
    // The last tab can always be shown.
    let last = labels.len() - 1;
    let scroll = layout.reveal(&labels, 0, last);
    assert!(
        layout
            .strip_tabs(&labels, scroll)
            .iter()
            .any(|tab| tab.index == last)
    );
}

#[test]
fn hits_tell_tabs_close_glyphs_and_the_more_button_apart() {
    let layout = layout();
    let labels = labels(3);
    let tabs = layout.strip_tabs(&labels, 0);
    let at = |x: i32, y: i32| Point::new(x as u16, y as u16);
    let tab = tabs[1];
    assert_eq!(
        layout.strip_hit(&labels, 0, at(tab.rect.x + 2, tab.rect.y + 2)),
        StripHit::Tab(1)
    );
    assert_eq!(
        layout.strip_hit(&labels, 0, at(tab.close.x + 2, tab.close.y + 2)),
        StripHit::Close(1)
    );
    let more = layout.more_button();
    assert_eq!(
        layout.strip_hit(&labels, 0, at(more.x + 1, more.y + 1)),
        StripHit::More
    );
    assert_eq!(
        layout.strip_hit(&labels, 0, at(tab.rect.x, layout.options.y)),
        StripHit::None
    );
}

#[test]
fn labels_are_file_names_unless_two_tabs_share_one() {
    let paths = paths(&[
        "code/main.luau",
        "code/enemies/util.luau",
        "code/ui/util.luau",
        "hud.rhai",
    ]);
    let refs: Vec<&SourcePath> = paths.iter().collect();
    assert_eq!(
        tab_labels(&LAYOUT_FONT, &refs),
        [
            "main.luau",
            "..mies/util.luau",
            "..e/ui/util.luau",
            "hud.rhai"
        ]
    );
}

#[test]
fn long_labels_keep_their_end() {
    let paths = paths(&["code/a_really_long_module_name.luau"]);
    let label = &tab_labels(&LAYOUT_FONT, &[&paths[0]])[0];
    assert_eq!(label.chars().count(), MAX_LABEL_CELLS);
    assert_eq!(label, "..dule_name.luau");
    assert_eq!(shorten_start(&LAYOUT_FONT, "abcdef", 2), "ef");
}
