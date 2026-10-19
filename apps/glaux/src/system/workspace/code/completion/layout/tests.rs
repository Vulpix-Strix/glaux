use glaux_input::Point;
use glaux_sense::{Doc, Insertion, Origin};
use glaux_syntax::SyntaxKind;
use pretty_assertions::assert_eq;

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

fn place() -> Place<'static> {
    Place {
        font: &LAYOUT_FONT,
        area: area(),
        cell_w: CELL_W,
        cell_h: CELL_H,
    }
}

fn items(count: usize) -> Vec<Candidate> {
    (0..count)
        .map(|n| Candidate {
            label: format!("item{n}"),
            kind: glaux_sense::CandidateKind::Function,
            origin: Origin::Api,
            insertion: Insertion::Name(format!("item{n}")),
            detail: "gfx".to_owned(),
            doc: Doc::None,
        })
        .collect()
}

fn panel() -> DocPanel {
    DocPanel {
        signature: vec![("item0(a, b)".to_owned(), SyntaxKind::Function)],
        summary: Some("Fills a rectangle with one color and nothing else at all.".to_owned()),
        params: Some("a: int  b: int".to_owned()),
    }
}

fn laid_out(count: usize, selected: usize, first: usize, anchor: (i32, i32)) -> CompletionLayout {
    CompletionLayout::new(
        place(),
        &items(count),
        first,
        selected,
        Some(&panel()),
        anchor,
    )
}

#[test]
fn the_list_opens_under_the_line_at_the_start_of_the_word() {
    let layout = laid_out(3, 0, 0, (60, 40));
    assert_eq!((layout.list.x, layout.list.y), (60, 40 + CELL_H));
    assert_eq!(layout.rows.len(), 3);
    assert_eq!(layout.rows[0].1.y, layout.list.y + 2);
    assert_eq!(layout.rows[1].1.y, layout.rows[0].1.y + CELL_H);
    assert_eq!(layout.track, None);
}

#[test]
fn the_list_flips_above_the_line_when_it_does_not_fit_below() {
    let layout = laid_out(4, 0, 0, (60, 200));
    assert!(layout.list.bottom() + SHADOW <= 200);
    assert!(layout.list.y >= area().y);
}

#[test]
fn the_list_stays_inside_the_area_at_the_right_edge() {
    let layout = laid_out(3, 0, 0, (318, 40));
    assert!(layout.list.right() + SHADOW <= area().right());
    let left = laid_out(3, 0, 0, (-20, 40));
    assert_eq!(left.list.x, area().x);
}

#[test]
fn the_list_shows_eight_rows_and_scrolls_for_more() {
    let layout = laid_out(20, 11, 5, (60, 40));
    assert_eq!(layout.rows.len(), VISIBLE_ROWS);
    assert_eq!(layout.rows.first().unwrap().0, 5);
    assert_eq!(layout.rows.last().unwrap().0, 12);
    let track = layout.track.unwrap();
    let thumb = layout.thumb.unwrap();
    assert!(thumb.y > track.y && thumb.bottom() <= track.bottom());
    // The scroll indicator sits inside the border, right of the rows.
    assert!(track.x >= layout.rows[0].1.right());
    assert!(track.right() < layout.list.right());
}

#[test]
fn rows_find_the_candidate_under_a_point() {
    let layout = laid_out(20, 0, 5, (60, 40));
    let row = layout.rows[2].1;
    let at = Point::new(row.x as u16 + 3, row.y as u16 + 1);
    assert_eq!(layout.row_at(at), Some(7));
    assert_eq!(layout.row_at(Point::new(0, 0)), None);
}

#[test]
fn the_panel_sits_right_of_the_list_level_with_the_picked_row() {
    let layout = laid_out(5, 2, 0, (20, 40));
    let panel = layout.panel.unwrap();
    assert_eq!(panel.window.x, layout.list.right() + 2);
    assert_eq!(panel.window.y, layout.rows[2].1.y - 1);
    assert!(panel.window.right() + SHADOW <= area().right());
}

#[test]
fn the_panel_moves_left_when_the_right_has_no_room() {
    let layout = laid_out(5, 0, 0, (240, 40));
    let panel = layout.panel.unwrap();
    assert!(panel.window.right() <= layout.list.x);
    assert!(panel.window.x >= area().x);
}

#[test]
fn the_panel_is_left_out_when_neither_side_can_hold_readable_text() {
    let narrow = Place {
        area: Rect {
            width: 130,
            ..area()
        },
        ..place()
    };
    let layout = CompletionLayout::new(narrow, &items(3), 0, 0, Some(&panel()), (4, 40));
    assert_eq!(layout.panel, None);
}

#[test]
fn the_panel_keeps_inside_the_area_below_the_picked_row() {
    let layout = laid_out(8, 7, 0, (20, 120));
    let panel = layout.panel.unwrap();
    assert!(panel.window.bottom() + SHADOW <= area().bottom());
    assert!(panel.window.y >= area().y);
}

#[test]
fn the_panel_has_a_signature_a_rule_and_text_in_order_above_its_foot() {
    let layout = laid_out(3, 0, 0, (20, 40));
    let panel = layout.panel.unwrap();
    let kinds: Vec<&str> = panel
        .lines
        .iter()
        .map(|line| match line {
            PanelLine::Signature { .. } => "signature",
            PanelLine::Rule { .. } => "rule",
            PanelLine::Summary { .. } => "summary",
            PanelLine::Params { .. } => "params",
        })
        .collect();
    assert_eq!(kinds[0], "signature");
    assert_eq!(kinds[1], "rule");
    assert!(kinds.contains(&"summary") && kinds.contains(&"params"));
    let last = match panel.lines.last().unwrap() {
        PanelLine::Params { y, .. } => *y,
        other => panic!("{other:?}"),
    };
    assert!(last + CELL_H <= panel.foot.y);
    assert_eq!(panel.foot.bottom(), panel.window.bottom() - 1);
}

#[test]
fn a_panel_without_text_has_only_the_signature() {
    let bare = DocPanel {
        summary: None,
        params: None,
        ..panel()
    };
    let layout = CompletionLayout::new(place(), &items(3), 0, 0, Some(&bare), (20, 40));
    let lines = layout.panel.unwrap().lines;
    assert!(matches!(lines.as_slice(), [PanelLine::Signature { .. }]));
}
