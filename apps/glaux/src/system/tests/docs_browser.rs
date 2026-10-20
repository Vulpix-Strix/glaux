use glaux_docs::Library;
use glaux_i18n::FallbackChain;

use super::*;
use pretty_assertions::assert_eq;

fn text_of(system: &System) -> String {
    let code = system.code.as_ref().unwrap();
    code.active_document().unwrap().text().to_owned()
}

fn typed(system: System, text: &str) -> System {
    let events: Vec<InputEvent> = text.chars().map(InputEvent::Text).collect();
    system.update(&events)
}

/// The workspace on its Code view with `-- main` open.
fn in_code(fixture: &Fixture) -> System {
    let system = press_break(enter(
        shell_with_cartridge(fixture, "-- main\n"),
        "load pong",
    ));
    assert_eq!(workspace_tab(&system), Tab::Code);
    system
}

fn docs_open(system: &System) -> bool {
    workspace::tests_support::docs_open(&system.workspace)
}

fn docs_button() -> Point {
    let rect = workspace::tests_support::docs_button(fake_rom().font());
    Point::new(
        (rect.x + rect.width / 2) as u16,
        (rect.y + rect.height / 2) as u16,
    )
}

#[test]
fn f1_opens_the_browser_over_the_code_view_and_escape_closes_it() {
    let fixture = Fixture::new("sys-docs-f1");
    let system = in_code(&fixture);
    let system = system.update(&[InputEvent::Docs]);
    assert!(docs_open(&system));
    assert_eq!(workspace_tab(&system), Tab::Code);
    // Nothing under the caret to look up, so the search is what opens.
    assert!(workspace::tests_support::docs_searching(&system.workspace));
    let (tabs, current) = workspace::tests_support::docs_pages(&system.workspace);
    assert_eq!(tabs, 1);
    assert_eq!(current.as_deref(), Some("guide/index"));

    // Escape closes the search, then the browser, and only then does it
    // leave the workspace.
    let system = press_break(system);
    assert!(docs_open(&system));
    let system = press_break(system);
    assert!(!docs_open(&system));
    assert!(is_workspace(&system));
    let system = press_break(system);
    assert!(!is_workspace(&system));
}

#[test]
fn f1_again_hides_the_browser() {
    let fixture = Fixture::new("sys-docs-f1-twice-hide");
    let system = in_code(&fixture).update(&[InputEvent::Docs, InputEvent::Break, InputEvent::Docs]);
    assert!(!docs_open(&system));
    assert!(is_workspace(&system));
}

#[test]
fn the_browser_never_touches_the_open_documents() {
    let fixture = Fixture::new("sys-docs-documents");
    let system = typed(in_code(&fixture), "abc");
    let code = system.code.as_ref().unwrap();
    let id = code.active_document().unwrap().id();
    let revision = code.active_document().unwrap().revision();
    assert!(code.is_modified() && code.can_undo(id));

    // Read, search, follow links, scroll, switch tabs and filter.
    let mut events = vec![InputEvent::Docs, InputEvent::Find];
    events.extend("game".chars().map(InputEvent::Text));
    events.extend([
        InputEvent::Enter,
        InputEvent::Tab { reverse: false },
        InputEvent::Enter,
        InputEvent::PageDown { select: false },
        InputEvent::Text('l'),
        InputEvent::Back,
        InputEvent::ToggleSidebar,
        InputEvent::Find,
        InputEvent::Text('b'),
        InputEvent::Enter,
        InputEvent::CloseDocument,
    ]);
    let system = system.update(&events);
    // Closing the last tab hides the browser too.
    let system = if docs_open(&system) {
        press_break(system)
    } else {
        system
    };
    assert!(!docs_open(&system));

    let code = system.code.as_ref().unwrap();
    let document = code.active_document().unwrap();
    assert_eq!(document.text(), "abc-- main\n");
    assert_eq!(document.revision(), revision);
    assert!(code.is_modified() && code.can_undo(id));

    // The caret is still after `abc`, and undo still has the typing.
    let system = typed(system, "d");
    assert_eq!(text_of(&system), "abcd-- main\n");
    let system = system.update(&[InputEvent::Undo, InputEvent::Undo]);
    assert_eq!(text_of(&system), "-- main\n");
    assert!(!system.code.as_ref().unwrap().is_modified());
}

