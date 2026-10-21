use super::*;
use KeyboardKey::*;
use pretty_assertions::assert_eq;

const FRAME: Duration = Duration::from_millis(16);
const PRESSED: KeyState = KeyState {
    pressed: true,
    down: true,
};
const HELD: KeyState = KeyState {
    pressed: false,
    down: true,
};

/// Events for one frame in which every key in `keys` has `state` and
/// every other key is up.
fn frame(
    edit: &mut EditKeys,
    keys: &[KeyboardKey],
    state: KeyState,
    shift: bool,
    elapsed: Duration,
) -> Vec<InputEvent> {
    let modifiers = Modifiers {
        shift,
        ..Modifiers::default()
    };
    frame_with(edit, keys, state, modifiers, elapsed)
}

/// [`frame`] with any modifiers held.
fn frame_with(
    edit: &mut EditKeys,
    keys: &[KeyboardKey],
    state: KeyState,
    modifiers: Modifiers,
    elapsed: Duration,
) -> Vec<InputEvent> {
    let mut out = Vec::new();
    edit.events(
        |key| {
            if keys.contains(&key) {
                state
            } else {
                KeyState::default()
            }
        },
        modifiers,
        elapsed,
        &mut out,
    );
    out
}

fn press(keys: &[KeyboardKey], shift: bool) -> Vec<InputEvent> {
    frame(&mut EditKeys::default(), keys, PRESSED, shift, FRAME)
}

/// Presses `key`, holds it for `frames` frames of `FRAME` each, and
/// returns how many events each frame produced, press frame first.
fn hold(key: KeyboardKey, frames: usize) -> Vec<usize> {
    let mut edit = EditKeys::default();
    let mut counts = vec![frame(&mut edit, &[key], PRESSED, false, FRAME).len()];
    for _ in 0..frames {
        counts.push(frame(&mut edit, &[key], HELD, false, FRAME).len());
    }
    counts
}

#[test]
fn movement_keys_select_only_with_shift() {
    let keys = [KEY_LEFT, KEY_RIGHT, KEY_HOME, KEY_END, KEY_UP, KEY_DOWN];
    for select in [false, true] {
        assert_eq!(
            press(&keys, select),
            [
                InputEvent::Left { select },
                InputEvent::Right { select },
                InputEvent::Home { select },
                InputEvent::End { select },
                InputEvent::Up { select },
                InputEvent::Down { select },
            ]
        );
    }
}

#[test]
fn ctrl_movement_keys_move_by_word_and_document() {
    let keys = [KEY_LEFT, KEY_RIGHT, KEY_HOME, KEY_END];
    for shift in [false, true] {
        let select = shift;
        let ctrl = Modifiers {
            shift,
            ctrl: true,
            ..Modifiers::default()
        };
        assert_eq!(
            frame_with(&mut EditKeys::default(), &keys, PRESSED, ctrl, FRAME),
            [
                InputEvent::WordLeft { select },
                InputEvent::WordRight { select },
                InputEvent::DocumentStart { select },
                InputEvent::DocumentEnd { select },
            ]
        );
    }
}

#[test]
fn page_keys_page_without_ctrl() {
    assert_eq!(
        press(&[KEY_PAGE_UP, KEY_PAGE_DOWN], false),
        [
            InputEvent::PageUp { select: false },
            InputEvent::PageDown { select: false },
        ]
    );
    assert_eq!(
        press(&[KEY_PAGE_DOWN], true),
        [InputEvent::PageDown { select: true }]
    );
}

#[test]
fn held_ctrl_page_keys_never_page_or_repeat() {
    // With Ctrl held, PageDown is only the NextDocument shortcut, which
    // fires once per press.
    let ctrl = Modifiers {
        ctrl: true,
        ..Modifiers::default()
    };
    let mut edit = EditKeys::default();
    let mut events = frame_with(&mut edit, &[KEY_PAGE_DOWN], PRESSED, ctrl, FRAME);
    let mut commands = Vec::new();
    shortcut_events(|key| key == KEY_PAGE_DOWN, ctrl, &mut commands);
    for _ in 0..120 {
        events.extend(frame_with(&mut edit, &[KEY_PAGE_DOWN], HELD, ctrl, FRAME));
        shortcut_events(|_| false, ctrl, &mut commands);
    }
    assert_eq!(events, []);
    assert_eq!(commands, [InputEvent::NextDocument]);

    // Letting go of Ctrl mid-hold starts a fresh delay before paging.
    assert_eq!(frame(&mut edit, &[KEY_PAGE_DOWN], HELD, false, FRAME), []);
    assert_eq!(
        frame(&mut edit, &[KEY_PAGE_DOWN], HELD, false, REPEAT_DELAY),
        [InputEvent::PageDown { select: false }]
    );
}

