use pretty_assertions::assert_eq;

use super::super::view::Head;
use super::*;
use crate::system::test_font::LAYOUT_FONT;

const CELL_W: i32 = 6;
const CELL_H: i32 = 12;

fn area() -> Rect {
    Rect {
        x: 0,
        y: 28,
        width: 320,
        height: 200,
    }
}

fn place_in(area: Rect) -> Place<'static> {
    Place {
        font: &LAYOUT_FONT,
        area,
        cell_w: CELL_W,
        cell_h: CELL_H,
    }
}

fn view(params: &[&str], active: Option<usize>) -> SignatureView {
    SignatureView {
        name: "spr".to_owned(),
        params: params.iter().map(|p| (*p).to_owned()).collect(),
        returns: None,
        active,
        overload: None,
        head: None,
        note: None,
    }
}

fn laid_out(view: &SignatureView, anchor: (i32, i32)) -> SignatureLayout {
    SignatureLayout::new(place_in(area()), view, anchor, &[]).expect("a layout")
}

/// The text of the signature line.
fn line(layout: &SignatureLayout) -> String {
    layout.segments.iter().map(|s| s.text.as_str()).collect()
}

fn chips(layout: &SignatureLayout) -> Vec<&str> {
    layout
        .segments
        .iter()
        .filter(|s| s.chip)
        .map(|s| s.text.as_str())
        .collect()
}

fn with_detail(note: &str) -> SignatureView {
    SignatureView {
        head: Some(Head {
            name: "w?".to_owned(),
            ty: Some("int"),
        }),
        note: Some(note.to_owned()),
        ..view(&["tile", "w?"], Some(1))
    }
}

#[test]
fn the_popup_goes_above_the_line_and_below_when_there_is_no_room() {
    let view = view(&["a", "b"], Some(0));
    let above = laid_out(&view, (40, 100));
    assert!(above.window.bottom() + SHADOW <= 100);

    let below = laid_out(&view, (40, 30));
    assert_eq!(below.window.y, 30 + CELL_H);
}

#[test]
fn the_popup_and_its_shadow_stay_inside_the_area() {
    let view = view(&["tile", "x", "y"], Some(0));
    let layout = laid_out(&view, (318, 100));
    assert!(layout.window.right() + SHADOW <= area().right());
    assert!(layout.window.x >= area().x);

    let layout = laid_out(&view, (10, 227));
    assert!(layout.window.bottom() + SHADOW <= area().bottom());
}

#[test]
fn a_signature_that_fits_is_shown_whole_with_the_active_parameter_set_off() {
    let layout = laid_out(&view(&["tile", "x", "y"], Some(1)), (40, 100));
    assert_eq!(line(&layout), "spr(tile, x, y)");
    assert_eq!(chips(&layout), ["x"]);
}

#[test]
fn no_active_parameter_sets_nothing_off() {
    let layout = laid_out(&view(&["tile", "x"], None), (40, 100));
    assert_eq!(line(&layout), "spr(tile, x)");
    assert!(chips(&layout).is_empty());
}

#[test]
fn a_function_without_parameters_shows_empty_parentheses() {
    assert_eq!(line(&laid_out(&view(&[], None), (40, 100))), "spr()");
}

#[test]
fn the_return_type_and_overload_follow_the_parentheses() {
    let view = SignatureView {
        returns: Some("bool"),
        overload: Some((2, 3)),
        ..view(&["a"], Some(0))
    };
    assert_eq!(line(&laid_out(&view, (40, 100))), "spr(a) : bool 2/3");
}

#[test]
fn a_long_signature_drops_the_return_type_before_any_parameter() {
    let params: Vec<String> = (0..9).map(|n| format!("param{n}")).collect();
    let params: Vec<&str> = params.iter().map(String::as_str).collect();
    // "spr(param0, ..., param8)" is 4 + 9 * 6 + 8 * 2 + 1 = 75 columns, so
    // narrow the area to leave exactly enough for it without the type.
    let view = SignatureView {
        returns: Some("bool"),
        ..view(&params, Some(0))
    };
    let narrow = Rect {
        width: 75 * CELL_W + 2 + 2 * PAD_X + SHADOW,
        ..area()
    };
    let layout = SignatureLayout::new(place_in(narrow), &view, (0, 100), &[]).unwrap();
    assert!(!line(&layout).contains("bool"), "{}", line(&layout));
    assert!(line(&layout).starts_with("spr(param0, param1"));
}