#[test]
fn typing_in_the_browser_does_not_reach_the_script() {
    let fixture = Fixture::new("sys-docs-typing");
    let system = in_code(&fixture).update(&[InputEvent::Docs]);
    let system = typed(system, "xyz");
    let system = system.update(&[InputEvent::Backspace, InputEvent::Enter, InputEvent::Undo]);
    assert_eq!(text_of(&system), "-- main\n");
    assert!(!system.code.as_ref().unwrap().is_modified());
}

#[test]
fn the_docs_button_opens_and_closes_the_browser_without_changing_the_view() {
    let fixture = Fixture::new("sys-docs-button");
    let system = in_code(&fixture);
    let system = system.update(&click(docs_button()));
    assert!(docs_open(&system));
    assert_eq!(workspace_tab(&system), Tab::Code);
    let system = system.update(&click(docs_button()));
    assert!(!docs_open(&system));
    assert_eq!(workspace_tab(&system), Tab::Code);
}

#[test]
fn the_docs_button_works_from_the_sprite_view_and_leaves_it_selected() {
    let fixture = Fixture::new("sys-docs-sprites");
    let system = in_code(&fixture).update(&click(tab_centre(Tab::Sprites)));
    assert_eq!(workspace_tab(&system), Tab::Sprites);
    let system = system.update(&click(docs_button()));
    assert!(docs_open(&system));
    assert_eq!(workspace_tab(&system), Tab::Sprites);
    let system = press_break(system);
    assert_eq!(workspace_tab(&system), Tab::Sprites);
    assert!(is_workspace(&system));
}

#[test]
fn switching_views_hides_the_browser_and_it_comes_back_as_it_was() {
    let fixture = Fixture::new("sys-docs-switch");
    let system = in_code(&fixture).update(&[InputEvent::Docs, InputEvent::Find]);
    let system = system.update(&[
        InputEvent::Text('g'),
        InputEvent::Text('a'),
        InputEvent::Text('m'),
        InputEvent::Enter,
    ]);
    let pages = workspace::tests_support::docs_pages(&system.workspace);
    assert_eq!(pages.1.as_deref(), Some("guide/game-loop"));

    let system = system.update(&[InputEvent::NextView]);
    assert!(!docs_open(&system));
    assert_eq!(workspace_tab(&system), Tab::Sprites);
    let system = system.update(&click(docs_button()));
    assert!(docs_open(&system));
    assert_eq!(
        workspace::tests_support::docs_pages(&system.workspace),
        pages
    );

    // A press on a view's tab hides it too.
    let system = system.update(&click(tab_centre(Tab::Code)));
    assert!(!docs_open(&system));
    assert_eq!(workspace_tab(&system), Tab::Code);
}

#[test]
fn f1_in_the_shell_does_nothing() {
    let fixture = Fixture::new("sys-docs-shell");
    let system = loaded(&fixture);
    let before = transcript(&system);
    let system = system.update(&[InputEvent::Docs, InputEvent::Back, InputEvent::Forward]);
    assert_eq!(transcript(&system), before);
    assert!(!is_workspace(&system));
}

/// Makes the browser read two small pages on this thread: the front page,
/// which links outward, and one that links to a blocked and a missing page.
fn use_linking_docs() {
    let files: [(&str, &str); 2] = [
        (
            "en/guide/index.md",
            "# Guide\n\nSee [site](https://example.com/page) and [more](glaux://guide/more).\n",
        ),
        (
            "en/guide/more.md",
            "# More\n\n[local](file:///etc/passwd) [gone](glaux://guide/gone) [mail](mailto:me@example.com)\n",
        ),
    ];
    let library = Library::from_files(
        files
            .iter()
            .map(|(path, text)| ((*path).to_owned(), text.as_bytes().to_vec())),
    );
    workspace::tests_support::use_docs(library, FallbackChain::english());
}

#[test]
fn an_allowed_link_becomes_a_request_for_the_host_and_is_announced() {
    use_linking_docs();
    let fixture = Fixture::new("sys-docs-external");
    let mut system = in_code(&fixture).update(&[
        InputEvent::Docs,
        InputEvent::Break,
        InputEvent::Tab { reverse: false },
        InputEvent::Enter,
    ]);
    let url = ExternalUrl::parse("https://example.com/page").unwrap();
    assert_eq!(
        system.take_host_request(),
        Some(HostRequest::OpenExternal(url))
    );
    assert_eq!(system.take_host_request(), None);
    assert_eq!(
        workspace::tests_support::notice(&system.workspace),
        Some("Opening https://example.com/page")
    );
    // The browser stays where it was.
    assert!(docs_open(&system));
}