#[test]
fn editing_keys_ignore_shift() {
    let keys = [KEY_BACKSPACE, KEY_DELETE, KEY_ENTER];
    let expected = [InputEvent::Backspace, InputEvent::Delete, InputEvent::Enter];
    assert_eq!(press(&keys, false), expected);
    assert_eq!(press(&keys, true), expected);
}

#[test]
fn ctrl_tab_switches_views_instead_of_tabbing() {
    let press_with = |shift, ctrl| {
        let mut out = Vec::new();
        EditKeys::default().events(
            |key| {
                if key == KEY_TAB {
                    PRESSED
                } else {
                    KeyState::default()
                }
            },
            Modifiers {
                shift,
                ctrl,
                ..Modifiers::default()
            },
            FRAME,
            &mut out,
        );
        out
    };
    assert_eq!(
        press_with(false, false),
        [InputEvent::Tab { reverse: false }]
    );
    assert_eq!(press_with(true, false), [InputEvent::Tab { reverse: true }]);
    assert_eq!(press_with(false, true), [InputEvent::NextView]);
    assert_eq!(press_with(true, true), [InputEvent::PreviousView]);
}

#[test]
fn view_and_pointer_events_are_kept_only_in_workspace_mode() {
    let at = glaux_input::Point::new(1, 2);
    let all = [
        InputEvent::Text('a'),
        InputEvent::Break,
        InputEvent::NextView,
        InputEvent::PointerMove(Some(at)),
        InputEvent::PointerDown {
            button: glaux_input::PointerButton::Primary,
            at,
            select: false,
        },
        InputEvent::Wheel { delta: -1 },
        InputEvent::ButtonDown(Button::A),
    ];
    let kept = |mode| {
        let mut events = all.to_vec();
        retain_for_mode(mode, &mut events);
        events
    };
    assert_eq!(
        kept(InputMode::Text),
        [InputEvent::Text('a'), InputEvent::Break]
    );
    assert_eq!(
        kept(InputMode::Game),
        [InputEvent::Break, InputEvent::ButtonDown(Button::A)]
    );
    assert_eq!(kept(InputMode::Workspace), all[..6]);
}

#[test]
fn the_run_keys_work_in_the_workspace_and_f12_never_reaches_a_game() {
    assert_eq!(press(&[KeyboardKey::KEY_F5], false), [InputEvent::Run]);
    assert_eq!(
        press(&[KeyboardKey::KEY_F12], false),
        [InputEvent::ToggleLogs]
    );
    assert_eq!(shortcuts(&[KEY_R], false, true), [InputEvent::Run]);
    assert_eq!(shortcuts(&[KEY_R], true, true), [InputEvent::Restart]);
    assert_eq!(
        shortcuts(&[KeyboardKey::KEY_PERIOD], false, true),
        [InputEvent::Stop]
    );

    let all = [
        InputEvent::Run,
        InputEvent::Stop,
        InputEvent::Restart,
        InputEvent::ToggleLogs,
        InputEvent::KeyDown(Key::F5),
        InputEvent::KeyDown(Key::F12),
        InputEvent::KeyUp(Key::F12),
    ];
    let kept = |mode| {
        let mut events = all.to_vec();
        retain_for_mode(mode, &mut events);
        events
    };
    // A game keeps F5 as a key but F12 is the system's.
    assert_eq!(
        kept(InputMode::Game),
        [InputEvent::ToggleLogs, InputEvent::KeyDown(Key::F5)]
    );
    assert_eq!(kept(InputMode::Workspace), all[..4]);
    assert_eq!(kept(InputMode::Text), Vec::<InputEvent>::new());
}

fn shortcuts(keys: &[KeyboardKey], shift: bool, ctrl: bool) -> Vec<InputEvent> {
    let mut out = Vec::new();
    shortcut_events(
        |key| keys.contains(&key),
        Modifiers {
            shift,
            ctrl,
            ..Modifiers::default()
        },
        &mut out,
    );
    out
}

