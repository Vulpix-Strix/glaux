use std::ops::RangeInclusive;

use comrak::nodes::{AstNode, NodeValue};
use comrak::{Arena, Options, parse_document};
use glaux_i18n::Language;

use crate::{DocId, ScriptLanguage};

/// The heading text that opens a Luau-only subsection.
const LUAU_HEADING: &str = "Luau";
/// The heading text that opens a Rhai-only subsection.
const RHAI_HEADING: &str = "Rhai";
/// Language-specific headings must be at least this deep, so `# Luau` can
/// still be an ordinary page title.
const MIN_LANGUAGE_HEADING_LEVEL: u8 = 3;
/// The section whose table is compared with the API catalog.
const PARAMETERS_HEADING: &str = "Parameters";

/// Which script language a part of a page is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// Applies to every language.
    Common,
    /// Only applies to one script language.
    Language(ScriptLanguage),
}

/// A heading and the lines it covers.
///
/// A subsection headed exactly `Luau` or `Rhai` at level 3 or deeper is
/// specific to that language, and so is everything below it. Every other
/// section is [`Scope::Common`]. A plain Markdown viewer shows all of it,
/// so pages stay readable without any special support.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    pub level: u8,
    pub title: String,
    pub scope: Scope,
    /// Lines from the heading to the line before the next heading of the
    /// same or a shallower level (one-based, inclusive).
    pub lines: RangeInclusive<usize>,
}

/// A link in the page text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkRef {
    pub target: String,
    pub line: usize,
}

/// An image in the page text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageRef {
    pub target: String,
    pub alt: String,
    pub line: usize,
}

/// A run of text a reader would see: prose, inline code, code block lines
/// and image alt text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextRun {
    pub line: usize,
    pub text: String,
}

/// One documentation page in one language.
#[derive(Debug, Clone)]
pub struct Page {
    id: DocId,
    language: Language,
    source: String,
    title: Option<String>,
    summary: Option<String>,
    outline: Vec<Section>,
    links: Vec<LinkRef>,
    images: Vec<ImageRef>,
    text_runs: Vec<TextRun>,
    html_lines: Vec<usize>,
    parameter_names: Option<Vec<String>>,
}

impl Page {
    /// Parses `source` as GitHub Flavored Markdown.
    pub fn new(id: DocId, language: Language, source: String) -> Self {
        let arena = Arena::new();
        let root = parse_document(&arena, &source, &options());
        let mut page = Self {
            id,
            language,
            title: None,
            summary: None,
            outline: Vec::new(),
            links: Vec::new(),
            images: Vec::new(),
            text_runs: Vec::new(),
            html_lines: Vec::new(),
            parameter_names: None,
            source: String::new(),
        };
        page.scan(root, source.lines().count().max(1));
        page.source = source;
        page
    }

    pub fn id(&self) -> &DocId {
        &self.id
    }

    pub fn language(&self) -> &Language {
        &self.language
    }

    /// The Markdown source.
    pub fn source(&self) -> &str {
        &self.source
    }

    /// The text of the first level 1 heading.
    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    /// The first paragraph of the page, as plain text on one line.
    pub fn summary(&self) -> Option<&str> {
        self.summary.as_deref()
    }

    /// Every heading in document order.
    pub fn outline(&self) -> &[Section] {
        &self.outline
    }

    pub fn links(&self) -> &[LinkRef] {
        &self.links
    }

    pub fn images(&self) -> &[ImageRef] {
        &self.images
    }

    /// The text a reader would see, with source lines, for checking that a
    /// font can draw it.
    pub fn text_runs(&self) -> &[TextRun] {
        &self.text_runs
    }

    /// Lines holding raw HTML other than comments.
    pub fn html_lines(&self) -> &[usize] {
        &self.html_lines
    }

    /// The first column of the table under a level 2 `Parameters` heading,
    /// or `None` when the page has no such table.
    pub fn parameter_names(&self) -> Option<&[String]> {
        self.parameter_names.as_deref()
    }

