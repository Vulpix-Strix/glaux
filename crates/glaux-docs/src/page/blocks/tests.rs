use glaux_i18n::Language;
use pretty_assertions::assert_eq;

use super::*;
use crate::{DocId, Page, Scope, ScriptLanguage, anchor};

fn page(source: &str) -> Page {
    Page::new(
        DocId::parse("guide/test").unwrap(),
        Language::english(),
        source.to_owned(),
    )
}

fn kinds(source: &str) -> Vec<BlockKind> {
    page(source)
        .blocks()
        .iter()
        .map(|block| block.kind.clone())
        .collect()
}

fn text(text: &str) -> Inline {
    Inline::Text(text.to_owned())
}

fn code(text: &str) -> Inline {
    Inline::Code(text.to_owned())
}

#[test]
fn headings_and_paragraphs_keep_their_inline_content() {
    assert_eq!(
        kinds("# Title\n\nSome `code`, a [link](glaux://guide/index)\nand *more* **text**.\n"),
        [
            BlockKind::Heading {
                level: 1,
                inlines: vec![text("Title")]
            },
            BlockKind::Paragraph(vec![
                text("Some "),
                code("code"),
                text(", a "),
                Inline::Link {
                    target: "glaux://guide/index".into(),
                    children: vec![text("link")]
                },
                text(" and "),
                Inline::Emph(vec![text("more")]),
                text(" "),
                Inline::Strong(vec![text("text")]),
                text("."),
            ]),
        ]
    );
}

#[test]
fn code_blocks_keep_every_line_and_their_language() {
    assert_eq!(
        kinds("```luau\nlocal x = 1\n\nprint(x)\n```\n\n```\nplain\n```\n"),
        [
            BlockKind::Code {
                lang: Some("luau".into()),
                lines: vec!["local x = 1".into(), String::new(), "print(x)".into()]
            },
            BlockKind::Code {
                lang: None,
                lines: vec!["plain".into()]
            },
        ]
    );
}

#[test]
fn lists_keep_their_kind_start_and_item_blocks() {
    assert_eq!(
        kinds("- one\n- `two`\n\n3. three\n4. four\n"),
        [
            BlockKind::List {
                ordered: false,
                start: 1,
                items: vec![
                    vec![BlockKind::Paragraph(vec![text("one")])],
                    vec![BlockKind::Paragraph(vec![code("two")])],
                ]
            },
            BlockKind::List {
                ordered: true,
                start: 3,
                items: vec![
                    vec![BlockKind::Paragraph(vec![text("three")])],
                    vec![BlockKind::Paragraph(vec![text("four")])],
                ]
            },
        ]
    );
}

#[test]
fn nested_lists_stay_inside_their_item() {
    let blocks = kinds("- outer\n  - inner\n");
    let [BlockKind::List { items, .. }] = blocks.as_slice() else {
        panic!("{blocks:?}");
    };
    assert_eq!(items.len(), 1);
    assert!(matches!(items[0][1], BlockKind::List { .. }), "{items:?}");
}

#[test]
fn table_cells_hold_inline_code_and_links() {
    assert_eq!(
        kinds("| Name | Meaning |\n|---|---|\n| `x` | See [spr](glaux://api/graphics/spr) |\n"),
        [BlockKind::Table {
            header: vec![vec![text("Name")], vec![text("Meaning")]],
            rows: vec![vec![
                vec![code("x")],
                vec![
                    text("See "),
                    Inline::Link {
                        target: "glaux://api/graphics/spr".into(),
                        children: vec![text("spr")]
                    }
                ],
            ]],
        }]
    );
}

#[test]
fn alerts_quotes_and_rules() {
    assert_eq!(
        kinds("> [!WARNING]\n> Careful.\n\n> Quoted.\n\n---\n"),
        [
            BlockKind::Alert {
                kind: AlertKind::Warning,
                title: None,
                blocks: vec![BlockKind::Paragraph(vec![text("Careful.")])]
            },
            BlockKind::Quote(vec![BlockKind::Paragraph(vec![text("Quoted.")])]),
            BlockKind::Rule,
        ]
    );
}

#[test]
fn constructs_without_a_picture_degrade_to_text() {
    assert_eq!(
        kinds("An ![alt text](x.png) and ~~gone~~.\n"),
        [BlockKind::Paragraph(vec![text("An [alt text] and gone.")])]
    );
}