#[test]
fn ctrl_shortcuts_become_editing_commands() {
    assert_eq!(shortcuts(&[KEY_Z], false, true), [InputEvent::Undo]);
    assert_eq!(shortcuts(&[KEY_Z], true, true), [InputEvent::Redo]);
    assert_eq!(shortcuts(&[KEY_Y], false, true), [InputEvent::Redo]);
    assert_eq!(shortcuts(&[KEY_Y], true, true), [InputEvent::Redo]);
    assert_eq!(shortcuts(&[KEY_S], false, true), [InputEvent::Save]);
    assert_eq!(shortcuts(&[KEY_A], false, true), [InputEvent::SelectAll]);
    assert_eq!(shortcuts(&[KEY_Q], false, true), []);
}

#[test]
fn ctrl_shortcuts_become_document_commands() {
    assert_eq!(shortcuts(&[KEY_P], false, true), [InputEvent::OpenFile]);
    assert_eq!(
        shortcuts(&[KEY_W], false, true),
        [InputEvent::CloseDocument]
    );
    assert_eq!(
        shortcuts(&[KEY_PAGE_DOWN], false, true),
        [InputEvent::NextDocument]
    );
    assert_eq!(
        shortcuts(&[KEY_PAGE_UP], false, true),
        [InputEvent::PreviousDocument]
    );
    assert_eq!(shortcuts(&[KEY_N], false, true), [InputEvent::NewFile]);
    assert_eq!(shortcuts(&[KEY_F], false, true), [InputEvent::Find]);
    assert_eq!(shortcuts(&[KEY_H], false, true), [InputEvent::Replace]);
    assert_eq!(shortcuts(&[KEY_G], false, true), [InputEvent::GoToLine]);
    assert_eq!(shortcuts(&[KEY_SPACE], false, true), [InputEvent::Complete]);
    for shift in [false, true] {
        assert_eq!(shortcuts(&[KEY_O], shift, true), [InputEvent::GoToSymbol]);
    }
}

#[test]
fn f11_goes_to_the_definition_and_f12_still_toggles_the_logs() {
    assert_eq!(
        press(&[KeyboardKey::KEY_F11], false),
        [InputEvent::GoToDefinition]
    );
    assert_eq!(
        press(&[KeyboardKey::KEY_F12], false),
        [InputEvent::ToggleLogs]
    );
}

#[test]
fn ctrl_shift_space_asks_for_the_signature_instead_of_suggestions() {
    assert_eq!(
        shortcuts(&[KEY_SPACE], true, true),
        [InputEvent::SignatureHelp]
    );
}

#[test]
fn ctrl_b_toggles_the_side_panel() {
    assert_eq!(
        shortcuts(&[KEY_B], false, true),
        [InputEvent::ToggleSidebar]
    );
    assert_eq!(shortcuts(&[KEY_B], false, false), []);
}

#[test]
fn f1_opens_the_docs_once_per_press() {
    assert_eq!(press(&[KEY_F1], false), [InputEvent::Docs]);
    let counts = hold(KEY_F1, 40);
    assert_eq!(counts[0], 1);
    assert!(counts[1..].iter().all(|&count| count == 0), "{counts:?}");
}

#[test]
fn alt_with_a_horizontal_arrow_goes_back_and_forward() {
    let alt = Modifiers {
        alt: true,
        ..Modifiers::default()
    };
    let key = |key| frame_with(&mut EditKeys::default(), &[key], PRESSED, alt, FRAME);
    assert_eq!(key(KEY_LEFT), [InputEvent::Back]);
    assert_eq!(key(KEY_RIGHT), [InputEvent::Forward]);
    // Ctrl keeps its word movement even with Alt held, and Alt alone
    // elsewhere changes nothing.
    let both = Modifiers {
        ctrl: true,
        alt: true,
        ..Modifiers::default()
    };
    assert_eq!(
        frame_with(&mut EditKeys::default(), &[KEY_LEFT], PRESSED, both, FRAME),
        [InputEvent::WordLeft { select: false }]
    );
    assert_eq!(
        frame_with(&mut EditKeys::default(), &[KEY_DOWN], PRESSED, alt, FRAME),
        [InputEvent::Down { select: false }]
    );
}

#[test]
fn ctrl_shortcuts_become_clipboard_commands() {
    assert_eq!(shortcuts(&[KEY_C], false, true), [InputEvent::Copy]);
    assert_eq!(shortcuts(&[KEY_X], false, true), [InputEvent::Cut]);
    assert_eq!(shortcuts(&[KEY_V], false, true), [InputEvent::Paste]);
}

