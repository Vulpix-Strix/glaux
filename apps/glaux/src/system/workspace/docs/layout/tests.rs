use glaux_docs::DocId;
use glaux_i18n::Language;
use pretty_assertions::assert_eq;

use super::*;
use crate::system::test_font::LAYOUT_FONT;

fn page(source: &str) -> Page {
    Page::new(
        DocId::parse("guide/test").unwrap(),
        Language::english(),
        source.to_owned(),
    )
}

fn lay(source: &str, cols: usize, filter: LanguageFilter) -> Layout {
    build(&Params {
        page: &page(source),
        tag: String::new(),
        cols,
        filter,
        font: &LAYOUT_FONT,
    })
}

fn all(source: &str, cols: usize) -> Layout {
    lay(source, cols, LanguageFilter::All)
}

/// A row's text with the gaps between its runs as spaces.
fn text(row: &Row) -> String {
    let mut text = String::new();
    for run in &row.runs {
        while LAYOUT_FONT.text_cells(&text) < run.col {
            text.push(' ');
        }
        text.push_str(&run.text);
    }
    text
}

/// The text of every row that is not a gap.
fn lines(layout: &Layout) -> Vec<String> {
    layout
        .rows
        .iter()
        .filter(|row| row.gap.is_none())
        .map(text)
        .collect()
}

fn cells(row: &Row) -> usize {
    row.runs
        .iter()
        .map(|run| run.col + run.cells)
        .max()
        .unwrap_or(0)
}

#[test]
fn a_paragraph_wraps_at_spaces_and_keeps_every_word() {
    let layout = all("# T\n\nthe quick brown fox jumps over the lazy dog\n", 12);
    assert_eq!(
        lines(&layout)[1..],
        ["the quick", "brown fox", "jumps over", "the lazy dog"]
    );
    for row in &layout.rows {
        assert!(cells(row) <= 12);
    }
}

#[test]
fn a_word_longer_than_a_row_breaks_where_the_row_ends() {
    let layout = all("# T\n\nab abcdefghijklmnop cd\n", 6);
    assert_eq!(
        lines(&layout)[1..],
        ["ab", "abcdef", "ghijkl", "mnop", "cd"]
    );
}

#[test]
fn wide_glyphs_take_two_cells_when_wrapping() {
    let layout = all("# T\n\n✅✅✅✅ a\n", 6);
    let rows = lines(&layout);
    assert_eq!(rows[1..], ["✅✅✅", "✅ a"]);
    for row in &layout.rows {
        assert!(cells(row) <= 6, "{}", text(row));
    }
}

#[test]
fn the_title_is_the_header_and_not_repeated() {
    let layout = build(&Params {
        page: &page("# Title\n\nSummary.\n"),
        tag: "fn  graphics".to_owned(),
        cols: 30,
        filter: LanguageFilter::All,
        font: &LAYOUT_FONT,
    });
    // The tag ends at the last of 30 cells.
    let header = format!("{:<18}fn  graphics", "Title");
    assert_eq!(lines(&layout), [header.as_str(), "Summary."]);
    assert_eq!(layout.rows[0].deco.fill, Some(BAR));
}

#[test]
fn the_header_drops_its_tag_when_the_title_needs_the_room() {
    let layout = build(&Params {
        page: &page("# A rather long title\n\nSummary.\n"),
        tag: "fn  graphics".to_owned(),
        cols: 24,
        filter: LanguageFilter::All,
        font: &LAYOUT_FONT,
    });
    assert_eq!(text(&layout.rows[0]), "A rather long title");
}

#[test]
fn headings_are_listed_with_their_anchor_and_row() {
    let layout = all(
        "# T\n\nIntro.\n\n## First part\n\ntext\n\n### Deep\n\n## Second\n",
        40,
    );
    let found: Vec<_> = layout
        .headings
        .iter()
        .map(|h| (h.level, h.title.as_str(), h.anchor.as_str()))
        .collect();
    assert_eq!(
        found,
        [
            (2, "First part", "first-part"),
            (3, "Deep", "deep"),
            (2, "Second", "second")
        ]
    );
    for heading in &layout.headings {
        assert_eq!(text(&layout.rows[heading.row]), heading.title);
    }
    assert_eq!(layout.anchor_row("second"), Some(layout.headings[2].row));
    assert_eq!(layout.anchor_row("missing"), None);
}

