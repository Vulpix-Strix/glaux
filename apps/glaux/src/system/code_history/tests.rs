use proptest::prelude::*;

use super::*;
use crate::system::code_edit::{self, Edit};
use pretty_assertions::assert_eq;

/// A document with its history and selection, edited the way the Code view
/// edits it: through the editing keys' replacements.
struct Editing {
    document: CodeDocument,
    history: EditHistory,
    selection: Selection,
}

impl Editing {
    /// `text` with the caret at `caret`.
    fn new(text: &str, caret: usize) -> Self {
        let document = CodeDocument::from_text("code/main.luau", text);
        let selection = Selection::collapsed(Caret::at(&document, caret));
        Self {
            document,
            history: EditHistory::default(),
            selection,
        }
    }

    fn text(&self) -> &str {
        self.document.text()
    }

    fn select(&mut self, anchor: usize, caret: usize) {
        self.selection =
            Selection::between(&self.document, anchor, Caret::at(&self.document, caret));
    }

    /// Applies `edit` and records it as `kind`, leaving the caret after
    /// it. Returns whether the document took it.
    fn apply(&mut self, edit: Option<Edit>, kind: EditKind) -> bool {
        let Some(Edit { range, text }) = edit else {
            return false;
        };
        let Ok(replaced) = self.document.replace(range, &text) else {
            return false;
        };
        self.history
            .record(&replaced, &text, kind, self.selection, None);
        self.selection = Selection::collapsed(Caret::at(&self.document, replaced.range.end));
        true
    }

    fn type_text(&mut self, text: &str) {
        for ch in text.chars() {
            self.apply(code_edit::typed(self.selection, ch), EditKind::Typing);
        }
    }

    fn enter(&mut self) {
        let edit = code_edit::newline(&self.document, self.selection);
        self.apply(Some(edit), EditKind::Structural);
    }

    fn backspace(&mut self) {
        self.apply(
            code_edit::backspace(&self.document, self.selection),
            EditKind::Backspace,
        );
    }

    fn delete(&mut self) {
        self.apply(
            code_edit::delete(&self.document, self.selection),
            EditKind::Delete,
        );
    }

    /// Undoes one step, returning whether there was one.
    fn undo(&mut self) -> bool {
        match self.history.undo(&mut self.document) {
            Some(result) => {
                self.selection = result.expect("history replays against its own text");
                true
            }
            None => false,
        }
    }

    fn redo(&mut self) -> bool {
        match self.history.redo(&mut self.document) {
            Some(result) => {
                self.selection = result.expect("history replays against its own text");
                true
            }
            None => false,
        }
    }

    /// Applies `edit` as a structural step that leaves `after` selected.
    fn apply_selecting(&mut self, edit: code_edit::Reselecting) {
        let replaced = self
            .document
            .replace(edit.edit.range, &edit.edit.text)
            .unwrap();
        let after = Selection::between(
            &self.document,
            edit.anchor,
            Caret::at(&self.document, edit.caret),
        );
        self.history.record(
            &replaced,
            &edit.edit.text,
            EditKind::Structural,
            self.selection,
            Some(after),
        );
        self.selection = after;
    }

    /// Undo steps, counted by undoing them all.
    fn undo_steps(&mut self) -> usize {
        let mut steps = 0;
        while self.undo() {
            steps += 1;
        }
        steps
    }

    fn caret(&self) -> usize {
        self.selection.caret().byte()
    }

    fn selected(&self) -> (usize, usize) {
        (self.selection.anchor(), self.caret())
    }
}

#[test]
fn a_typed_word_undoes_as_one_step_back_to_its_caret() {
    let mut editing = Editing::new("x = ;", 4);
    editing.type_text("hello world");
    assert_eq!(editing.text(), "x = hello world;");

    assert!(editing.undo());
    assert_eq!(editing.text(), "x = ;");
    assert_eq!(editing.caret(), 4);
    assert!(!editing.undo());
}

#[test]
fn runs_of_backspaces_and_of_deletes_each_undo_as_one_step() {
    let mut editing = Editing::new("abcdef", 4);
    for _ in 0..3 {
        editing.backspace();
    }
    assert_eq!(editing.text(), "aef");
    assert!(editing.undo());
    assert_eq!((editing.text(), editing.caret()), ("abcdef", 4));

    editing.select(1, 1);
    for _ in 0..3 {
        editing.delete();
    }
    assert_eq!(editing.text(), "aef");
    assert!(editing.undo());
    assert_eq!((editing.text(), editing.caret()), ("abcdef", 1));
}