#[test]
fn raw_html_and_footnotes_are_dropped() {
    assert_eq!(
        kinds("<div>x</div>\n\nText[^1] <b>bold</b>.\n\n[^1]: Note.\n"),
        [BlockKind::Paragraph(vec![text("Text bold.")])]
    );
}

#[test]
fn language_subsections_and_tagged_blocks_carry_their_scope() {
    let page = page(
        "# T\n\nCommon.\n\n```rhai\nfn a() {}\n```\n\n## Examples\n\n### Luau\n\nLuau text.\n\n```luau\nx()\n```\n\n### Rhai\n\nRhai text.\n\n## After\n\nBack.\n",
    );
    let scopes: Vec<_> = page
        .blocks()
        .iter()
        .map(|block| match &block.kind {
            BlockKind::Heading { inlines, .. } => (plain_text(inlines), block.scope),
            BlockKind::Paragraph(inlines) => (plain_text(inlines), block.scope),
            BlockKind::Code { lines, .. } => (lines.join("\n"), block.scope),
            other => panic!("{other:?}"),
        })
        .collect();
    let luau = Scope::Language(ScriptLanguage::Luau);
    let rhai = Scope::Language(ScriptLanguage::Rhai);
    assert_eq!(
        scopes,
        [
            ("T".to_owned(), Scope::Common),
            ("Common.".to_owned(), Scope::Common),
            ("fn a() {}".to_owned(), rhai),
            ("Examples".to_owned(), Scope::Common),
            ("Luau".to_owned(), luau),
            ("Luau text.".to_owned(), luau),
            ("x()".to_owned(), luau),
            ("Rhai".to_owned(), rhai),
            ("Rhai text.".to_owned(), rhai),
            ("After".to_owned(), Scope::Common),
            ("Back.".to_owned(), Scope::Common),
        ]
    );
}

#[test]
fn windows_line_endings_read_like_unix_ones() {
    let unix = "# T\n\nOne\ntwo.\n\n```luau\na\nb\n```\n\n| A | B |\n|---|---|\n| 1 | 2 |\n";
    let windows = unix.replace('\n', "\r\n");
    assert_eq!(kinds(&windows), kinds(unix));
}

#[test]
fn adding_blocks_leaves_the_other_views_of_the_page_alone() {
    let page = page(
        "# T\n\nSee [a](glaux://guide/a).\n\n## Parameters\n\n| Name | Meaning |\n|---|---|\n| `x` | Column |\n",
    );
    assert_eq!(page.links().len(), 1);
    assert_eq!(page.outline().len(), 2);
    assert_eq!(page.parameters().map(<[_]>::len), Some(1));
    assert_eq!(page.title(), Some("T"));
}

#[test]
fn anchors_follow_the_heading_title() {
    assert_eq!(anchor("Parameters"), "parameters");
    assert_eq!(anchor("The game loop"), "the-game-loop");
    assert_eq!(anchor("  A -- B! "), "a-b");
    assert_eq!(anchor("Éxito"), "éxito");
}

/// The website computes the same anchors in TypeScript (`test/anchor.test.ts`
/// in glaux-website lists these cases too), so a change here is a change to
/// every published link that points at a heading.
#[test]
fn anchors_handle_punctuation_and_unicode() {
    let cases = [
        ("Don't panic", "don-t-panic"),
        ("snake_case_name", "snake-case-name"),
        ("A & B", "a-b"),
        ("C++ vs. C#", "c-vs-c"),
        ("Año nuevo", "año-nuevo"),
        ("日本語 の 見出し", "日本語-の-見出し"),
        ("Ⅷ chapter", "ⅷ-chapter"),
        ("½ cup", "½-cup"),
        ("İstanbul", "i\u{307}stanbul"),
        ("!!!", ""),
        ("--x--", "x"),
        ("Emoji 🎮 time", "emoji-time"),
        ("Cafe\u{301}", "cafe"),
        ("हिन्दी", "हिन-दी"),
        ("Straße", "straße"),
        ("ΣΑΣ", "σασ"),
        ("x\u{200b}y", "x-y"),
        ("١٢٣ arabic digits", "١٢٣-arabic-digits"),
    ];
    for (title, expected) in cases {
        assert_eq!(anchor(title), expected, "{title:?}");
    }
}
