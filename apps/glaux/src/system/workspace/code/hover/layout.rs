//! Where the hover popup sits and how its text breaks into lines.

use glaux_machine::font::BitmapFont;
use glaux_syntax::SyntaxKind;

use super::view::HoverView;
use crate::system::screen::{DIM, PALE};
use crate::system::syntax_theme;
use crate::system::workspace::code::popup::{Place, SHADOW};
use crate::system::workspace::{Rect, take_cells, wrap};
use crate::ui_text;

/// Space around the popup's text.
const PAD_X: i32 = 4;
const PAD_Y: i32 = 1;
/// Widest the popup gets, in columns, however much room there is.
const MAX_COLUMNS: usize = 40;
/// How many lines of the summary and rows of parameters the popup tries to
/// show, from the most to the least. When even the least does not fit
/// above or below the word, there is no popup.
const BUDGETS: [(usize, usize); 5] = [(4, 4), (4, 2), (2, 2), (2, 0), (1, 0)];
/// Stands for the parameters a short popup leaves out.
const ELIDED: &str = "..";

/// A run of text on one row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Text {
    pub x: i32,
    pub y: i32,
    pub text: String,
    pub color: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HoverLayout {
    /// The border included.
    pub window: Rect,
    pub rules: Vec<Rect>,
    pub texts: Vec<Text>,
}

impl HoverLayout {
    /// Lays out `view` under the word at `word`, or over it when there is no
    /// room below. `None` when the popup would not fit in `place.area`.
    pub fn new(place: Place<'_>, view: &HoverView, word: Rect) -> Option<Self> {
        BUDGETS
            .into_iter()
            .find_map(|(summary, params)| Self::with_budget(place, view, word, summary, params))
    }

    fn with_budget(
        place: Place<'_>,
        view: &HoverView,
        word: Rect,
        summary_lines: usize,
        param_rows: usize,
    ) -> Option<Self> {
        let Place {
            font,
            area,
            cell_w,
            cell_h,
        } = place;
        let chrome = 2 + 2 * PAD_X;
        let columns = usize::try_from((area.width - SHADOW - chrome) / cell_w.max(1))
            .ok()?
            .min(MAX_COLUMNS);
        if columns == 0 {
            return None;
        }

        let head: Vec<(String, u8)> = view
            .signature
            .iter()
            .map(|(text, kind)| (text.clone(), syntax_theme::color(*kind)))
            .collect();
        let head_cells: usize = head.iter().map(|(text, _)| font.text_cells(text)).sum();
        let tag_cells = view.tag.as_deref().map_or(0, |tag| font.text_cells(tag));
        let summary: Vec<String> = view
            .summary
            .as_deref()
            .map(|summary| wrap(font, summary, columns))
            .unwrap_or_default()
            .into_iter()
            .take(summary_lines)
            .collect();
        let table = Table::new(font, view, param_rows, columns);
        let returns = view.returns.map(|ty| {
            let label = ui_text::get("code-hover-returns");
            (font.text_cells(&label) + 1, label, ty)
        });
        let foot = view.opens_page.then(|| ui_text::get("code-hover-foot"));

        let widest = [
            (head_cells + if tag_cells > 0 { tag_cells + 2 } else { 0 }).min(columns),
            summary
                .iter()
                .map(|line| font.text_cells(line))
                .max()
                .unwrap_or(0),
            table.cells(font),
            returns
                .as_ref()
                .map_or(0, |(label, _, ty)| label + font.text_cells(ty)),
            foot.as_deref().map_or(0, |foot| font.text_cells(foot)),
        ]
        .into_iter()
        .max()
        .unwrap_or(0)
        .min(columns);

        let rows = |count: usize| {
            if count == 0 {
                0
            } else {
                1 + PAD_Y + to_i32(count) * cell_h + PAD_Y
            }
        };
        let height = 2
            + PAD_Y
            + cell_h
            + PAD_Y
            + rows(summary.len())
            + rows(table.rows.len())
            + rows(usize::from(returns.is_some()))
            + rows(usize::from(foot.is_some()));
        let width = to_i32(widest) * cell_w + chrome;

        let x = word
            .x
            .clamp(area.x, (area.right() - width - SHADOW).max(area.x));
        let below = word.bottom();
        let above = word.y - height - SHADOW;
        let y = if below + height + SHADOW <= area.bottom() {
            below
        } else if above >= area.y {
            above
        } else {
            return None;
        };
        let window = Rect {
            x,
            y,
            width,
            height,
        };

        let text_x = window.x + 1 + PAD_X;
        let mut rules = Vec::new();
        let mut texts = Vec::new();
        let mut cursor = window.y + 1 + PAD_Y;
        let put = |texts: &mut Vec<Text>, x: i32, y: i32, text: &str, color: u8| {
            texts.push(Text {
                x,
                y,
                text: text.to_owned(),
                color,
            });
        };

        let mut at = text_x;
        let mut left = columns;
        for (text, color) in &head {
            let shown = take_cells(font, text, left);
            put(&mut texts, at, cursor, shown, *color);
            let used = font.text_cells(shown);
            left -= used;
            at += to_i32(used) * cell_w;
        }
        if let Some(tag) = &view.tag
            && head_cells + tag_cells + 2 <= widest
        {
            let x = text_x + to_i32(widest - tag_cells) * cell_w;
            put(&mut texts, x, cursor, tag, DIM);
        }
        cursor += cell_h + PAD_Y;

        let section = |rules: &mut Vec<Rect>, cursor: &mut i32, count: usize| -> i32 {
            rules.push(Rect {
                x: window.x + 1,
                y: *cursor,
                width: window.width - 2,
                height: 1,
            });
            *cursor += 1 + PAD_Y;
            let top = *cursor;
            *cursor += to_i32(count) * cell_h + PAD_Y;
            top
        };

        if !summary.is_empty() {
            let top = section(&mut rules, &mut cursor, summary.len());
            for (row, line) in summary.iter().enumerate() {
                put(&mut texts, text_x, top + to_i32(row) * cell_h, line, PALE);
            }
        }
        if !table.rows.is_empty() {
            let top = section(&mut rules, &mut cursor, table.rows.len());
            for (row, line) in table.rows.iter().enumerate() {
                let y = top + to_i32(row) * cell_h;
                let name_x = text_x;
                let type_x = name_x + to_i32(table.name_cells) * cell_w;
                let meaning_x = type_x + to_i32(table.type_cells) * cell_w;
                put(
                    &mut texts,
                    name_x,
                    y,
                    &line.name,
                    syntax_theme::color(SyntaxKind::Variable),
                );
                put(
                    &mut texts,
                    type_x,
                    y,
                    &line.ty,
                    syntax_theme::color(SyntaxKind::Type),
                );
                put(&mut texts, meaning_x, y, &line.meaning, PALE);
            }
        }
        if let Some((label_cells, label, ty)) = &returns {
            let top = section(&mut rules, &mut cursor, 1);
            put(&mut texts, text_x, top, label, PALE);
            let color = syntax_theme::color(SyntaxKind::Type);
            put(
                &mut texts,
                text_x + to_i32(*label_cells) * cell_w,
                top,
                ty,
                color,
            );
        }
        if let Some(foot) = &foot {
            let top = section(&mut rules, &mut cursor, 1);
            put(&mut texts, text_x, top, foot, DIM);
        }
        Some(Self {
            window,
            rules,
            texts,
        })
    }