#[test]
fn shortcut_keys_without_ctrl_are_not_commands() {
    for shift in [false, true] {
        assert_eq!(
            shortcuts(
                &[
                    KEY_Z,
                    KEY_Y,
                    KEY_S,
                    KEY_P,
                    KEY_W,
                    KEY_PAGE_UP,
                    KEY_PAGE_DOWN,
                    KEY_N,
                    KEY_F,
                    KEY_H,
                    KEY_G,
                    KEY_O,
                    KEY_SPACE,
                    KEY_C,
                    KEY_X,
                    KEY_V
                ],
                shift,
                false
            ),
            []
        );
    }
}

#[test]
fn editing_commands_are_kept_only_in_workspace_mode() {
    let all = [
        InputEvent::Undo,
        InputEvent::Redo,
        InputEvent::Save,
        InputEvent::SelectAll,
        InputEvent::OpenFile,
        InputEvent::CloseDocument,
        InputEvent::NextDocument,
        InputEvent::PreviousDocument,
        InputEvent::NewFile,
        InputEvent::Find,
        InputEvent::Replace,
        InputEvent::GoToLine,
        InputEvent::Complete,
        InputEvent::SignatureHelp,
        InputEvent::Docs,
        InputEvent::Back,
        InputEvent::Forward,
        InputEvent::GoToDefinition,
        InputEvent::GoToSymbol,
        InputEvent::ToggleSidebar,
        InputEvent::Copy,
        InputEvent::Cut,
        InputEvent::Paste,
    ];
    for (mode, kept) in [
        (InputMode::Text, 0),
        (InputMode::Game, 0),
        (InputMode::Workspace, all.len()),
    ] {
        let mut events = all.to_vec();
        retain_for_mode(mode, &mut events);
        assert_eq!(events, all[..kept], "{mode:?}");
    }
}

#[test]
fn keypad_enter_is_enter() {
    assert_eq!(press(&[KEY_KP_ENTER], false), [InputEvent::Enter]);
}

#[test]
fn escape_is_break_and_does_not_repeat() {
    assert_eq!(press(&[KEY_ESCAPE], false), [InputEvent::Break]);
    assert_eq!(press(&[KEY_ESCAPE], true), [InputEvent::Break]);
    assert_eq!(hold(KEY_ESCAPE, 120).iter().sum::<usize>(), 1);
}

#[test]
fn unrelated_keys_are_ignored() {
    assert_eq!(press(&[KEY_A, KEY_LEFT_SHIFT], true), []);
}

#[test]
fn held_key_without_press_does_not_fire() {
    let mut edit = EditKeys::default();
    let long = REPEAT_DELAY * 2;
    assert_eq!(frame(&mut edit, &[KEY_BACKSPACE], HELD, false, long), []);
}

#[test]
fn held_key_waits_for_delay_then_repeats_at_interval() {
    let mut edit = EditKeys::default();
    let key = &[KEY_BACKSPACE];
    assert_eq!(frame(&mut edit, key, PRESSED, false, FRAME).len(), 1);

    let just_before = REPEAT_DELAY - Duration::from_millis(1);
    assert_eq!(frame(&mut edit, key, HELD, false, just_before), []);
    assert_eq!(
        frame(&mut edit, key, HELD, false, Duration::from_millis(1)),
        [InputEvent::Backspace]
    );

    let half = REPEAT_INTERVAL / 2;
    assert_eq!(frame(&mut edit, key, HELD, false, half), []);
    assert_eq!(
        frame(&mut edit, key, HELD, false, REPEAT_INTERVAL - half),
        [InputEvent::Backspace]
    );
}

#[test]
fn repeating_keys_repeat_while_held() {
    // With a 0.5 s delay, a 33 ms interval and 16 ms frames, the first
    // repeat lands on frame 32 and then roughly every other frame.
    for key in [
        KEY_LEFT,
        KEY_RIGHT,
        KEY_HOME,
        KEY_END,
        KEY_UP,
        KEY_DOWN,
        KEY_PAGE_UP,
        KEY_PAGE_DOWN,
        KEY_BACKSPACE,
        KEY_DELETE,
    ] {
        let counts = hold(key, 40);
        assert_eq!(counts[0], 1, "{key:?} press");
        assert!(counts[1..32].iter().all(|&n| n == 0), "{key:?} delay");
        assert_eq!(counts[32..].iter().sum::<usize>(), 5, "{key:?} repeats");
    }
}