#[test]
fn a_long_signature_keeps_the_active_parameter_and_elides_the_rest() {
    let params: Vec<String> = (0..12).map(|n| format!("param{n}")).collect();
    let params: Vec<&str> = params.iter().map(String::as_str).collect();
    let view = view(&params, Some(6));
    let layout = laid_out(&view, (40, 100));
    let shown = line(&layout);

    assert!(shown.starts_with("spr(.., "), "{shown}");
    assert!(shown.ends_with(", ..)"), "{shown}");
    assert_eq!(chips(&layout), ["param6"]);
    assert!(
        LAYOUT_FONT.text_cells(&shown) <= layout.columns,
        "{shown} is wider than {} columns",
        layout.columns
    );
}

#[test]
fn elision_only_marks_the_side_that_lost_parameters() {
    let params: Vec<String> = (0..12).map(|n| format!("param{n}")).collect();
    let params: Vec<&str> = params.iter().map(String::as_str).collect();

    let first = line(&laid_out(&view(&params, Some(0)), (40, 100)));
    assert!(first.starts_with("spr(param0"), "{first}");
    assert!(first.ends_with(", ..)"), "{first}");

    let last = line(&laid_out(&view(&params, Some(11)), (40, 100)));
    assert!(last.starts_with("spr(.., "), "{last}");
    assert!(last.ends_with("param11)"), "{last}");
}

#[test]
fn the_head_and_note_share_a_row_when_the_note_has_room() {
    let layout = laid_out(&with_detail("Width in tiles."), (40, 100));
    assert_eq!(
        layout
            .head
            .iter()
            .map(|s| s.text.as_str())
            .collect::<String>(),
        "w? int"
    );
    let note = &layout.notes[0];
    // Beside the head: one cell after its six columns.
    assert_eq!(note.x, layout.text_x + 7 * CELL_W);
    assert_eq!(note.y, layout.head_y);
    assert!(layout.rule.is_some());
}

#[test]
fn a_narrow_popup_puts_the_note_under_the_head() {
    let narrow = Rect {
        width: 16 * CELL_W + 2 + 2 * PAD_X + SHADOW,
        ..area()
    };
    let layout = SignatureLayout::new(
        place_in(narrow),
        &with_detail("Width in tiles of the block."),
        (0, 100),
        &[],
    )
    .unwrap();
    let note = &layout.notes[0];
    assert_eq!(note.x, layout.text_x);
    assert_eq!(note.y, layout.head_y + CELL_H);
}

#[test]
fn a_long_note_wraps_into_at_most_three_lines() {
    let note = "word ".repeat(80);
    let layout = laid_out(&with_detail(&note), (40, 100));
    assert_eq!(layout.notes.len(), 3);
    assert!(layout.notes.iter().all(|line| line.x == layout.notes[0].x));
    let ys: Vec<_> = layout.notes.iter().map(|line| line.y).collect();
    assert_eq!(ys, [ys[0], ys[0] + CELL_H, ys[0] + 2 * CELL_H]);
}

#[test]
fn a_view_with_nothing_to_add_has_neither_rule_nor_detail() {
    let layout = laid_out(&view(&["a"], Some(0)), (40, 100));
    assert_eq!(layout.rule, None);
    assert!(layout.head.is_empty() && layout.notes.is_empty());
    assert_eq!(layout.window.height, 2 + 2 * PAD_Y + CELL_H);
}

#[test]
fn the_window_is_tall_enough_for_every_line_it_draws() {
    let layout = laid_out(&with_detail("Width in tiles of the block."), (40, 100));
    let last = layout.notes.last().unwrap();
    assert!(last.y + CELL_H < layout.window.bottom());
    assert!(layout.rule.unwrap().y > layout.line_y + CELL_H - 1);
}

#[test]
fn nothing_is_laid_out_over_what_must_stay_uncovered() {
    let view = view(&["a", "b"], Some(0));
    let free = laid_out(&view, (40, 100)).window;

    let over = Rect {
        x: free.x + 2,
        y: free.y + 2,
        width: 10,
        height: 10,
    };
    assert_eq!(
        SignatureLayout::new(place_in(area()), &view, (40, 100), &[over]),
        None
    );
    // The popup's shadow counts as covering.
    let under_the_shadow = Rect {
        x: free.right(),
        y: free.y + 4,
        width: 3,
        height: 3,
    };
    assert_eq!(
        SignatureLayout::new(place_in(area()), &view, (40, 100), &[under_the_shadow]),
        None
    );
    let apart = Rect {
        x: free.right() + SHADOW + 1,
        y: free.y,
        width: 10,
        height: 10,
    };
    assert!(SignatureLayout::new(place_in(area()), &view, (40, 100), &[apart]).is_some());
}

#[test]
fn an_area_too_narrow_for_any_text_has_no_layout() {
    let sliver = Rect {
        width: 2 + 2 * PAD_X + SHADOW,
        ..area()
    };
    assert_eq!(
        SignatureLayout::new(place_in(sliver), &view(&["a"], Some(0)), (0, 100), &[]),
        None
    );
}
