//! Which pointer the workspace shows.

use glaux_machine::system_rom::SystemCursor;

use super::*;
use crate::cartridge::fixture::write_cartridge;
use crate::cartridge_fs::fixture::Fixture;
use crate::system::workspace::tests_support::tab_rect;
use pretty_assertions::assert_eq;

/// A workspace on the Code view with `a.luau` open, and its session.
fn code_view(fixture: &Fixture) -> (Workspace, CodeSession) {
    let dir = fixture.root().join("games/pong");
    write_cartridge(&dir, "Pong");
    std::fs::write(dir.join("code/a.luau"), "local x = 1\nreturn x\n").unwrap();
    let cartridge = LoadedCartridge::load(&fixture.storage(), "games/pong").unwrap();
    let mut session = CodeSession::default();
    let path = SourcePath::parse("code/a.luau").unwrap();
    session.open(&cartridge, &path).unwrap();
    let workspace = Workspace::new(&LAYOUT_FONT, Palette::default());
    workspace.code().refresh_metrics(&mut session);
    (workspace, session)
}

fn centre(rect: Rect) -> Point {
    Point::new(
        (rect.x + rect.width / 2) as u16,
        (rect.y + rect.height / 2) as u16,
    )
}

fn cursor(workspace: &Workspace, at: Point, session: &CodeSession) -> SystemCursor {
    workspace.cursor(at, Some(session)).0
}

fn send(workspace: &mut Workspace, session: &mut CodeSession, event: InputEvent) {
    workspace.handle_input(event, None, Some(session));
}

fn press(at: Point) -> InputEvent {
    InputEvent::PointerDown {
        button: PointerButton::Primary,
        at,
        select: false,
    }
}

fn release(at: Point) -> InputEvent {
    InputEvent::PointerUp {
        button: PointerButton::Primary,
        at,
    }
}

#[test]
fn the_code_view_shows_an_i_beam_over_text_and_a_hand_over_controls() {
    let fixture = Fixture::new("ws-cursor-code");
    let (workspace, session) = code_view(&fixture);
    let code = workspace.code();
    let (_, strip_tab, close) = code.tab_rects(&session)[0];
    for (at, expected) in [
        (centre(code.layout().text), SystemCursor::Text),
        (centre(strip_tab), SystemCursor::Hand),
        (centre(close), SystemCursor::Hand),
        (centre(code.layout().settings_button()), SystemCursor::Hand),
        (centre(code.layout().status), SystemCursor::Arrow),
        (
            centre(tab_rect(Tab::Sprites, FontMetrics::SYSTEM)),
            SystemCursor::Hand,
        ),
    ] {
        assert_eq!(cursor(&workspace, at, &session), expected, "at {at:?}");
    }
    // With nothing open there is no text to point into.
    let empty = CodeSession::default();
    let text = centre(code.layout().text);
    assert_eq!(cursor(&workspace, text, &empty), SystemCursor::Arrow);
}

#[test]
fn drags_keep_their_pointer_wherever_it_goes() {
    let fixture = Fixture::new("ws-cursor-drag");
    let (mut workspace, mut session) = code_view(&fixture);
    let text = centre(workspace.code().layout().text);
    let bar = centre(tab_rect(Tab::Sprites, FontMetrics::SYSTEM));

    // Selecting text keeps the I-beam even over the tab bar.
    send(&mut workspace, &mut session, press(text));
    send(
        &mut workspace,
        &mut session,
        InputEvent::PointerMove(Some(bar)),
    );
    assert_eq!(cursor(&workspace, bar, &session), SystemCursor::Text);
    send(&mut workspace, &mut session, release(bar));
    assert_eq!(cursor(&workspace, bar, &session), SystemCursor::Hand);

    // A tab being dragged is held in a closed hand.
    let (_, strip_tab, _) = workspace.code().tab_rects(&session)[0];
    send(&mut workspace, &mut session, press(centre(strip_tab)));
    assert_eq!(cursor(&workspace, text, &session), SystemCursor::Grab);
    send(&mut workspace, &mut session, InputEvent::PointerCancel);
    assert_eq!(cursor(&workspace, text, &session), SystemCursor::Text);
}

#[test]
fn an_overlay_hides_the_view_below_from_the_pointer() {
    let fixture = Fixture::new("ws-cursor-overlay");
    let (mut workspace, mut session) = code_view(&fixture);
    let text = centre(workspace.code().layout().text);
    let bar = centre(tab_rect(Tab::Sprites, FontMetrics::SYSTEM));
    let settings = centre(workspace.code().layout().settings_button());
    send(&mut workspace, &mut session, press(settings));
    assert!(workspace.code().has_overlay());
    assert_eq!(cursor(&workspace, bar, &session), SystemCursor::Arrow);
    let rows = workspace.code().settings_rows();
    // The window covers the text; outside its rows only the arrow shows.
    let outside = Point::new(1, text.y);
    assert_eq!(cursor(&workspace, outside, &session), SystemCursor::Arrow);
    assert_eq!(
        cursor(&workspace, centre(rows[0]), &session),
        SystemCursor::Hand
    );
}

#[test]
fn animated_cursors_step_through_their_frames_over_time() {
    let fixture = Fixture::new("ws-cursor-frames");
    let (mut workspace, session) = code_view(&fixture);
    let at = Point::new(1, 1);
    let frames: Vec<usize> = (0..3)
        .map(|_| {
            let (_, frame) = workspace.cursor(at, Some(&session));
            for _ in 0..CURSOR_FRAME_TICKS {
                workspace.tick(None);
            }
            frame
        })
        .collect();
    assert_eq!(frames, [0, 1, 2]);
}
