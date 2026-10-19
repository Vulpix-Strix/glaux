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
    let (tabs, current) = workspace::tests_support::docs_pages(&system.workspace);
    assert_eq!(tabs, 1);
    assert_eq!(current.as_deref(), Some("guide/index"));

    // Escape closes the browser, and only then does it leave the
    // workspace.
    let system = press_break(system);
    assert!(!docs_open(&system));
    assert!(is_workspace(&system));
    let system = press_break(system);
    assert!(!is_workspace(&system));
}

#[test]
fn f1_again_hides_the_browser() {
    let fixture = Fixture::new("sys-docs-f1-twice");
    let system = in_code(&fixture).update(&[InputEvent::Docs, InputEvent::Docs]);
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