    fn scan<'a>(&mut self, root: &'a AstNode<'a>, total_lines: usize) {
        for node in root.descendants() {
            let line = node.data().sourcepos.start.line;
            match &node.data().value {
                NodeValue::Text(text) => self.push_text(line, text),
                NodeValue::Code(code) => self.push_text(line, &code.literal),
                NodeValue::CodeBlock(block) => {
                    let first = if block.fenced { line + 1 } else { line };
                    for (offset, text) in block.literal.lines().enumerate() {
                        self.push_text(first + offset, text);
                    }
                }
                NodeValue::Link(link) => self.links.push(LinkRef {
                    target: link.url.clone(),
                    line,
                }),
                NodeValue::Image(image) => self.images.push(ImageRef {
                    target: image.url.clone(),
                    alt: inline_text(node),
                    line,
                }),
                NodeValue::HtmlBlock(html) if !is_comment(&html.literal) => {
                    self.html_lines.push(line);
                }
                NodeValue::HtmlInline(html) if !is_comment(html) => self.html_lines.push(line),
                _ => {}
            }
        }

        let mut headings = Vec::new();
        for node in root.children() {
            let data = node.data();
            match &data.value {
                NodeValue::Heading(heading) => {
                    headings.push((heading.level, inline_text(node), data.sourcepos.start.line));
                }
                NodeValue::Paragraph if self.summary.is_none() => {
                    self.summary = Some(inline_text(node));
                }
                _ => {}
            }
        }
        self.title = headings
            .iter()
            .find(|(level, ..)| *level == 1)
            .map(|(_, title, _)| title.clone());
        self.outline = build_outline(&headings, total_lines);
        self.parameter_names = parameter_names(root, &self.outline);
    }

    fn push_text(&mut self, line: usize, text: &str) {
        if !text.is_empty() {
            self.text_runs.push(TextRun {
                line,
                text: text.to_owned(),
            });
        }
    }
}

fn options() -> Options<'static> {
    let mut options = Options::default();
    options.extension.table = true;
    options.extension.strikethrough = true;
    options.extension.tasklist = true;
    options.extension.autolink = true;
    options.extension.footnotes = true;
    options.extension.alerts = true;
    options
}

fn is_comment(html: &str) -> bool {
    html.trim_start().starts_with("<!--")
}

/// The plain text of a node's inline content on one line.
fn inline_text<'a>(node: &'a AstNode<'a>) -> String {
    let mut text = String::new();
    for descendant in node.descendants().skip(1) {
        match &descendant.data().value {
            NodeValue::Text(part) => text.push_str(part),
            NodeValue::Code(code) => text.push_str(&code.literal),
            NodeValue::SoftBreak | NodeValue::LineBreak => text.push(' '),
            _ => {}
        }
    }
    text
}

fn build_outline(headings: &[(u8, String, usize)], total_lines: usize) -> Vec<Section> {
    let mut sections = Vec::with_capacity(headings.len());
    let mut stack: Vec<(u8, Scope)> = Vec::new();
    for (index, (level, title, line)) in headings.iter().enumerate() {
        while stack.last().is_some_and(|(open, _)| open >= level) {
            stack.pop();
        }
        let own = if *level >= MIN_LANGUAGE_HEADING_LEVEL {
            match title.as_str() {
                LUAU_HEADING => Some(Scope::Language(ScriptLanguage::Luau)),
                RHAI_HEADING => Some(Scope::Language(ScriptLanguage::Rhai)),
                _ => None,
            }
        } else {
            None
        };
        let scope = own
            .or_else(|| stack.last().map(|(_, scope)| *scope))
            .unwrap_or(Scope::Common);
        stack.push((*level, scope));

        let end = headings[index + 1..]
            .iter()
            .find(|(next, ..)| next <= level)
            .map_or(total_lines, |(.., next_line)| next_line.saturating_sub(1));
        sections.push(Section {
            level: *level,
            title: title.clone(),
            scope,
            lines: *line..=end.max(*line),
        });
    }
    sections
}

