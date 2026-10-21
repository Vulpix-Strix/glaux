use pretty_assertions::assert_eq;

use super::super::view::ParamRow;
use super::*;
use crate::system::test_font::LAYOUT_FONT;

const CELL_W: i32 = 6;
const CELL_H: i32 = 12;

fn area() -> Rect {
    Rect {
        x: 0,
        y: 28,
        width: 320,
        height: 188,
    }
}

fn place() -> Place<'static> {
    Place {
        font: &LAYOUT_FONT,
        area: area(),
        cell_w: CELL_W,
        cell_h: CELL_H,
    }
}

fn word_at(x: i32, y: i32) -> Rect {
    Rect {
        x,
        y,
        width: 3 * CELL_W,
        height: CELL_H,
    }
}

fn view() -> HoverView {
    let row = |name: &str| ParamRow {
        name: name.to_owned(),
        ty: "number",
        meaning: "Where it goes, counted from the top left corner.".to_owned(),
    };
    HoverView {
        signature: vec![("spr".to_owned(), SyntaxKind::Function)],
        tag: Some("graphics".to_owned()),
        summary: Some("Draws the sprite with this id at a position on the screen.".to_owned()),
        params: ["id", "x", "y", "flip"].map(row).to_vec(),
        returns: Some("bool"),
        opens_page: true,
    }
}

fn inside(outer: Rect, inner: Rect) -> bool {
    inner.x >= outer.x
        && inner.y >= outer.y
        && inner.right() <= outer.right()
        && inner.bottom() <= outer.bottom()
}

fn texts(layout: &HoverLayout) -> Vec<&str> {
    layout.texts.iter().map(|t| t.text.as_str()).collect()
}

#[test]
fn the_popup_sits_right_under_the_word() {
    let word = word_at(60, 60);
    let layout = HoverLayout::new(place(), &view(), word).expect("a layout");
    assert_eq!(layout.window.x, word.x);
    assert_eq!(layout.window.y, word.bottom());
    assert!(layout.hit_area().contains_xy(word.x + 1, word.bottom() + 1));
}

#[test]
fn it_goes_over_the_word_when_there_is_no_room_below() {
    let word = word_at(60, 190);
    let layout = HoverLayout::new(place(), &view(), word).expect("a layout");
    assert!(layout.window.bottom() + SHADOW <= word.y);
    assert!(inside(area(), layout.window));
}

#[test]
fn it_shrinks_before_giving_up() {
    // Not room for every row, but for the rest once parameters give way.
    let tight = Rect {
        height: 130,
        ..area()
    };
    let place = Place {
        area: tight,
        ..place()
    };
    let layout = HoverLayout::new(place, &view(), word_at(60, tight.y + 10)).expect("a layout");
    assert!(inside(tight, layout.hit_area()));
    assert!(texts(&layout).contains(&"spr"));
    assert!(texts(&layout).contains(&".."));
    assert!(!texts(&layout).contains(&"flip"));
}

#[test]
fn there_is_no_popup_when_not_even_the_shortest_fits() {
    let tiny = Rect {
        height: 30,
        ..area()
    };
    let place = Place {
        area: tiny,
        ..place()
    };
    assert_eq!(HoverLayout::new(place, &view(), word_at(60, tiny.y)), None);
}

#[test]
fn the_popup_stays_inside_the_area_wherever_the_word_is() {
    for x in (0..320).step_by(7) {
        for y in (28..204).step_by(11) {
            let word = word_at(x, y);
            if let Some(layout) = HoverLayout::new(place(), &view(), word) {
                let shadowed = layout.hit_area();
                assert!(
                    inside(area(), shadowed),
                    "{word:?} gave {:?}",
                    layout.window
                );
                assert!(layout.window.width <= 2 + 2 * PAD_X + 40 * CELL_W);
            }
        }
    }
}

#[test]
fn every_row_of_text_is_inside_the_window() {
    let layout = HoverLayout::new(place(), &view(), word_at(280, 60)).expect("a layout");
    for text in &layout.texts {
        let width = LAYOUT_FONT.text_cells(&text.text) as i32 * CELL_W;
        assert!(
            text.x + width < layout.window.right(),
            "{} runs out of the window",
            text.text
        );
        assert!(text.y + CELL_H <= layout.window.bottom());
    }
}

#[test]
fn a_word_with_no_page_has_no_foot() {
    let mut local = view();
    local.opens_page = false;
    local.params.clear();
    local.returns = None;
    local.tag = None;
    let layout = HoverLayout::new(place(), &local, word_at(60, 60)).expect("a layout");
    assert!(!texts(&layout).iter().any(|t| t.starts_with("F1")));
    let with_page = HoverLayout::new(place(), &view(), word_at(60, 60)).expect("a layout");
    assert!(texts(&with_page).iter().any(|t| t.starts_with("F1")));
}