#[test]
fn moving_the_caret_ends_a_run() {
    let mut editing = Editing::new("", 0);
    editing.type_text("ab");
    editing.select(0, 0);
    editing.type_text("x");
    assert_eq!(editing.text(), "xab");

    assert!(editing.undo());
    assert_eq!(editing.text(), "ab");
    assert!(editing.undo());
    assert_eq!(editing.text(), "");
}

#[test]
fn a_different_kind_of_edit_ends_a_run() {
    let mut editing = Editing::new("", 0);
    editing.type_text("ab");
    editing.backspace();
    editing.type_text("c");
    assert_eq!(editing.text(), "ac");

    assert_eq!(editing.undo_steps(), 3);
    assert_eq!(editing.text(), "");
}

#[test]
fn line_breaks_are_steps_of_their_own() {
    let mut editing = Editing::new("", 0);
    editing.type_text("a");
    editing.enter();
    editing.enter();
    editing.type_text("b");
    assert_eq!(editing.text(), "a\n\nb");

    let mut texts = Vec::new();
    while editing.undo() {
        texts.push(editing.text().to_owned());
    }
    assert_eq!(texts, ["a\n\n", "a\n", "a", ""]);
}

#[test]
fn deleting_a_selection_is_a_step_of_its_own() {
    let mut editing = Editing::new("abcdef", 6);
    editing.backspace();
    editing.select(1, 3);
    editing.apply(
        code_edit::backspace(&editing.document, editing.selection),
        EditKind::Structural,
    );
    editing.backspace();
    assert_eq!(editing.text(), "de");

    assert!(editing.undo());
    assert_eq!(editing.text(), "ade");
    assert!(editing.undo());
    assert_eq!(editing.text(), "abcde");
    assert_eq!(editing.selected(), (1, 3));
}

#[test]
fn typing_over_a_selection_restores_it_on_undo_and_keeps_typing_in_the_step() {
    let mut editing = Editing::new("hello world", 0);
    editing.select(6, 11);
    editing.type_text("there");
    assert_eq!(editing.text(), "hello there");

    assert!(editing.undo());
    assert_eq!(editing.text(), "hello world");
    assert_eq!(editing.selected(), (6, 11));
    assert!(!editing.undo());
}

#[test]
fn a_sealed_history_starts_a_new_step() {
    let mut editing = Editing::new("", 0);
    editing.type_text("ab");
    editing.history.seal();
    editing.type_text("cd");

    assert!(editing.undo());
    assert_eq!(editing.text(), "ab");
    assert!(editing.undo());
    assert_eq!(editing.text(), "");
}

#[test]
fn redo_reapplies_undone_steps_with_their_caret() {
    let mut editing = Editing::new("", 0);
    editing.type_text("ab");
    editing.enter();
    editing.type_text("cd");

    assert_eq!(editing.undo_steps(), 3);
    assert!(editing.redo());
    assert_eq!((editing.text(), editing.caret()), ("ab", 2));
    assert!(editing.redo());
    assert!(editing.redo());
    assert_eq!((editing.text(), editing.caret()), ("ab\ncd", 5));
    assert!(!editing.redo());
}

#[test]
fn an_edit_after_undo_drops_the_redo_steps_and_starts_its_own_step() {
    let mut editing = Editing::new("", 0);
    editing.type_text("ab");
    editing.history.seal();
    editing.type_text("c");
    assert!(editing.undo());
    editing.type_text("d");
    assert_eq!(editing.text(), "abd");

    assert!(!editing.redo());
    // Typing right after undo must not join the step it returned to.
    assert!(editing.undo());
    assert_eq!(editing.text(), "ab");
}

#[test]
fn the_oldest_steps_go_past_the_step_limit() {
    let mut editing = Editing::new("", 0);
    for _ in 0..STEP_LIMIT + 5 {
        editing.history.seal();
        editing.type_text("x");
    }
    assert_eq!(editing.undo_steps(), STEP_LIMIT);
    assert_eq!(editing.text(), "x".repeat(5));
}