fn parameter_names<'a>(root: &'a AstNode<'a>, outline: &[Section]) -> Option<Vec<String>> {
    let section = outline
        .iter()
        .find(|section| section.level == 2 && section.title == PARAMETERS_HEADING)?;
    let table = root.children().find(|node| {
        let data = node.data();
        matches!(data.value, NodeValue::Table(_))
            && section.lines.contains(&data.sourcepos.start.line)
    })?;
    let names = table
        .children()
        .filter(|row| matches!(row.data().value, NodeValue::TableRow(false)))
        .filter_map(|row| row.children().next())
        .map(|cell| inline_text(cell))
        .collect();
    Some(names)
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    fn page(source: &str) -> Page {
        Page::new(
            DocId::parse("guide/test").unwrap(),
            Language::english(),
            source.to_owned(),
        )
    }

    #[test]
    fn title_and_summary_come_from_the_first_heading_and_paragraph() {
        let page =
            page("# Drawing *sprites*\n\nPlaces a `tile`\non screen.\n\nSecond paragraph.\n");
        assert_eq!(page.title(), Some("Drawing sprites"));
        assert_eq!(page.summary(), Some("Places a tile on screen."));
    }

    #[test]
    fn page_without_heading_or_paragraph_has_neither() {
        let page = page("## Only a subsection\n");
        assert_eq!(page.title(), None);
        assert_eq!(page.summary(), None);
    }

    #[test]
    fn language_subsections_scope_everything_below_them() {
        let page = page(
            "# T\n\nIntro.\n\n## Notes\n\nCommon.\n\n### Luau\n\nLuau only.\n\n#### Detail\n\n### Rhai\n\nRhai only.\n\n## After\n",
        );
        let scopes: Vec<_> = page
            .outline()
            .iter()
            .map(|section| (section.title.as_str(), section.scope))
            .collect();
        assert_eq!(
            scopes,
            [
                ("T", Scope::Common),
                ("Notes", Scope::Common),
                ("Luau", Scope::Language(ScriptLanguage::Luau)),
                ("Detail", Scope::Language(ScriptLanguage::Luau)),
                ("Rhai", Scope::Language(ScriptLanguage::Rhai)),
                ("After", Scope::Common),
            ]
        );
    }

    #[test]
    fn shallow_headings_named_after_a_language_stay_common() {
        let page = page("# Luau\n\n## Rhai\n");
        assert!(page.outline().iter().all(|s| s.scope == Scope::Common));
    }

    #[test]
    fn section_lines_end_before_the_next_heading_of_equal_or_lower_depth() {
        let page = page("# T\n\n## A\n\ntext\n\n### A1\n\n## B\n");
        let ranges: Vec<_> = page
            .outline()
            .iter()
            .map(|s| (s.title.as_str(), s.lines.clone()))
            .collect();
        assert_eq!(
            ranges,
            [("T", 1..=9), ("A", 3..=8), ("A1", 7..=8), ("B", 9..=9)]
        );
    }

    #[test]
    fn links_and_images_are_collected_with_lines() {
        let page = page(
            "# T\n\nSee [spr](glaux://api/graphics/spr).\n\n![A sprite sheet](../assets/sheet.png)\n",
        );
        assert_eq!(
            page.links(),
            [LinkRef {
                target: "glaux://api/graphics/spr".into(),
                line: 3
            }]
        );
        assert_eq!(
            page.images(),
            [ImageRef {
                target: "../assets/sheet.png".into(),
                alt: "A sprite sheet".into(),
                line: 5
            }]
        );
    }

    #[test]
    fn raw_html_is_reported_but_comments_are_not() {
        let page = page("# T\n\n<!-- note -->\n\n<div>x</div>\n\nInline <b>bold</b>.\n");
        assert_eq!(page.html_lines(), [5, 7, 7]);
    }

    #[test]
    fn text_runs_cover_prose_code_and_alt_text_with_lines() {
        let page = page(
            "# T\n\nHello `pset`.\n\n```luau\ncls(1)\npset(0, 0, 2)\n```\n\n![Alt é](x.png)\n",
        );
        let runs: Vec<_> = page
            .text_runs()
            .iter()
            .map(|run| (run.line, run.text.as_str()))
            .collect();
        assert_eq!(
            runs,
            [
                (1, "T"),
                (3, "Hello "),
                (3, "pset"),
                (3, "."),
                (6, "cls(1)"),
                (7, "pset(0, 0, 2)"),
                (10, "Alt é"),
            ]
        );
    }

    #[test]
    fn parameters_table_names_are_read_from_the_first_column() {
        let page = page(
            "# T\n\n## Parameters\n\n| Name | Meaning |\n|---|---|\n| `x` | Column |\n| `y` | Row |\n\n## Other\n\n| `z` | no |\n|---|---|\n",
        );
        assert_eq!(
            page.parameter_names(),
            Some(&["x".to_owned(), "y".to_owned()][..])
        );
    }

    #[test]
    fn page_without_a_parameters_table_has_no_parameter_names() {
        assert_eq!(
            page("# T\n\n## Parameters\n\nNone.\n").parameter_names(),
            None
        );
        assert_eq!(page("# T\n").parameter_names(), None);
    }
}
