use glaux_docs::Library;
use glaux_i18n::{FallbackChain, Language};
use pretty_assertions::assert_eq;

use super::*;

const AREA: Rect = Rect {
    x: 0,
    y: 14,
    width: 320,
    height: 226,
};

fn library() -> Library {
    let files: [(&str, &str); 7] = [
        (
            "api/graphics.ron",
            "#![enable(implicit_some)]\n[\n (id: \"api/graphics/spr\", kind: Function, luau: (name: \"spr\"), rhai: (name: \"spr\")),\n (id: \"api/graphics/text\", kind: Function, luau: (name: \"print\"), rhai: (name: \"text\")),\n]",
        ),
        (
            "en/guide/index.md",
            "# Guide\n\nStart here.\n\n[Sprites](glaux://guide/sprites)\n",
        ),
        ("en/guide/sprites.md", "# Sprites\n\nTiles on screen.\n"),
        ("en/api/index.md", "# API\n\nEverything.\n"),
        ("en/api/graphics/spr.md", "# spr\n\nDraws tiles.\n"),
        ("en/api/graphics/text.md", "# text\n\nDraws text.\n"),
        ("es/guide/sprites.md", "# Duendes\n\nMosaicos.\n"),
    ];
    Library::from_files(
        files
            .iter()
            .map(|(path, text)| ((*path).to_owned(), text.as_bytes().to_vec())),
    )
}

fn layout() -> SearchLayout {
    SearchLayout::new(AREA, 6, 12, [3, 3, 6])
}

struct Case {
    library: Library,
    chain: FallbackChain,
    search: Search,
}

impl Case {
    fn new() -> Self {
        let library = library();
        let chain = FallbackChain::english();
        let search = Search::new(Source {
            library: &library,
            chain: &chain,
        });
        Self {
            library,
            chain,
            search,
        }
    }

    fn send(&mut self, event: InputEvent) -> Option<SearchAction> {
        self.search.handle_input(
            event,
            &layout(),
            Source {
                library: &self.library,
                chain: &self.chain,
            },
        )
    }

    fn type_text(&mut self, text: &str) {
        for ch in text.chars() {
            self.send(InputEvent::Text(ch));
        }
    }

    fn titles(&self) -> Vec<&str> {
        self.search
            .results()
            .iter()
            .map(|found| found.title.as_str())
            .collect()
    }
}

fn centre(rect: Rect) -> Point {
    Point::new(
        (rect.x + rect.width / 2) as u16,
        (rect.y + rect.height / 2) as u16,
    )
}

#[test]
fn an_empty_query_lists_every_page_in_navigation_order() {
    let case = Case::new();
    assert_eq!(case.titles(), ["Guide", "Sprites", "API", "spr", "text"]);
    assert_eq!(case.search.scope(), Scope::All);
}

#[test]
fn typing_narrows_the_list_to_the_best_matches() {
    let mut case = Case::new();
    case.type_text("spr");
    assert_eq!(case.titles(), ["spr", "Sprites"]);
    case.send(InputEvent::Backspace);
    // A shorter query matches the same pages, the closer name first.
    assert_eq!(case.titles(), ["spr", "Sprites"]);
    case.type_text("zz");
    assert_eq!(case.titles(), Vec::<&str>::new());
}

#[test]
fn a_name_in_one_language_finds_the_page() {
    let mut case = Case::new();
    case.type_text("print");
    assert_eq!(case.titles(), ["text"]);
    assert_eq!(case.search.results()[0].mark, 'f');
    assert_eq!(case.search.results()[0].tag, "graphics");
}

#[test]
fn the_scope_filters_apis_and_guides_without_touching_the_query() {
    let mut case = Case::new();
    case.type_text("s");
    let everything = case.titles().len();
    case.send(InputEvent::Tab { reverse: false });
    assert_eq!(case.search.scope(), Scope::Api);
    assert!(
        case.search
            .results()
            .iter()
            .all(|found| found.id.section() == "api")
    );
    case.send(InputEvent::Tab { reverse: false });
    assert_eq!(case.search.scope(), Scope::Guides);
    assert!(
        case.search
            .results()
            .iter()
            .all(|found| found.id.section() == "guide")
    );
    case.send(InputEvent::Tab { reverse: false });
    assert_eq!(case.search.scope(), Scope::All);
    assert_eq!(case.titles().len(), everything);
    case.send(InputEvent::Tab { reverse: true });
    assert_eq!(case.search.scope(), Scope::Guides);
    assert_eq!(case.search.field().text(), "s");
}

#[test]
fn clicking_a_scope_chip_picks_it() {
    let mut case = Case::new();
    let chip = layout().scopes[1];
    case.send(InputEvent::PointerDown {
        button: PointerButton::Primary,
        at: centre(chip),
        select: false,
    });
    assert_eq!(case.search.scope(), Scope::Api);
}

#[test]
fn marks_say_what_each_page_is() {
    let case = Case::new();
    let marks: Vec<_> = case
        .search
        .results()
        .iter()
        .map(|found| (found.title.as_str(), found.mark, found.tag.as_str()))
        .collect();
    assert_eq!(
        marks,
        [
            ("Guide", 'g', "guide"),
            ("Sprites", 'g', "guide"),
            ("API", 'i', "api"),
            ("spr", 'f', "graphics"),
            ("text", 'f', "graphics"),
        ]
    );
}