#[test]
fn the_oldest_steps_go_past_the_byte_limit_but_the_newest_stays() {
    let mut editing = Editing::new("", 0);
    let half = "a".repeat(BYTE_LIMIT / 2 + 1);
    editing.apply(
        Some(Edit {
            range: 0..0,
            text: half.clone(),
        }),
        EditKind::Structural,
    );
    let end = editing.text().len();
    let huge = "b".repeat(BYTE_LIMIT + 1);
    editing.apply(
        Some(Edit {
            range: end..end,
            text: huge,
        }),
        EditKind::Structural,
    );

    assert_eq!(editing.undo_steps(), 1);
    assert_eq!(editing.text(), half);
}

#[test]
fn wide_and_multibyte_characters_undo_whole() {
    let mut editing = Editing::new("x", 1);
    editing.type_text("日本é🦀");
    editing.history.seal();
    editing.backspace();
    editing.backspace();
    assert_eq!(editing.text(), "x日本");

    assert!(editing.undo());
    assert_eq!(editing.text(), "x日本é🦀");
    assert_eq!(editing.caret(), "x日本é🦀".len());
    assert!(editing.undo());
    assert_eq!((editing.text(), editing.caret()), ("x", 1));
}

#[test]
fn crlf_line_breaks_undo_and_redo_as_one_ending() {
    let mut editing = Editing::new("a\r\nb", 4);
    editing.enter();
    editing.type_text("c");
    assert_eq!(editing.text(), "a\r\nb\r\nc");
    editing.history.seal();
    // Joining the lines takes the whole CRLF.
    editing.select(4, 4);
    editing.delete();
    assert_eq!(editing.text(), "a\r\nbc");

    assert_eq!(editing.undo_steps(), 3);
    assert_eq!(editing.text(), "a\r\nb");
    while editing.redo() {}
    assert_eq!(editing.text(), "a\r\nbc");
}

#[test]
fn edits_after_a_byte_order_mark_keep_it() {
    let text = "\u{feff}ab";
    let mut editing = Editing::new(text, 3);
    editing.type_text("x");
    editing.history.seal();
    editing.select(3, 6);
    editing.apply(
        code_edit::delete(&editing.document, editing.selection),
        EditKind::Structural,
    );
    assert_eq!(editing.text(), "\u{feff}");

    assert_eq!(editing.undo_steps(), 2);
    assert_eq!(editing.text(), text);
    assert_eq!(editing.caret(), 3);
}

fn awkward_text() -> impl Strategy<Value = String> {
    let pieces = prop::sample::select(vec![
        "a", " ", "\t", "\r\n", "\n", "é", "e\u{301}", "日", "🦀", "\u{feff}",
    ]);
    prop::collection::vec(pieces, 0..30).prop_map(|pieces| pieces.concat())
}

proptest! {
    #[test]
    fn undoing_everything_restores_the_text_and_redoing_restores_the_edits(
        text in awkward_text(),
        caret in 0usize..120,
        steps in prop::collection::vec((0u8..8, 0usize..120), 1..40),
    ) {
        let mut editing = Editing::new(&text, caret);
        for (step, target) in steps {
            match step {
                0 => editing.type_text("x"),
                1 => editing.type_text("日"),
                2 => editing.enter(),
                3 => editing.backspace(),
                4 => editing.delete(),
                5 => editing.history.seal(),
                6 => {
                    let anchor = editing.selection.anchor();
                    editing.select(anchor, target);
                }
                _ => editing.select(target, target),
            }
        }
        let edited = editing.text().to_owned();

        while editing.undo() {}
        prop_assert_eq!(editing.text(), text.as_str());
        while editing.redo() {}
        prop_assert_eq!(editing.text(), edited.as_str());
    }
}

#[test]
fn undo_and_redo_restore_the_lines_a_block_indent_selected() {
    let mut editing = Editing::new("a\nb\nc", 0);
    editing.select(0, 3);
    let edit = code_edit::indent_lines(
        &editing.document,
        editing.selection,
        code_edit::IndentUnit::Tab,
    )
    .unwrap();
    editing.apply_selecting(edit);
    assert_eq!(editing.text(), "\ta\n\tb\nc");
    let indented = editing.selection;
    assert_eq!(indented.range(), 0..5);
    assert!(editing.history.can_undo() && !editing.history.can_redo());
    editing.undo();
    assert_eq!(
        (editing.text(), editing.selection.range()),
        ("a\nb\nc", 0..3)
    );
    assert!(editing.history.can_redo());
    editing.redo();
    assert_eq!(editing.text(), "\ta\n\tb\nc");
    assert_eq!(editing.selection, indented);
}