#[test]
fn enter_does_not_repeat() {
    for key in [KEY_ENTER, KEY_KP_ENTER] {
        assert_eq!(hold(key, 120).iter().sum::<usize>(), 1, "{key:?}");
    }
}

#[test]
fn tab_does_not_repeat() {
    assert_eq!(hold(KEY_TAB, 120).iter().sum::<usize>(), 1);
}

#[test]
fn tab_press_does_not_start_text_repeat() {
    let mut text = TextKeys::default();
    assert_eq!(
        text_frame(&mut text, "\t", &[KEY_TAB], &[], FRAME),
        texts("\t")
    );
    assert_eq!(text_frame(&mut text, "", &[], &[KEY_TAB], REPEAT_DELAY), []);
}

#[test]
fn repeats_use_current_shift_state() {
    let mut edit = EditKeys::default();
    frame(&mut edit, &[KEY_LEFT], PRESSED, false, FRAME);
    assert_eq!(
        frame(&mut edit, &[KEY_LEFT], HELD, true, REPEAT_DELAY),
        [InputEvent::Left { select: true }]
    );
}

#[test]
fn stalled_frame_emits_a_single_repeat() {
    let mut edit = EditKeys::default();
    frame(&mut edit, &[KEY_DELETE], PRESSED, false, FRAME);
    let stall = REPEAT_DELAY + REPEAT_INTERVAL * 10;
    assert_eq!(
        frame(&mut edit, &[KEY_DELETE], HELD, false, stall),
        [InputEvent::Delete]
    );
}

/// Text events for one frame. Keys in `down` are held; `pressed` keys
/// went down this frame (and are held too).
fn text_frame(
    text: &mut TextKeys,
    chars: &str,
    pressed: &[KeyboardKey],
    down: &[KeyboardKey],
    elapsed: Duration,
) -> Vec<InputEvent> {
    let chars: Vec<char> = chars.chars().collect();
    let mut out = Vec::new();
    text.events(
        &chars,
        pressed,
        |key| pressed.contains(&key) || down.contains(&key),
        elapsed,
        &mut out,
    );
    out
}

fn texts(s: &str) -> Vec<InputEvent> {
    s.chars().map(InputEvent::Text).collect()
}

#[test]
fn typed_characters_become_text_events() {
    let mut text = TextKeys::default();
    assert_eq!(
        text_frame(&mut text, "hé", &[KEY_H, KEY_E], &[], FRAME),
        texts("hé")
    );
}

#[test]
fn held_character_repeats_after_delay() {
    let mut text = TextKeys::default();
    assert_eq!(text_frame(&mut text, "o", &[KEY_O], &[], FRAME), texts("o"));
    let just_before = REPEAT_DELAY - Duration::from_millis(1);
    assert_eq!(text_frame(&mut text, "", &[], &[KEY_O], just_before), []);
    assert_eq!(
        text_frame(&mut text, "", &[], &[KEY_O], Duration::from_millis(1)),
        texts("o")
    );
    assert_eq!(
        text_frame(&mut text, "", &[], &[KEY_O], REPEAT_INTERVAL),
        texts("o")
    );
}

#[test]
fn shifted_character_repeats_as_typed() {
    let mut text = TextKeys::default();
    text_frame(&mut text, "O", &[KEY_LEFT_SHIFT, KEY_O], &[], FRAME);
    assert_eq!(
        text_frame(&mut text, "", &[], &[KEY_O], REPEAT_DELAY),
        texts("O")
    );
}

#[test]
fn releasing_the_key_stops_the_repeat() {
    let mut text = TextKeys::default();
    text_frame(&mut text, "h", &[KEY_H], &[], FRAME);
    assert_eq!(text_frame(&mut text, "", &[], &[], REPEAT_DELAY), []);
    assert_eq!(text_frame(&mut text, "", &[], &[KEY_H], REPEAT_DELAY), []);
}

#[test]
fn new_key_replaces_the_repeating_character() {
    let mut text = TextKeys::default();
    text_frame(&mut text, "a", &[KEY_A], &[], FRAME);
    text_frame(&mut text, "b", &[KEY_B], &[KEY_A], FRAME);
    assert_eq!(
        text_frame(&mut text, "", &[], &[KEY_A, KEY_B], REPEAT_DELAY),
        texts("b")
    );
}