    /// The popup with its shadow, which the pointer may rest on.
    pub fn hit_area(&self) -> Rect {
        Rect {
            width: self.window.width + SHADOW,
            height: self.window.height + SHADOW,
            ..self.window
        }
    }
}

/// The parameters as a table of name, type and meaning.
struct Table {
    rows: Vec<Row>,
    name_cells: usize,
    type_cells: usize,
}

struct Row {
    name: String,
    ty: String,
    meaning: String,
}

impl Table {
    fn new(font: &BitmapFont, view: &HoverView, shown: usize, columns: usize) -> Self {
        let mut rows: Vec<Row> = view
            .params
            .iter()
            .take(shown)
            .map(|param| Row {
                name: param.name.clone(),
                ty: param.ty.to_owned(),
                meaning: param.meaning.clone(),
            })
            .collect();
        if view.params.len() > rows.len() {
            rows.push(Row {
                name: ELIDED.to_owned(),
                ty: String::new(),
                meaning: String::new(),
            });
        }
        let widest = |text: fn(&Row) -> &str| {
            rows.iter()
                .map(|row| font.text_cells(text(row)))
                .max()
                .map_or(0, |cells| cells + 1)
        };
        let name_cells = widest(|row| &row.name);
        let type_cells = widest(|row| &row.ty);
        let room = columns.saturating_sub(name_cells + type_cells);
        for row in &mut rows {
            row.meaning = take_cells(font, &row.meaning, room).to_owned();
        }
        Self {
            rows,
            name_cells,
            type_cells,
        }
    }

    /// The columns the widest row takes.
    fn cells(&self, font: &BitmapFont) -> usize {
        self.rows
            .iter()
            .map(|row| self.name_cells + self.type_cells + font.text_cells(&row.meaning))
            .max()
            .unwrap_or(0)
    }
}

fn to_i32(value: usize) -> i32 {
    i32::try_from(value).unwrap_or(i32::MAX)
}

#[cfg(test)]
mod tests;