#[test]
fn blocked_and_broken_links_report_and_ask_nothing_of_the_host() {
    use_linking_docs();
    let fixture = Fixture::new("sys-docs-blocked");
    let mut system = in_code(&fixture).update(&[
        InputEvent::Docs,
        InputEvent::Break,
        InputEvent::Tab { reverse: false },
        InputEvent::Tab { reverse: false },
        InputEvent::Enter,
    ]);
    assert_eq!(
        workspace::tests_support::docs_pages(&system.workspace)
            .1
            .as_deref(),
        Some("guide/more")
    );
    system = system.update(&[InputEvent::Tab { reverse: false }, InputEvent::Enter]);
    assert_eq!(
        workspace::tests_support::notice(&system.workspace),
        Some("Blocked link: file")
    );
    system = system.update(&[InputEvent::Tab { reverse: false }, InputEvent::Enter]);
    assert_eq!(
        workspace::tests_support::notice(&system.workspace),
        Some("Broken link: glaux://guide/gone")
    );
    assert_eq!(system.take_host_request(), None);
    // Mail is allowed.
    let mut system = system.update(&[InputEvent::Tab { reverse: false }, InputEvent::Enter]);
    assert_eq!(
        system.take_host_request(),
        Some(HostRequest::OpenExternal(
            ExternalUrl::parse("mailto:me@example.com").unwrap()
        ))
    );
}

#[test]
fn the_browser_draws_in_the_workspace_frame() {
    let fixture = Fixture::new("sys-docs-render");
    let closed = render(&in_code(&fixture));
    let open = render(&in_code(&fixture).update(&[InputEvent::Docs]));
    // The tab bar is the same, and the view below it is not.
    let bar = workspace::tests_support::tab_rect(Tab::Code, fake_rom().font().metrics());
    let below = (bar.height..HEIGHT as i32)
        .flat_map(|y| (0..WIDTH as i32).map(move |x| (x, y)))
        .filter(|&(x, y)| closed.pixel(x, y) != open.pixel(x, y))
        .count();
    assert!(below > 500, "{below} pixels differ");
    for x in 0..bar.x + bar.width {
        assert_eq!(closed.pixel(x, 2), open.pixel(x, 2), "x = {x}");
    }
}

/// The point on the first letter of the Docs button's label, where the fake
/// font draws a solid block.
fn docs_label_pixel() -> (i32, i32) {
    let rect = workspace::tests_support::docs_button(fake_rom().font());
    let (_, y) = workspace::tests_support::hint_origin(fake_rom().font().metrics());
    (rect.x + 3, y)
}

fn presented_at(system: &System, pointer: Point) -> Framebuffer {
    let rom = fake_rom();
    let mut fb = Framebuffer::new();
    system.render(&mut fb, &rom, Some(pointer));
    system.draw_cursor(&mut fb, &rom, Some(pointer));
    fb
}

#[test]
fn the_button_is_dim_at_rest_pale_under_the_pointer_and_accented_while_open() {
    let fixture = Fixture::new("sys-docs-button-states");
    let system = in_code(&fixture);
    let (x, y) = docs_label_pixel();
    let button = workspace::tests_support::docs_button(fake_rom().font());

    let rest = render(&system);
    assert_eq!(rest.pixel(x, y), Some(DIM), "rest");
    let hover = presented_at(&system, Point::new((button.x + 1) as u16, 1));
    assert_eq!(hover.pixel(x, y), Some(PALE), "hover");

    let open = system.update(&[InputEvent::Docs]);
    let frame = render(&open);
    assert_eq!(frame.pixel(x, y), Some(ACCENT), "open");
    // The plate takes the view's color with an accent line on top, as the
    // active tab does.
    assert_eq!(frame.pixel(button.x, 0), Some(ACCENT));
    assert_eq!(frame.pixel((button.x + button.width) - 1, 0), Some(ACCENT));
    assert_eq!(frame.pixel(button.x, 1), Some(BACKGROUND));
    assert_eq!(frame.pixel(button.x - 1, 0), rest.pixel(button.x - 1, 0));
}