#[test]
fn platform_repeats_of_held_character_are_dropped() {
    let mut text = TextKeys::default();
    text_frame(&mut text, "o", &[KEY_O], &[], FRAME);
    assert_eq!(text_frame(&mut text, "o", &[], &[KEY_O], FRAME), []);
    assert_eq!(
        text_frame(&mut text, "oo", &[], &[KEY_O], REPEAT_DELAY),
        texts("o")
    );
}

#[test]
fn characters_without_a_key_press_still_type() {
    let mut text = TextKeys::default();
    assert_eq!(text_frame(&mut text, "ñ", &[], &[], FRAME), texts("ñ"));
    assert_eq!(text_frame(&mut text, "", &[], &[], REPEAT_DELAY * 2), []);
}

#[test]
fn key_press_without_text_does_not_repeat() {
    let mut text = TextKeys::default();
    text_frame(&mut text, "x", &[KEY_X], &[], FRAME);
    // Ctrl+O produces no character, and ends the previous repeat.
    assert_eq!(
        text_frame(&mut text, "", &[KEY_LEFT_CONTROL, KEY_O], &[KEY_X], FRAME),
        []
    );
    assert_eq!(
        text_frame(&mut text, "", &[], &[KEY_X, KEY_O], REPEAT_DELAY),
        []
    );
}

#[test]
fn editing_key_press_ends_text_repeat() {
    let mut text = TextKeys::default();
    text_frame(&mut text, "a", &[KEY_A], &[], FRAME);
    assert_eq!(
        text_frame(&mut text, "", &[KEY_BACKSPACE], &[KEY_A], FRAME),
        []
    );
    assert_eq!(
        text_frame(&mut text, "", &[], &[KEY_A, KEY_BACKSPACE], REPEAT_DELAY),
        []
    );
}

#[test]
fn modifier_press_keeps_text_repeat() {
    let mut text = TextKeys::default();
    text_frame(&mut text, "a", &[KEY_A], &[], FRAME);
    text_frame(&mut text, "", &[KEY_LEFT_SHIFT], &[KEY_A], FRAME);
    assert_eq!(
        text_frame(&mut text, "", &[], &[KEY_A, KEY_LEFT_SHIFT], REPEAT_DELAY),
        texts("a")
    );
}

#[test]
fn release_resets_repeat_timer() {
    let mut edit = EditKeys::default();
    let key = &[KEY_RIGHT];
    frame(&mut edit, key, PRESSED, false, FRAME);
    frame(&mut edit, key, HELD, false, REPEAT_DELAY / 2);
    frame(&mut edit, &[], PRESSED, false, FRAME);

    // Holding the key again without a new press does nothing, because
    // the release cleared the timer.
    assert_eq!(frame(&mut edit, key, HELD, false, REPEAT_DELAY), []);

    frame(&mut edit, key, PRESSED, false, FRAME);
    assert_eq!(
        frame(&mut edit, key, HELD, false, REPEAT_DELAY / 2),
        [],
        "a new press restarts the delay"
    );
}

/// Game events for one frame in which `queued` keys went down and
/// `down` keys are held.
fn game_frame(
    game: &mut GameKeys,
    queued: &[KeyboardKey],
    down: &[KeyboardKey],
) -> Vec<InputEvent> {
    let mut out = Vec::new();
    game.events(queued, |key| down.contains(&key), &mut out);
    out
}

/// Only the button events of `game_frame`.
fn button_frame(
    game: &mut GameKeys,
    queued: &[KeyboardKey],
    down: &[KeyboardKey],
) -> Vec<InputEvent> {
    let mut events = game_frame(game, queued, down);
    events.retain(|e| matches!(e, InputEvent::ButtonDown(_) | InputEvent::ButtonUp(_)));
    events
}

/// Only the key events of `game_frame`.
fn key_frame(game: &mut GameKeys, queued: &[KeyboardKey], down: &[KeyboardKey]) -> Vec<InputEvent> {
    let mut events = game_frame(game, queued, down);
    events.retain(|e| matches!(e, InputEvent::KeyDown(_) | InputEvent::KeyUp(_)));
    events
}

/// The documented desktop bindings, spelled out independently of
/// `GAME_KEYS` so a change to the table fails a test.
const DESKTOP_BINDINGS: [(KeyboardKey, Button); 6] = [
    (KEY_UP, Button::Up),
    (KEY_DOWN, Button::Down),
    (KEY_LEFT, Button::Left),
    (KEY_RIGHT, Button::Right),
    (KEY_Z, Button::A),
    (KEY_X, Button::B),
];