#[test]
fn section_headings_get_a_rule_and_deeper_ones_do_not() {
    let layout = all("# T\n\n## Two\n\n### Three\n", 20);
    let under = |title: &str| {
        layout
            .rows
            .iter()
            .find(|row| text(row) == title)
            .map(|row| row.deco.underline)
    };
    assert_eq!(under("Two"), Some(Some(BAR)));
    assert_eq!(under("Three"), Some(None));
}

#[test]
fn links_are_numbered_in_order_and_found_by_cell() {
    let layout = all(
        "# T\n\nSee [one](glaux://guide/a) and [two words](https://example.com).\n",
        40,
    );
    assert_eq!(layout.links, ["glaux://guide/a", "https://example.com"]);
    let row = layout.link_rows[0];
    assert_eq!(text(&layout.rows[row]), "See one and two words.");
    assert_eq!(layout.link_at(row, 4), Some(0));
    assert_eq!(layout.link_at(row, 3), None);
    // The space inside a link belongs to it, so its underline is unbroken.
    assert_eq!(layout.link_at(row, 16), Some(1));
    assert_eq!(layout.link_at(row, 20), Some(1));
    assert_eq!(layout.link_at(row, 21), None);
    let runs: Vec<_> = layout.rows[row]
        .runs
        .iter()
        .map(|run| (run.text.as_str(), run.style.underline))
        .collect();
    assert_eq!(
        runs,
        [
            ("See ", false),
            ("one", true),
            (" and ", false),
            ("two words", true),
            (".", false)
        ]
    );
}

#[test]
fn a_link_that_wraps_keeps_its_number_on_both_rows() {
    let layout = all("# T\n\nGo [to the next page](glaux://guide/a) now\n", 14);
    assert_eq!(layout.links.len(), 1);
    let rows: Vec<_> = layout
        .rows
        .iter()
        .enumerate()
        .filter(|(_, row)| row.runs.iter().any(|run| run.style.link == Some(0)))
        .map(|(index, _)| index)
        .collect();
    assert_eq!(rows.len(), 2);
    assert_eq!(layout.link_rows[0], rows[0]);
}

#[test]
fn lists_mark_their_items_and_indent_the_wrapped_lines() {
    let layout = all(
        "# T\n\n- one two three four\n- five\n\n9. nine\n10. ten\n",
        12,
    );
    assert_eq!(
        lines(&layout)[1..],
        ["• one two", "  three four", "• five", "9. nine", "10. ten"]
    );
}

#[test]
fn nested_lists_indent_one_more_level() {
    let layout = all("# T\n\n- outer\n  - inner\n", 20);
    assert_eq!(lines(&layout)[1..], ["• outer", "  • inner"]);
}

#[test]
fn code_blocks_are_boxed_with_a_cell_of_margin() {
    let layout = all("# T\n\n```text\nab\ncd\n```\n", 10);
    let code: Vec<&Row> = layout
        .rows
        .iter()
        .filter(|row| row.deco.frame.is_some())
        .collect();
    assert_eq!(
        code.iter().map(|row| text(row)).collect::<Vec<_>>(),
        [" ab", " cd"]
    );
    let frame = |index: usize| code[index].deco.frame.unwrap();
    assert!(frame(0).boxed && frame(0).first && !frame(0).last);
    assert!(frame(1).last && !frame(1).first);
    assert_eq!(code[0].top, BOX_PAD);
    assert_eq!(code[1].bottom, BOX_PAD);
}

