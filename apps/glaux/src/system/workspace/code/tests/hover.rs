//! The quick look at the word under a resting pointer.

use super::*;
use crate::system::workspace::docs::DocTarget;
use pretty_assertions::assert_eq;

const DELAY: u32 = crate::system::workspace::code::hover::HOVER_DELAY_TICKS;

/// An editor over the one document `file` holding `text`, caret at its start.
fn editing(file: &str, text: &str) -> (CodeEditor, CodeSession) {
    let fixture = Fixture::new("code-hover");
    let (cartridge, mut session) = session_with_text(&fixture, &[file], text);
    let editor = editor();
    editor.refresh_metrics(&mut session);
    drop(cartridge);
    (editor, session)
}

fn luau(text: &str) -> (CodeEditor, CodeSession) {
    editing("a.luau", text)
}

/// A pixel inside the cell the character at `byte` is drawn in.
fn over(editor: &CodeEditor, session: &CodeSession, byte: usize) -> Point {
    let document = session.active_document().unwrap();
    let id = document.id();
    let geometry = editor.geometry(document);
    let tabs = session.tabs();
    let (view, metrics) = (tabs.view(id).unwrap(), tabs.metrics(id).unwrap());
    let at = spot(document, byte, metrics.key(), &editor.layout.font);
    let (x, y) = geometry.caret_point(*view, &at, metrics).unwrap();
    Point::new((x + 1) as u16, (y + 1) as u16)
}

fn tick(editor: &mut CodeEditor, session: &mut CodeSession, ticks: u32) {
    for _ in 0..ticks {
        editor.tick(Some(session));
    }
}

/// Moves the pointer onto the character at `byte` and waits for the popup.
fn rest_on(editor: &mut CodeEditor, session: &mut CodeSession, byte: usize) -> Point {
    let at = over(editor, session, byte);
    feed(editor, session, &[InputEvent::PointerMove(Some(at))]);
    tick(editor, session, DELAY);
    at
}

fn signature_of(editor: &CodeEditor) -> Option<String> {
    editor.hover().map(|popup| {
        popup
            .view()
            .signature
            .iter()
            .map(|(text, _)| text.as_str())
            .collect()
    })
}

fn page(id: &str) -> DocTarget {
    DocTarget::page(glaux_docs::DocId::parse(id).unwrap())
}

#[test]
fn the_popup_opens_when_the_pointer_has_rested_on_a_word() {
    let (mut editor, mut session) = luau("spr(1, 2, 3)\n");
    let at = over(&editor, &session, 1);
    feed(
        &mut editor,
        &mut session,
        &[InputEvent::PointerMove(Some(at))],
    );
    tick(&mut editor, &mut session, DELAY - 1);
    assert_eq!(editor.hover(), None);
    tick(&mut editor, &mut session, 1);
    assert_eq!(
        signature_of(&editor).as_deref().map(|s| &s[..3]),
        Some("spr")
    );
    assert_eq!(
        editor.hover().unwrap().target(),
        Some(&page("api/graphics/spr"))
    );
}

#[test]
fn moving_restarts_the_wait() {
    let (mut editor, mut session) = luau("spr(1, 2, 3)\n");
    let first = over(&editor, &session, 0);
    let second = over(&editor, &session, 2);
    feed(
        &mut editor,
        &mut session,
        &[InputEvent::PointerMove(Some(first))],
    );
    tick(&mut editor, &mut session, DELAY - 5);
    feed(
        &mut editor,
        &mut session,
        &[InputEvent::PointerMove(Some(second))],
    );
    tick(&mut editor, &mut session, DELAY - 5);
    assert_eq!(editor.hover(), None);
    tick(&mut editor, &mut session, 5);
    assert!(editor.hover().is_some());
}

#[test]
fn a_rhai_namespace_member_is_described_as_rhai_writes_it() {
    let (mut editor, mut session) = editing("a.rhai", "audio::tone(1);\n");
    rest_on(&mut editor, &mut session, 8);
    let shown = signature_of(&editor).unwrap();
    assert!(shown.starts_with("audio::tone("), "{shown}");
}

#[test]
fn constant_members_show_their_group() {
    let (mut editor, mut session) = luau("local k = Key.A\n");
    rest_on(&mut editor, &mut session, 14);
    assert_eq!(signature_of(&editor).as_deref(), Some("Key.A"));
}