#[test]
fn every_virtual_button_has_exactly_one_key() {
    for button in Button::ALL {
        let keys = GAME_KEYS.iter().filter(|(_, b)| *b == button).count();
        assert_eq!(keys, 1, "{button:?}");
    }
}

#[test]
fn each_binding_presses_and_releases_its_button_in_game_mode() {
    for (key, button) in DESKTOP_BINDINGS {
        let mut game = GameKeys::default();
        let mut down = button_frame(&mut game, &[key], &[key]);
        retain_for_mode(InputMode::Game, &mut down);
        assert_eq!(down, [InputEvent::ButtonDown(button)], "{key:?} down");

        let mut up = button_frame(&mut game, &[], &[]);
        retain_for_mode(InputMode::Game, &mut up);
        assert_eq!(up, [InputEvent::ButtonUp(button)], "{key:?} up");
    }
}

#[test]
fn escape_and_other_keys_are_not_game_buttons() {
    let mut game = GameKeys::default();
    let keys = [KEY_ESCAPE, KEY_ENTER, KEY_A, KEY_SPACE];
    assert_eq!(button_frame(&mut game, &keys, &keys), []);
}

#[test]
fn every_glaux_key_has_exactly_one_raylib_key() {
    for &key in Key::ALL {
        let codes = KEY_CODES.iter().filter(|(_, k)| *k == key).count();
        assert_eq!(codes, 1, "{key:?}");
    }
    let mut codes: Vec<KeyboardKey> = KEY_CODES.iter().map(|&(code, _)| code).collect();
    codes.sort_by_key(|&code| code as i32);
    codes.dedup();
    assert_eq!(codes.len(), KEY_CODES.len(), "a raylib key is mapped twice");
}

#[test]
fn representative_keys_map_to_glaux_keys() {
    for (code, key) in [
        (KEY_F2, Key::F2),
        (KEY_INSERT, Key::Insert),
        (KEY_ZERO, Key::Num0),
        (KEY_NINE, Key::Num9),
        (KEY_LEFT_SHIFT, Key::LeftShift),
        (KEY_PAGE_DOWN, Key::PageDown),
        (KEY_A, Key::A),
        (KEY_Z, Key::Z),
    ] {
        let mut game = GameKeys::default();
        assert_eq!(
            key_frame(&mut game, &[code], &[code]),
            [InputEvent::KeyDown(key)],
            "{code:?}"
        );
        assert_eq!(
            key_frame(&mut game, &[], &[]),
            [InputEvent::KeyUp(key)],
            "{code:?}"
        );
    }
}

#[test]
fn escape_and_keypad_enter_are_not_keys() {
    let mut game = GameKeys::default();
    let keys = [KEY_ESCAPE, KEY_KP_ENTER];
    assert_eq!(game_frame(&mut game, &keys, &keys), []);
}

#[test]
fn a_bound_key_reports_both_its_button_and_its_key() {
    let mut game = GameKeys::default();
    assert_eq!(
        game_frame(&mut game, &[KEY_Z], &[KEY_Z]),
        [
            InputEvent::ButtonDown(Button::A),
            InputEvent::KeyDown(Key::Z),
        ]
    );
}

#[test]
fn a_held_key_presses_once_and_releases_once() {
    let mut game = GameKeys::default();
    assert_eq!(
        game_frame(&mut game, &[KEY_UP], &[KEY_UP]),
        [
            InputEvent::ButtonDown(Button::Up),
            InputEvent::KeyDown(Key::Up),
        ]
    );
    for _ in 0..120 {
        assert_eq!(game_frame(&mut game, &[], &[KEY_UP]), []);
    }
    // A platform repeat reported as another press is not a new press.
    assert_eq!(game_frame(&mut game, &[KEY_UP], &[KEY_UP]), []);
    assert_eq!(
        game_frame(&mut game, &[], &[]),
        [InputEvent::ButtonUp(Button::Up), InputEvent::KeyUp(Key::Up),]
    );
    assert_eq!(game_frame(&mut game, &[], &[]), []);
}