#[test]
fn long_code_lines_wrap_without_losing_a_character() {
    let source: String = ('a'..='z').cycle().take(50).collect();
    let layout = all(&format!("# T\n\n```text\n{source}\n```\n"), 12);
    let rows: Vec<&Row> = layout
        .rows
        .iter()
        .filter(|row| row.deco.frame.is_some())
        .collect();
    assert!(rows.len() > 1);
    let mut rebuilt = String::new();
    for (index, row) in rows.iter().enumerate() {
        assert!(cells(row) <= 12, "{}", text(row));
        let mut runs = row.runs.iter();
        if index > 0 {
            let marker = runs.next().unwrap();
            assert_eq!(marker.text, CONTINUATION);
            assert_eq!((marker.col, marker.style.ink), (1, DIM));
        }
        for run in runs {
            rebuilt.push_str(&run.text);
        }
    }
    assert_eq!(rebuilt, source);
}

#[test]
fn tabs_in_code_become_spaces() {
    let layout = all("# T\n\n```text\n\tx\n```\n", 20);
    let row = layout
        .rows
        .iter()
        .find(|row| row.deco.frame.is_some())
        .unwrap();
    assert_eq!(text(row), "     x");
}

#[test]
fn highlighted_code_keeps_the_text_between_its_spans() {
    let source = "local x = 1";
    let layout = all(&format!("# T\n\n```luau\n{source}\n```\n"), 30);
    let row = layout
        .rows
        .iter()
        .find(|row| row.deco.frame.is_some())
        .unwrap();
    let joined: String = row.runs.iter().map(|run| run.text.as_str()).collect();
    assert_eq!(joined, source);
    let ink = |word: &str| {
        row.runs
            .iter()
            .find(|run| run.text.contains(word))
            .map(|run| run.style.ink)
    };
    assert_eq!(ink("local"), Some(syntax_theme::color(SyntaxKind::Keyword)));
    assert_eq!(ink("1"), Some(syntax_theme::color(SyntaxKind::Number)));
    assert_ne!(ink("local"), ink("1"));
}

#[test]
fn code_in_a_language_the_parser_does_not_know_is_plain() {
    let layout = all("# T\n\n```text\nlocal x\n```\n", 30);
    let row = layout
        .rows
        .iter()
        .find(|row| row.deco.frame.is_some())
        .unwrap();
    assert!(row.runs.iter().all(|run| run.style.ink == TEXT));
}

const TABLE: &str =
    "# T\n\n| Name | Meaning |\n|---|---|\n| `a` | the first one |\n| `bb` | second |\n";

fn table_rows(layout: &Layout) -> Vec<&Row> {
    layout
        .rows
        .iter()
        .filter(|row| row.deco.table.is_some())
        .collect()
}

#[test]
fn a_table_that_fits_takes_one_row_per_table_row() {
    let layout = all(TABLE, 40);
    let rows = table_rows(&layout);
    assert_eq!(
        rows.iter().map(|row| text(row)).collect::<Vec<_>>(),
        [" Name Meaning", " a    the first one", " bb   second"]
    );
    let lines = rows[1].deco.table.as_ref().unwrap();
    // A margin cell, the 4 cells of `Name`, and a gap.
    assert_eq!(lines.dividers, [6]);
    assert!(lines.rule_above && !lines.last && !lines.outer_top);
    assert!(rows[0].deco.table.as_ref().unwrap().outer_top);
    assert!(rows[2].deco.table.as_ref().unwrap().last);
    assert_eq!(rows[0].deco.fill, Some(BAR));
    // Margins and the gap around 4 and 13 cells of text.
    assert_eq!(lines.width, 1 + 4 + 1 + 13 + 1);
}