#[test]
fn a_local_describes_where_it_is_declared_and_opens_no_page() {
    let (mut editor, mut session) = luau("local function mine() end\nmine()\n");
    rest_on(&mut editor, &mut session, 26);
    let popup = editor.hover().unwrap();
    assert_eq!(popup.target(), None);
    assert_eq!(
        popup.view().summary.as_deref(),
        Some("Function declared on line 1.")
    );
    assert_eq!(editor.hint(), ui_text::get("code-tab-more-esc-close"));
}

#[test]
fn tab_opens_the_whole_hover_even_when_the_word_has_no_page() {
    let (mut editor, mut session) = luau("local function mine() end\nmine()\n");
    rest_on(&mut editor, &mut session, 26);
    assert!(editor.hover().is_some());
    editor.handle_input(InputEvent::Tab { reverse: false }, Some(&mut session));
    assert!(editor.hover().is_none());
    assert!(editor.has_overlay());
    assert_eq!(editor.hint(), ui_text::get("code-up-down-scroll-esc-close"));
    editor.handle_input(InputEvent::Break, Some(&mut session));
    assert!(!editor.has_overlay());
}

#[test]
fn words_with_nothing_to_say_open_nothing() {
    let (mut editor, mut session) = luau("-- spr\nnothing_here()\nx = \"spr\"\nlocal y = 1\n");
    let text = session.active_document().unwrap().text().to_owned();
    for needle in ["spr", "nothing_here", "\"spr\"", " y "] {
        let byte = text.find(needle).unwrap() + 1;
        rest_on(&mut editor, &mut session, byte);
        assert_eq!(editor.hover(), None, "{needle}");
    }
    // Neither does the space past the end of a line.
    let end = over(&editor, &session, text.find('\n').unwrap());
    feed(
        &mut editor,
        &mut session,
        &[InputEvent::PointerMove(Some(end))],
    );
    tick(&mut editor, &mut session, DELAY);
    assert_eq!(editor.hover(), None);
}

#[test]
fn leaving_the_word_closes_the_popup() {
    let (mut editor, mut session) = luau("spr(1) + btnp(0)\n");
    rest_on(&mut editor, &mut session, 1);
    assert!(editor.hover().is_some());
    let elsewhere = over(&editor, &session, 10);
    feed(
        &mut editor,
        &mut session,
        &[InputEvent::PointerMove(Some(elsewhere))],
    );
    assert_eq!(editor.hover(), None);
    tick(&mut editor, &mut session, DELAY);
    assert!(signature_of(&editor).unwrap().starts_with("btnp"));

    feed(&mut editor, &mut session, &[InputEvent::PointerMove(None)]);
    assert_eq!(editor.hover(), None);
}

#[test]
fn the_pointer_may_cross_from_the_word_to_the_popup() {
    let (mut editor, mut session) = luau("spr(1)\n\n\n\n");
    rest_on(&mut editor, &mut session, 1);
    let (layout, word) = editor.hover_layout(&session).unwrap();
    // Below the word, then down into the popup.
    let into = |x: i32, y: i32| InputEvent::PointerMove(Some(Point::new(x as u16, y as u16)));
    let popup = layout.window;
    assert!(popup.y >= word.bottom());
    feed(
        &mut editor,
        &mut session,
        &[
            into(word.x + 1, word.y + 1),
            into(popup.x + 1, popup.y + 1),
            into(popup.right() - 1, popup.bottom() - 1),
        ],
    );
    assert!(editor.hover().is_some());
    tick(&mut editor, &mut session, DELAY * 2);
    assert!(editor.hover().is_some());
    // Past the popup's shadow it is gone.
    feed(
        &mut editor,
        &mut session,
        &[into(popup.right() + 4, popup.y + 1)],
    );
    assert_eq!(editor.hover(), None);
}

#[test]
fn pressing_the_popup_opens_the_page() {
    let (mut editor, mut session) = luau("local x = 1\nspr(1)\n");
    rest_on(&mut editor, &mut session, 13);
    let (layout, _) = editor.hover_layout(&session).unwrap();
    let caret = caret_of(&session, "a.luau").byte();
    let inside = centre(layout.window);
    let asked = requests(
        &mut editor,
        &mut session,
        &[press(PointerButton::Primary, inside), release(inside)],
    );
    assert_eq!(asked, vec![CodeRequest::OpenDocs(page("api/graphics/spr"))]);
    assert_eq!(editor.hover(), None);
    // The press did not reach the text under the popup.
    assert_eq!(caret_of(&session, "a.luau").byte(), caret);
}