#[test]
fn platform_repeats_of_an_unbound_key_do_not_press_again() {
    let mut game = GameKeys::default();
    let pressed = InputEvent::KeyDown(Key::F2);
    assert_eq!(game_frame(&mut game, &[KEY_F2], &[KEY_F2]), [pressed]);
    for _ in 0..60 {
        assert_eq!(game_frame(&mut game, &[KEY_F2], &[KEY_F2]), []);
    }
}

#[test]
fn a_tap_within_one_frame_is_not_lost() {
    let mut game = GameKeys::default();
    assert_eq!(
        game_frame(&mut game, &[KEY_Z], &[]),
        [
            InputEvent::ButtonDown(Button::A),
            InputEvent::ButtonUp(Button::A),
            InputEvent::KeyDown(Key::Z),
            InputEvent::KeyUp(Key::Z),
        ]
    );
}

#[test]
fn a_key_held_when_tracking_starts_presses_once() {
    let mut game = GameKeys::default();
    assert_eq!(
        game_frame(&mut game, &[], &[KEY_X]),
        [
            InputEvent::ButtonDown(Button::B),
            InputEvent::KeyDown(Key::X),
        ]
    );
    assert_eq!(game_frame(&mut game, &[], &[KEY_X]), []);
}

#[test]
fn simultaneous_keys_each_report_their_own_transition() {
    let mut game = GameKeys::default();
    let keys = [KEY_LEFT_SHIFT, KEY_F1, KEY_SPACE];
    assert_eq!(
        key_frame(&mut game, &keys, &keys),
        [
            InputEvent::KeyDown(Key::F1),
            InputEvent::KeyDown(Key::Space),
            InputEvent::KeyDown(Key::LeftShift),
        ]
    );
    assert_eq!(
        key_frame(&mut game, &[], &[KEY_LEFT_SHIFT, KEY_SPACE]),
        [InputEvent::KeyUp(Key::F1)]
    );
}

/// Every event one frame of `keys` going down produces in `mode`.
fn translated(mode: InputMode, keys: &[KeyboardKey], chars: &str) -> Vec<InputEvent> {
    let chars: Vec<char> = chars.chars().collect();
    let mut out = Vec::new();
    TextKeys::default().events(&chars, keys, |k| keys.contains(&k), FRAME, &mut out);
    EditKeys::default().events(
        |k| {
            if keys.contains(&k) {
                PRESSED
            } else {
                KeyState::default()
            }
        },
        Modifiers::default(),
        FRAME,
        &mut out,
    );
    GameKeys::default().events(keys, |k| keys.contains(&k), &mut out);
    retain_for_mode(mode, &mut out);
    out
}

#[test]
fn text_mode_keeps_editing_and_drops_buttons_and_keys() {
    assert_eq!(
        translated(InputMode::Text, &[KEY_LEFT, KEY_UP], ""),
        [
            InputEvent::Left { select: false },
            InputEvent::Up { select: false }
        ]
    );
    assert_eq!(translated(InputMode::Text, &[KEY_Z], "z"), texts("z"));
    assert_eq!(translated(InputMode::Text, &[KEY_F2], ""), []);
}

#[test]
fn game_mode_keeps_buttons_and_keys_and_drops_text_and_editing() {
    assert_eq!(
        translated(InputMode::Game, &[KEY_LEFT, KEY_UP], ""),
        [
            InputEvent::ButtonDown(Button::Up),
            InputEvent::ButtonDown(Button::Left),
            InputEvent::KeyDown(Key::Up),
            InputEvent::KeyDown(Key::Left),
        ]
    );
    assert_eq!(
        translated(InputMode::Game, &[KEY_Z, KEY_X], "zx"),
        [
            InputEvent::ButtonDown(Button::A),
            InputEvent::ButtonDown(Button::B),
            InputEvent::KeyDown(Key::X),
            InputEvent::KeyDown(Key::Z),
        ]
    );
}

#[test]
fn escape_breaks_without_releasing_held_buttons() {
    assert_eq!(
        translated(InputMode::Game, &[KEY_Z, KEY_ESCAPE], "z"),
        [
            InputEvent::Break,
            InputEvent::ButtonDown(Button::A),
            InputEvent::KeyDown(Key::Z),
        ]
    );
}

#[test]
fn break_reaches_every_mode() {
    for mode in [InputMode::Text, InputMode::Game, InputMode::Workspace] {
        assert_eq!(
            translated(mode, &[KEY_ESCAPE], ""),
            [InputEvent::Break],
            "{mode:?}"
        );
    }
}
