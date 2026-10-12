use super::*;
use pretty_assertions::assert_eq;

fn listing(paths: &[&str], gaps: Vec<WalkGap>) -> ScriptListing {
    ScriptListing {
        paths: paths
            .iter()
            .map(|path| SourcePath::parse(path).unwrap())
            .collect(),
        gaps,
    }
}

const FILES: [&str; 5] = [
    "code/enemies/bat.luau",
    "code/main.luau",
    "code/ui/hud.rhai",
    "code/ui/menu.luau",
    "lib/enemy_ai.luau",
];

fn picker() -> FilePicker {
    FilePicker::new(Ok(listing(&FILES, Vec::new())))
}

fn layout() -> PickerLayout {
    PickerLayout::new(
        Rect {
            x: 0,
            y: 16,
            width: 320,
            height: 212,
        },
        10,
        0,
    )
}

fn type_text(picker: &mut FilePicker, text: &str) {
    for ch in text.chars() {
        assert_eq!(picker.handle_input(InputEvent::Text(ch), &layout()), None);
    }
}

fn shown(picker: &FilePicker) -> Vec<&str> {
    picker.matches().map(SourcePath::as_str).collect()
}

#[test]
fn without_a_filter_every_file_shows_in_listing_order() {
    assert_eq!(shown(&picker()), FILES);
}

#[test]
fn the_filter_matches_names_and_paths_ignoring_case() {
    let mut picker = picker();
    type_text(&mut picker, "UI/");
    assert_eq!(shown(&picker), ["code/ui/hud.rhai", "code/ui/menu.luau"]);

    let mut picker = self::picker();
    type_text(&mut picker, "hud");
    assert_eq!(shown(&picker), ["code/ui/hud.rhai"]);
}

#[test]
fn name_matches_rank_before_path_matches() {
    let mut picker = picker();
    // `enemies/` matches only through the path; `enemy_ai` starts with
    // the filter; `menu` holds it in its name.
    type_text(&mut picker, "en");
    assert_eq!(
        shown(&picker),
        [
            "lib/enemy_ai.luau",
            "code/ui/menu.luau",
            "code/enemies/bat.luau"
        ]
    );
    // The same filter always gives the same order.
    let mut again = self::picker();
    type_text(&mut again, "en");
    assert_eq!(shown(&again), shown(&picker));
}

#[test]
fn backspace_widens_the_filter_again() {
    let mut picker = picker();
    type_text(&mut picker, "hudx");
    assert_eq!(picker.match_count(), 0);
    picker.handle_input(InputEvent::Backspace, &layout());
    assert_eq!(picker.filter(), "hud");
    assert_eq!(picker.match_count(), 1);
}

#[test]
fn enter_opens_the_highlighted_file() {
    let mut picker = picker();
    picker.handle_input(InputEvent::Down, &layout());
    picker.handle_input(InputEvent::Down, &layout());
    assert_eq!(
        picker.handle_input(InputEvent::Enter, &layout()),
        Some(PickerAction::Open(
            SourcePath::parse("code/ui/hud.rhai").unwrap()
        ))
    );

    // Filtering highlights the best match again.
    type_text(&mut picker, "menu");
    assert_eq!(
        picker.handle_input(InputEvent::Enter, &layout()),
        Some(PickerAction::Open(
            SourcePath::parse("code/ui/menu.luau").unwrap()
        ))
    );
}

#[test]
fn nothing_opens_without_a_match() {
    let mut picker = picker();
    type_text(&mut picker, "zzz");
    assert_eq!(picker.handle_input(InputEvent::Enter, &layout()), None);
    assert_eq!(picker.handle_input(InputEvent::Down, &layout()), None);
    assert_eq!(
        picker.handle_input(InputEvent::Break, &layout()),
        Some(PickerAction::Close)
    );
}

#[test]
fn the_highlight_stays_on_the_list_and_scrolls_it() {
    let paths: Vec<String> = (0..25).map(|index| format!("f{index:02}.luau")).collect();
    let refs: Vec<&str> = paths.iter().map(String::as_str).collect();
    let mut picker = FilePicker::new(Ok(listing(&refs, Vec::new())));
    picker.handle_input(InputEvent::Up, &layout());
    assert_eq!((picker.focus(), picker.first()), (0, 0));
    for _ in 0..LIST_ROWS {
        picker.handle_input(InputEvent::Down, &layout());
    }
    assert_eq!((picker.focus(), picker.first()), (LIST_ROWS, 1));
    picker.handle_input(InputEvent::Wheel { delta: -100 }, &layout());
    assert_eq!((picker.focus(), picker.first()), (24, 24 + 1 - LIST_ROWS));
}

#[test]
fn clicking_a_row_opens_it_and_clicking_outside_closes() {
    let mut picker = picker();
    let row = layout().row(1);
    let at = Point::new((row.x + 2) as u16, (row.y + 2) as u16);
    let press = |at| InputEvent::PointerDown {
        button: PointerButton::Primary,
        at,
    };
    assert_eq!(
        picker.handle_input(press(at), &layout()),
        Some(PickerAction::Open(
            SourcePath::parse("code/main.luau").unwrap()
        ))
    );
    assert_eq!(
        picker.handle_input(press(Point::new(0, 20)), &layout()),
        Some(PickerAction::Close)
    );
}

#[test]
fn skipped_parts_of_the_cartridge_are_reported() {
    let picker = FilePicker::new(Ok(listing(
        &FILES,
        vec![WalkGap::Unreadable("/code/locked".into())],
    )));
    assert_eq!(picker.gap_lines(), ["Cannot read folder code/locked"]);

    let picker = FilePicker::new(Ok(listing(
        &FILES,
        vec![
            WalkGap::TooDeep("/a".into()),
            WalkGap::AlreadyWalked("/b".into()),
            WalkGap::FileLimit(1024),
        ],
    )));
    assert_eq!(
        picker.gap_lines(),
        [
            "Too deep, not searched: a",
            "...and 2 more places not searched"
        ]
    );
}

#[test]
fn a_listing_that_failed_shows_why() {
    let picker = FilePicker::new(Err(FsError::Unreadable("/".into())));
    assert_eq!(picker.error(), Some("Cannot read /"));
    assert_eq!(picker.match_count(), 0);
}