#[test]
fn pressing_elsewhere_closes_the_popup_and_goes_on_to_the_text() {
    let (mut editor, mut session) = luau("spr(1) + 2\n");
    rest_on(&mut editor, &mut session, 1);
    let beside = over(&editor, &session, 9);
    feed(
        &mut editor,
        &mut session,
        &[press(PointerButton::Primary, beside)],
    );
    assert_eq!(editor.hover(), None);
    assert_eq!(caret_of(&session, "a.luau").byte(), 9);
}

#[test]
fn escape_closes_the_popup_and_stays_in_the_view() {
    let (mut editor, mut session) = luau("spr(1)\n");
    assert!(!editor.claims_break());
    rest_on(&mut editor, &mut session, 1);
    assert!(editor.claims_break());
    let asked = requests(&mut editor, &mut session, &[InputEvent::Break]);
    assert_eq!(asked, vec![]);
    assert_eq!(editor.hover(), None);
    assert!(!editor.claims_break());
}

#[test]
fn typing_scrolling_and_the_pointer_leaving_close_the_popup() {
    for closing in [
        InputEvent::Text('x'),
        InputEvent::Wheel { delta: 1 },
        InputEvent::Left { select: false },
    ] {
        let (mut editor, mut session) = luau("spr(1)\n");
        rest_on(&mut editor, &mut session, 1);
        feed(&mut editor, &mut session, &[closing]);
        assert_eq!(editor.hover(), None, "{closing:?}");
        // Still there after the wait: the pointer must move again.
        tick(&mut editor, &mut session, DELAY * 2);
        assert_eq!(editor.hover(), None, "{closing:?} later");
    }
}

#[test]
fn a_changed_text_drops_the_popup() {
    let (mut editor, mut session) = luau("spr(1)\n");
    rest_on(&mut editor, &mut session, 1);
    let id = session.active_document().unwrap().id();
    session.edit(
        id,
        0..0,
        "x",
        crate::system::editing::history::EditKind::Typing,
        None,
    );
    feed(&mut editor, &mut session, &[InputEvent::PointerMove(None)]);
    assert_eq!(editor.hover(), None);
}

#[test]
fn the_popup_never_opens_over_a_signature_or_the_suggestions() {
    let (mut editor, mut session) = luau("");
    for ch in "spr(".chars() {
        feed(&mut editor, &mut session, &[InputEvent::Text(ch)]);
    }
    assert!(editor.signature().is_some());
    rest_on(&mut editor, &mut session, 1);
    assert_eq!(editor.hover(), None);

    let (mut editor, mut session) = luau("spr(1)\n");
    rest_on(&mut editor, &mut session, 1);
    assert!(editor.hover().is_some());
    feed(&mut editor, &mut session, &[InputEvent::Complete]);
    assert!(editor.completion().is_some());
    assert_eq!(editor.hover(), None);
}

#[test]
fn overlays_hide_the_popup_and_block_new_ones() {
    let (mut editor, mut session) = luau("spr(1)\n");
    rest_on(&mut editor, &mut session, 1);
    let at = over(&editor, &session, 1);
    feed(
        &mut editor,
        &mut session,
        &[press(PointerButton::Secondary, at)],
    );
    assert!(editor.context_menu().is_some());
    assert_eq!(editor.hover(), None);
    tick(&mut editor, &mut session, DELAY * 2);
    assert_eq!(editor.hover(), None);
}

#[test]
fn forgetting_the_cartridge_drops_the_popup() {
    let (mut editor, mut session) = luau("spr(1)\n");
    rest_on(&mut editor, &mut session, 1);
    editor.forget();
    assert_eq!(editor.hover(), None);
}

#[test]
fn f1_opens_the_page_of_the_popup_before_the_caret_s_word() {
    let (mut editor, mut session) = luau("spr(1)\naudio.tone(2)\n");
    feed(
        &mut editor,
        &mut session,
        &[InputEvent::DocumentEnd { select: false }],
    );
    rest_on(&mut editor, &mut session, 1);
    let asked = requests(&mut editor, &mut session, &[InputEvent::Docs]);
    assert_eq!(asked, vec![CodeRequest::OpenDocs(page("api/graphics/spr"))]);
    assert_eq!(editor.hover(), None);
}

#[test]
fn the_hint_follows_the_popup() {
    let (mut editor, mut session) = luau("spr(1)\n");
    let idle = editor.hint();
    rest_on(&mut editor, &mut session, 1);
    assert_eq!(
        editor.hint(),
        ui_text::get("code-click-open-docs-tab-more-esc-close")
    );
    feed(&mut editor, &mut session, &[InputEvent::Break]);
    assert_eq!(editor.hint(), idle);
}