#[test]
fn enter_opens_the_highlighted_page_and_arrows_move_the_highlight() {
    let mut case = Case::new();
    case.send(InputEvent::Down { select: false });
    case.send(InputEvent::Down { select: false });
    assert_eq!(
        case.send(InputEvent::Enter),
        Some(SearchAction::Open {
            id: DocId::parse("api/index").unwrap(),
            new_tab: false
        })
    );
    case.send(InputEvent::Up { select: false });
    assert_eq!(case.search.focus(), 1);
    for _ in 0..10 {
        case.send(InputEvent::Down { select: false });
    }
    assert_eq!(case.search.focus(), 4);
    case.send(InputEvent::Up { select: false });
    case.send(InputEvent::Up { select: false });
    case.send(InputEvent::Up { select: false });
    case.send(InputEvent::Up { select: false });
    case.send(InputEvent::Up { select: false });
    assert_eq!(case.search.focus(), 0);
}

#[test]
fn enter_with_nothing_found_does_nothing() {
    let mut case = Case::new();
    case.type_text("zzzz");
    assert_eq!(case.search.results(), []);
    assert_eq!(case.send(InputEvent::Enter), None);
}

#[test]
fn escape_and_a_press_outside_close_the_window() {
    let mut case = Case::new();
    assert_eq!(case.send(InputEvent::Break), Some(SearchAction::Close));
    assert_eq!(
        case.send(InputEvent::PointerDown {
            button: PointerButton::Primary,
            at: Point::new(1, 20),
            select: false
        }),
        Some(SearchAction::Close)
    );
}

#[test]
fn a_press_on_a_row_opens_it_and_shift_or_middle_opens_a_new_tab() {
    let mut case = Case::new();
    let row = layout().rows[3];
    let press = |button, select| InputEvent::PointerDown {
        button,
        at: centre(row),
        select,
    };
    let spr = DocId::parse("api/graphics/spr").unwrap();
    assert_eq!(
        case.send(press(PointerButton::Primary, false)),
        Some(SearchAction::Open {
            id: spr.clone(),
            new_tab: false
        })
    );
    assert_eq!(
        case.send(press(PointerButton::Primary, true)),
        Some(SearchAction::Open {
            id: spr.clone(),
            new_tab: true
        })
    );
    assert_eq!(
        case.send(press(PointerButton::Middle, false)),
        Some(SearchAction::Open {
            id: spr,
            new_tab: true
        })
    );
}

#[test]
fn a_long_list_scrolls_to_keep_the_highlight_in_view() {
    let files: Vec<(String, Vec<u8>)> = (0..20)
        .map(|n| {
            (
                format!("en/guide/p{n:02}.md"),
                format!("# P{n:02}\n\nText.\n").into_bytes(),
            )
        })
        .collect();
    let library = Library::from_files(files);
    let chain = FallbackChain::english();
    let source = Source {
        library: &library,
        chain: &chain,
    };
    let mut search = Search::new(source);
    for _ in 0..10 {
        search.handle_input(InputEvent::Down { select: false }, &layout(), source);
    }
    assert_eq!((search.focus(), search.first()), (10, 10 + 1 - LIST_ROWS));
    search.handle_input(InputEvent::Wheel { delta: 3 }, &layout(), source);
    assert_eq!(search.focus(), 7);
    assert_eq!(search.first(), 4);
    search.handle_input(InputEvent::PageDown { select: false }, &layout(), source);
    assert_eq!(search.focus(), 14);
}

#[test]
fn pages_that_are_not_in_the_chains_first_language_are_flagged() {
    let library = library();
    let chain = FallbackChain::new(Language::parse("es").unwrap());
    let source = Source {
        library: &library,
        chain: &chain,
    };
    let mut search = Search::new(source);
    for ch in "sp".chars() {
        search.handle_input(InputEvent::Text(ch), &layout(), source);
    }
    let found: Vec<_> = search
        .results()
        .iter()
        .map(|found| (found.title.as_str(), found.is_fallback))
        .collect();
    // The Spanish title replaces the English one, so `Sprites` no longer
    // matches, and the API pages exist only in English.
    assert_eq!(found, [("spr", true)]);
}

#[test]
fn clipboard_keys_reach_the_query() {
    let mut case = Case::new();
    case.type_text("abc");
    case.send(InputEvent::SelectAll);
    assert_eq!(
        case.send(InputEvent::Copy),
        Some(SearchAction::Copy("abc".to_owned()))
    );
    assert_eq!(case.send(InputEvent::Paste), Some(SearchAction::Paste));
    case.search.paste(
        "spr\nignored",
        Source {
            library: &case.library,
            chain: &case.chain,
        },
    );
    assert_eq!(case.search.field().text(), "spr");
    assert_eq!(case.titles(), ["spr", "Sprites"]);
}

#[test]
fn the_window_sits_inside_the_area_and_holds_its_parts() {
    let layout = layout();
    assert!(layout.window.x >= AREA.x && layout.window.right() <= AREA.right());
    assert!(layout.window.y >= AREA.y && layout.window.bottom() <= AREA.bottom());
    for rect in layout
        .scopes
        .iter()
        .chain(&layout.rows)
        .chain([&layout.field, &layout.count])
    {
        assert!(
            rect.x >= layout.window.x && rect.right() <= layout.window.right(),
            "{rect:?}"
        );
        assert!(
            rect.y >= layout.window.y && rect.bottom() <= layout.window.bottom(),
            "{rect:?}"
        );
    }
    assert_eq!(layout.rows.len(), LIST_ROWS);
}