#[test]
fn hovering_the_button_names_it_and_hovering_a_tab_names_the_tab() {
    let fixture = Fixture::new("sys-docs-button-hint");
    let system = in_code(&fixture);
    let metrics = fake_rom().font().metrics();
    let (hx, hy) = workspace::tests_support::hint_origin(metrics);
    let cell = metrics.cell_width() as i32;
    let button = workspace::tests_support::docs_button(fake_rom().font());

    let idle = render(&system);
    let over_button = presented_at(&system, Point::new((button.x + 1) as u16, 1));
    let over_tab = presented_at(&system, tab_centre(Tab::Map));

    // The button's hint is "Docs (F1)": nine blocks, one more than "Sprites".
    assert_eq!(over_button.pixel(hx, hy), Some(PALE));
    assert_eq!(over_button.pixel(hx + 8 * cell, hy), Some(PALE));
    assert_eq!(
        over_button.pixel(hx + 9 * cell, hy),
        idle.pixel(hx + 9 * cell, hy)
    );
    // Hovering either one never draws over the other's place.
    for x in button.x..(button.x + button.width) {
        for y in 0..button.height {
            if (x, y) == (button.x + 1, 1) {
                continue;
            }
            let moved = over_tab.pixel(x, y) != idle.pixel(x, y);
            assert!(!moved, "the tab hint reached the button at ({x}, {y})");
        }
    }
    let name_start = workspace::tests_support::name_start(fake_rom().font());
    for x in name_start..button.x {
        for y in 0..button.height {
            assert_eq!(over_button.pixel(x, y), idle.pixel(x, y), "({x}, {y})");
        }
    }
}

#[test]
fn a_long_cartridge_name_stops_before_the_docs_button() {
    let fixture = Fixture::new("sys-docs-button-name");
    let dir = fixture.root().join("long");
    write_cartridge(&dir, &"W".repeat(120));
    let long = render(&press_break(enter(
        shell_with_cartridge(&fixture, ""),
        "load long",
    )));
    let short = render(&in_code(&Fixture::new("sys-docs-button-name-short")));
    let button = workspace::tests_support::docs_button(fake_rom().font());
    for x in button.x - 4..WIDTH as i32 {
        for y in 0..button.height {
            assert_eq!(long.pixel(x, y), short.pixel(x, y), "({x}, {y})");
        }
    }
    // The truncated name does reach as far as the gap before the button.
    let last = (0..button.x - 4)
        .rev()
        .find(|&x| (0..button.height).any(|y| long.pixel(x, y) == Some(PALE)))
        .unwrap();
    assert!(last >= button.x - 4 - 2 * CELL_W, "{last}");
}

/// The workspace on its Code view with `script` open, caret at the start.
fn in_code_with(fixture: &Fixture, script: &str) -> System {
    let system = press_break(enter(shell_with_cartridge(fixture, script), "load pong"));
    assert_eq!(workspace_tab(&system), Tab::Code);
    system
}

fn location(system: &System) -> Option<(String, Option<String>)> {
    workspace::tests_support::docs_location(&system.workspace)
}

fn page(id: &str) -> Option<(String, Option<String>)> {
    Some((id.to_owned(), None))
}

/// Everything the browser must leave alone: the text, whether it is
/// modified, the revision and what undo has.
fn editor_state(system: &System) -> (String, bool, u64, bool) {
    let code = system.code.as_ref().unwrap();
    let document = code.active_document().unwrap();
    (
        document.text().to_owned(),
        code.is_modified(),
        document.revision(),
        code.can_undo(document.id()),
    )
}

#[test]
fn f1_in_a_call_opens_the_function_at_its_parameters_and_changes_nothing() {
    let fixture = Fixture::new("sys-docs-f1-signature");
    let system = typed(in_code_with(&fixture, ""), "spr(1, ");
    let before = editor_state(&system);
    assert_eq!(before.0, "spr(1, ");
    assert!(before.1 && before.3);
    let system = system.update(&[InputEvent::Docs]);

    assert!(docs_open(&system));
    assert_eq!(
        location(&system),
        Some(("api/graphics/spr".to_owned(), Some("parameters".to_owned())))
    );
    assert_eq!(editor_state(&system), before);

    // Back in the editor the popup is gone, the caret is where it was and
    // typing carries on from there, with undo still holding it all.
    let system = press_break(system);
    assert!(!docs_open(&system));
    let system = typed(system, "2)");
    assert_eq!(text_of(&system), "spr(1, 2)");
    let system = system.update(&[InputEvent::Undo, InputEvent::Undo, InputEvent::Undo]);
    assert!(text_of(&system).len() < "spr(1, 2)".len());
}