#[test]
fn a_table_that_does_not_fit_wraps_its_cells_between_words() {
    let layout = all(TABLE, 16);
    let rows = table_rows(&layout);
    assert!(rows.len() > 3);
    for row in &rows {
        assert!(cells(row) <= 16, "{}", text(row));
    }
    let words: Vec<String> = rows
        .iter()
        .flat_map(|row| row.runs.iter().map(|run| run.text.clone()))
        .flat_map(|text| {
            text.split_whitespace()
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .collect();
    assert_eq!(
        words,
        [
            "Name", "Meaning", "a", "the", "first", "one", "bb", "second"
        ]
    );
    // Only the first line of a table row has the rule above it.
    let ruled = rows
        .iter()
        .filter(|row| row.deco.table.as_ref().unwrap().rule_above)
        .count();
    assert_eq!(ruled, 3);
}

#[test]
fn a_table_whose_longest_words_cannot_sit_side_by_side_is_stacked() {
    let layout = all(
        "# T\n\n| Name | Meaning |\n|---|---|\n| `alphabetical` | unbelievable |\n",
        14,
    );
    let rows = table_rows(&layout);
    let texts: Vec<_> = rows.iter().map(|row| text(row)).collect();
    assert_eq!(
        texts,
        [" Name", "  Meaning", " alphabetical", "  unbelievable"]
    );
    assert!(
        rows.iter()
            .all(|row| row.deco.table.as_ref().unwrap().dividers.is_empty())
    );
}

#[test]
fn a_stacked_table_with_many_columns_labels_the_cells_under_the_first() {
    let layout = all(
        "# T\n\n| A | Bb | Cc |\n|---|---|---|\n| `alphabetical` | x | y |\n",
        8,
    );
    let texts: Vec<_> = table_rows(&layout).iter().map(|row| text(row)).collect();
    assert!(texts.contains(&"  Bb: x".to_owned()), "{texts:?}");
    assert!(texts.contains(&"  Cc: y".to_owned()), "{texts:?}");
}

#[test]
fn table_cells_keep_their_inline_code_and_links() {
    let layout = all(
        "# T\n\n| Name | See |\n|---|---|\n| `x` | [spr](glaux://api/graphics/spr) |\n",
        40,
    );
    let row = table_rows(&layout)[1];
    assert_eq!(row.runs[0].style.bg, Some(BAR));
    assert_eq!(row.runs[1].style.link, Some(0));
    assert_eq!(layout.links, ["glaux://api/graphics/spr"]);
}

const BOTH: &str = "# T\n\nCommon.\n\n### Luau\n\nLuau text.\n\n```luau\nlocal a = 1\n```\n\n### Rhai\n\nRhai text.\n\n```rhai\nlet a = 1;\n```\n\n## After\n\nBack.\n";

#[test]
fn the_language_filter_shows_one_language_or_both() {
    let all = lines(&all(BOTH, 40));
    assert!(all.iter().any(|line| line == "Luau text."));
    assert!(all.iter().any(|line| line == "Rhai text."));

    let luau = lines(&lay(BOTH, 40, LanguageFilter::Luau));
    assert!(luau.contains(&"Luau text.".to_owned()));
    assert!(luau.contains(&" local a = 1".to_owned()));
    assert!(!luau.iter().any(|line| line.contains("Rhai")));
    assert!(!luau.iter().any(|line| line.contains("let a")));
    assert!(luau.contains(&"Common.".to_owned()));
    assert!(luau.contains(&"Back.".to_owned()));

    let rhai = lines(&lay(BOTH, 40, LanguageFilter::Rhai));
    assert!(rhai.contains(&"Rhai text.".to_owned()));
    assert!(!rhai.iter().any(|line| line.contains("Luau")));
}

#[test]
fn the_filter_leaves_out_the_headings_of_hidden_parts() {
    let luau = lay(BOTH, 40, LanguageFilter::Luau);
    let titles: Vec<_> = luau.headings.iter().map(|h| h.title.as_str()).collect();
    assert_eq!(titles, ["Luau", "After"]);
    // The anchors still come from the page's own headings.
    assert_eq!(luau.headings[1].anchor, "after");
}

#[test]
fn quotes_and_alerts_frame_their_rows() {
    let layout = all(
        "# T\n\n> [!WARNING]\n> Careful now.\n\n> Quoted.\n\n---\n",
        20,
    );
    let framed: Vec<_> = layout
        .rows
        .iter()
        .filter_map(|row| {
            row.deco
                .frame
                .map(|frame| (text(row), frame.ink, frame.boxed))
        })
        .collect();
    assert_eq!(
        framed,
        [
            (" Warning".to_owned(), WARN, true),
            (" Careful now.".to_owned(), WARN, true),
            ("  Quoted.".to_owned(), DIM, false),
        ]
    );
    assert!(layout.rows.iter().any(|row| row.deco.rule));
}

#[test]
fn only_the_rows_that_fit_whole_are_visible() {
    let layout = all("# T\n\none\n\ntwo\n\nthree\n", 20);
    // Header (cell_h + 2), a gap of 4, then lines of 12.
    let cell_h = layout.cell_h;
    assert_eq!(layout.rows[0].height(cell_h), cell_h + 2);
    let total: i32 = layout.rows.iter().map(|row| row.height(cell_h)).sum();
    assert_eq!(layout.visible(0, total), 0..layout.rows.len());
    assert_eq!(layout.visible(0, total - 1), 0..layout.rows.len() - 1);
    assert_eq!(
        layout.visible(layout.rows.len() + 3, 100),
        layout.rows.len()..layout.rows.len()
    );
}

#[test]
fn the_farthest_scroll_still_fills_the_view_to_the_last_row() {
    let layout = all("# T\n\none\n\ntwo\n\nthree\n\nfour\n", 20);
    let cell_h = layout.cell_h;
    let height = 3 * cell_h;
    let last = layout.max_scroll(height);
    assert_eq!(layout.visible(last, height).end, layout.rows.len());
    assert!(layout.visible(last - 1, height).end < layout.rows.len());
    // A view taller than the page never scrolls.
    assert_eq!(layout.max_scroll(1000), 0);
}

#[test]
fn constructs_that_degrade_still_lay_out() {
    let layout = all(
        "# T\n\n![alt](x.png) ~~gone~~ <b>x</b>\n\n<div>y</div>\n",
        40,
    );
    assert_eq!(lines(&layout)[1], "[alt] gone x");
}

#[test]
fn a_page_without_a_title_is_headed_by_its_id() {
    let layout = all("Just text.\n", 40);
    assert_eq!(text(&layout.rows[0]), "guide/test");
}

#[test]
fn the_embedded_pages_all_lay_out_at_every_width() {
    let library = glaux_docs::Library::embedded();
    for language in library.languages() {
        for page in library.pages(&language) {
            for cols in [8, 20, 37, 51, 80] {
                for filter in [
                    LanguageFilter::All,
                    LanguageFilter::Luau,
                    LanguageFilter::Rhai,
                ] {
                    let layout = build(&Params {
                        page,
                        tag: String::new(),
                        cols,
                        filter,
                        font: &LAYOUT_FONT,
                    });
                    for row in &layout.rows {
                        assert!(
                            cells(row) <= cols.max(1),
                            "{} at {cols}: {}",
                            page.id(),
                            text(row)
                        );
                    }
                    assert_eq!(layout.links.len(), layout.link_rows.len());
                    assert!(layout.link_rows.iter().all(|&row| row < layout.rows.len()));
                }
            }
        }
    }
}

#[test]
fn docs_glyphs_everything_the_layout_draws_for_the_shipped_pages_is_drawable() {
    let font = crate::rom::load().unwrap().font().clone();
    let library = glaux_docs::Library::embedded();
    let chain = glaux_i18n::FallbackChain::english();
    let mut missing = Vec::new();
    for id in super::super::nav::order(&library, &chain) {
        let page = library.page(&id, &chain).unwrap().page;
        // A narrow width forces wrapping, stacked tables and continuation
        // markers, which add glyphs the page source does not hold.
        for cols in [20, 53] {
            let layout = build(&Params {
                page: &page,
                tag: String::new(),
                cols,
                filter: LanguageFilter::All,
                font: &font,
            });
            for run in layout.rows.iter().flat_map(|row| &row.runs) {
                missing.extend(
                    run.text
                        .chars()
                        .filter(|&ch| font.glyph_id(ch).is_none())
                        .map(|ch| format!("{id} at {cols} columns: U+{:04X}", u32::from(ch))),
                );
            }
        }
    }
    missing.dedup();
    assert_eq!(missing, Vec::<String>::new());
}
