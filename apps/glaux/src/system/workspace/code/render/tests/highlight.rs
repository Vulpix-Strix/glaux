//! How syntax colors show, and how they give way to selections and matches.

use glaux_syntax::SyntaxKind;

use super::*;
use crate::system::editing::cursor::Caret;
use crate::system::editing::selection::Selection;
use crate::system::syntax_theme::color;
use pretty_assertions::assert_eq;

fn kind_ink(
    framebuffer: &Framebuffer,
    geometry: &TextGeometry,
    column: i32,
    kind: SyntaxKind,
) -> Vec<bool> {
    ink(framebuffer, cell(geometry, column, 0), color(kind))
}

#[test]
fn each_category_draws_in_its_color() {
    let fixture = Fixture::new("code-render-syntax");
    let mut editor = editor();
    hide_caret(&mut editor);
    let session = showing(&fixture, &editor, "local x = \"s\" -- c");
    let framebuffer = render(&editor, &session);
    let geometry = geometry_of(&editor, &session);
    let at = |column, kind| kind_ink(&framebuffer, &geometry, column, kind);
    assert_eq!(at(0, SyntaxKind::Keyword), glyph('l'));
    assert_eq!(at(6, SyntaxKind::Variable), glyph('x'));
    assert_eq!(at(8, SyntaxKind::Operator), glyph('='));
    assert_eq!(at(10, SyntaxKind::String), glyph('"'));
    assert_eq!(at(11, SyntaxKind::String), glyph('s'));
    assert_eq!(at(14, SyntaxKind::Comment), glyph('-'));
    // A keyword's cell holds nothing in any other color.
    assert_eq!(at(0, SyntaxKind::Variable), blank());
    assert_eq!(
        ink(&framebuffer, cell(&geometry, 0, 0), TEXT).len(),
        blank().len()
    );
}

#[test]
fn the_files_language_picks_the_grammar() {
    let fixture = Fixture::new("code-render-syntax-rhai");
    let mut editor = editor();
    hide_caret(&mut editor);
    let (_, mut session) = session_with_text(&fixture, &["a.rhai"], "fn f() {}");
    editor.refresh_metrics(&mut session);
    let framebuffer = render(&editor, &session);
    let geometry = geometry_of(&editor, &session);
    assert_eq!(
        kind_ink(&framebuffer, &geometry, 0, SyntaxKind::Keyword),
        glyph('f')
    );
    assert_eq!(
        kind_ink(&framebuffer, &geometry, 3, SyntaxKind::Function),
        glyph('f')
    );
}

#[test]
fn scrolling_highlights_the_lines_it_reveals() {
    let fixture = Fixture::new("code-render-syntax-scroll");
    let mut editor = editor();
    hide_caret(&mut editor);
    let text = "local a = 1\n".repeat(300);
    let mut session = showing(&fixture, &editor, &text);
    let id = id_of(&session, "a.luau");
    session.tabs_mut().view_mut(id).unwrap().top_line = 200;
    editor.refresh_metrics(&mut session);
    let framebuffer = render(&editor, &session);
    let geometry = geometry_of(&editor, &session);
    let last = geometry.rows as i32 - 1;
    for row in [0, last] {
        assert_eq!(
            ink(
                &framebuffer,
                cell(&geometry, 0, row),
                color(SyntaxKind::Keyword)
            ),
            glyph('l'),
            "row {row}"
        );
    }
}

#[test]
fn edits_recolor_the_text() {
    let fixture = Fixture::new("code-render-syntax-edit");
    let mut editor = editor();
    hide_caret(&mut editor);
    let mut session = showing(&fixture, &editor, "local x");
    let id = id_of(&session, "a.luau");
    let at = |editor: &CodeEditor, session: &CodeSession, kind| {
        let framebuffer = render(editor, session);
        kind_ink(&framebuffer, &geometry_of(editor, session), 0, kind)
    };
    assert_eq!(at(&editor, &session, SyntaxKind::Keyword), glyph('l'));

    session
        .edit(id, 0..5, "lo", EditKind::Typing, None)
        .unwrap()
        .unwrap();
    editor.refresh_metrics(&mut session);
    assert_eq!(at(&editor, &session, SyntaxKind::Variable), glyph('l'));

    session.undo(id).unwrap();
    editor.refresh_metrics(&mut session);
    assert_eq!(at(&editor, &session, SyntaxKind::Keyword), glyph('l'));

    session.redo(id).unwrap();
    editor.refresh_metrics(&mut session);
    assert_eq!(at(&editor, &session, SyntaxKind::Variable), glyph('l'));
}

#[test]
fn selected_text_is_inverted_whatever_its_category() {
    let fixture = Fixture::new("code-render-syntax-selected");
    let mut editor = editor();
    hide_caret(&mut editor);
    let mut session = showing(&fixture, &editor, "local x");
    let id = id_of(&session, "a.luau");
    let document = session.active_document().unwrap();
    let selection = Selection::between(document, 0, Caret::at(document, 5));
    session.tabs_mut().set_selection(id, selection);
    let framebuffer = render(&editor, &session);
    let geometry = geometry_of(&editor, &session);
    assert_eq!(
        kind_ink(&framebuffer, &geometry, 0, SyntaxKind::Keyword),
        blank()
    );
    assert_eq!(
        ink(&framebuffer, cell(&geometry, 0, 0), BACKGROUND),
        glyph('l')
    );
    // Past the selection the category shows again.
    assert_eq!(
        kind_ink(&framebuffer, &geometry, 6, SyntaxKind::Variable),
        glyph('x')
    );
}

#[test]
fn found_text_keeps_the_plain_color_on_its_highlight() {
    let fixture = Fixture::new("code-render-syntax-found");
    let mut editor = editor();
    let mut session = showing(&fixture, &editor, "local local");
    editor.handle_input(InputEvent::Find, Some(&mut session));
    for ch in "local".chars() {
        editor.handle_input(InputEvent::Text(ch), Some(&mut session));
    }
    hide_caret(&mut editor);
    let framebuffer = render(&editor, &session);
    let geometry = editor.geometry(session.active_document().unwrap());
    // One of the two matches is selected. The other is only highlighted.
    let selected = session
        .tabs()
        .selection(id_of(&session, "a.luau"))
        .unwrap()
        .range();
    let column = if selected.start == 0 { 6 } else { 0 };
    assert_eq!(
        kind_ink(&framebuffer, &geometry, column, SyntaxKind::Keyword),
        blank()
    );
    assert_eq!(
        ink(&framebuffer, cell(&geometry, column, 0), TEXT),
        glyph('l')
    );
    assert!(ink(&framebuffer, cell(&geometry, column, 0), MATCH).contains(&true));
}