#[test]
fn f1_with_suggestions_open_shows_the_one_picked() {
    let fixture = Fixture::new("sys-docs-f1-completion");
    let system = typed(in_code_with(&fixture, ""), "pse");
    let before = editor_state(&system);
    let system = system.update(&[InputEvent::Docs]);
    assert_eq!(location(&system), page("api/graphics/pset"));
    assert_eq!(editor_state(&system), before);
    // The list closed: the editor takes Enter as a line break.
    let system = press_break(system);
    let system = system.update(&[InputEvent::Enter]);
    assert_eq!(text_of(&system), "pse\n");
}

#[test]
fn f1_on_a_known_name_opens_its_page() {
    let fixture = Fixture::new("sys-docs-f1-symbol");
    let system = in_code_with(&fixture, "pset(0, 0, 1)\nspr(1, 1, 1)\n");
    let before = editor_state(&system);
    let system = system.update(&[InputEvent::Docs]);
    assert_eq!(location(&system), page("api/graphics/pset"));
    assert_eq!(editor_state(&system), before);
}

#[test]
fn f1_without_a_name_opens_the_search() {
    let fixture = Fixture::new("sys-docs-f1-search");
    let system = in_code_with(&fixture, "-- nothing to look up\n");
    let system = system.update(&[InputEvent::Docs]);
    assert!(docs_open(&system));
    assert!(workspace::tests_support::docs_searching(&system.workspace));
    assert_eq!(editor_state(&system).0, "-- nothing to look up\n");
}

#[test]
fn f1_on_the_scripts_own_function_says_it_has_no_page() {
    let fixture = Fixture::new("sys-docs-f1-local");
    let system = in_code_with(&fixture, "function mine() end\nmine()\n");
    let system = system.update(&[InputEvent::Down { select: false }, InputEvent::Docs]);
    assert!(!docs_open(&system));
    assert_eq!(
        workspace::tests_support::notice(&system.workspace),
        Some("No documentation for mine")
    );
}

#[test]
fn f1_brings_forward_a_page_that_is_already_open() {
    let fixture = Fixture::new("sys-docs-f1-twice");
    let system = in_code_with(&fixture, "pset(0, 0, 1)\nspr(1, 1, 1)\n");
    let system = system.update(&[InputEvent::Docs]);
    let system = press_break(system);
    let system = system.update(&[InputEvent::Down { select: false }, InputEvent::Docs]);
    assert_eq!(location(&system), page("api/graphics/spr"));
    assert_eq!(workspace::tests_support::docs_pages(&system.workspace).0, 2);

    let system = press_break(system);
    let system = system.update(&[InputEvent::Up { select: false }, InputEvent::Docs]);
    assert_eq!(location(&system), page("api/graphics/pset"));
    assert_eq!(
        workspace::tests_support::docs_pages(&system.workspace).0,
        2,
        "the open tab was shown again"
    );
}

#[test]
fn f1_from_the_sprite_view_opens_the_browser_where_it_was() {
    let fixture = Fixture::new("sys-docs-f1-sprites");
    let system = in_code_with(&fixture, "pset(0, 0, 1)\n").update(&[InputEvent::Docs]);
    let system = press_break(system).update(&click(tab_centre(Tab::Sprites)));
    let system = system.update(&[InputEvent::Docs]);
    assert!(docs_open(&system));
    assert_eq!(workspace_tab(&system), Tab::Sprites);
    assert_eq!(location(&system), page("api/graphics/pset"));
}

#[test]
fn opening_and_closing_documentation_from_the_editor_keeps_the_selection() {
    let fixture = Fixture::new("sys-docs-f1-selection");
    let system = in_code_with(&fixture, "pset(0, 0, 1)\n");
    let system = system.update(&[InputEvent::Right { select: true }]);
    let system = system.update(&[InputEvent::Docs]);
    let system = press_break(system);
    // The selection survived: typing replaces the selected `p`.
    let system = typed(system, "x");
    assert_eq!(text_of(&system), "xset(0, 0, 1)\n");
}
