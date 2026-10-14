use glaux_syntax::SyntaxKind;
use pretty_assertions::assert_eq;

use super::*;

fn syntax(document: &CodeDocument) -> DocumentSyntax {
    let mut syntax = DocumentSyntax::new(document.language());
    syntax.refresh(document, 0..document.text().len());
    syntax
}

fn kind_at(syntax: &DocumentSyntax, byte: usize) -> Option<SyntaxKind> {
    syntax
        .spans()
        .iter()
        .find(|span| span.range.contains(&byte))
        .map(|span| span.kind)
}

#[test]
fn the_documents_extension_picks_the_grammar() {
    let text = "fn area() { 1 }";
    let rhai = CodeDocument::from_text("code/a.rhai", text);
    let luau = CodeDocument::from_text("code/a.luau", text);
    assert_eq!(kind_at(&syntax(&rhai), 0), Some(SyntaxKind::Keyword));
    assert_ne!(kind_at(&syntax(&luau), 0), Some(SyntaxKind::Keyword));
}

#[test]
fn spans_cover_only_the_window() {
    let document =
        CodeDocument::from_text("code/a.luau", "local a = 1\nlocal b = 2\nlocal c = 3\n");
    let mut syntax = DocumentSyntax::new(Language::Luau);
    syntax.refresh(&document, 12..23);
    assert!(!syntax.spans().is_empty());
    assert!(
        syntax
            .spans()
            .iter()
            .all(|span| span.range.start >= 12 && span.range.end <= 23)
    );
}

/// Edits, undoing them, redoing them, and putting back an earlier text all
/// reach the document through `replace`, so the tree must come out the same
/// as a fresh parse each time.
#[test]
fn highlighting_follows_edits_undo_redo_and_reloads() {
    let original = "local a = 1\nfunction f() return \"é\" end\n";
    let mut document = CodeDocument::from_text("code/a.luau", original);
    let mut live = syntax(&document);
    let mut check = |document: &CodeDocument| {
        live.refresh(document, 0..document.text().len());
        assert_eq!(
            live.spans(),
            syntax(document).spans(),
            "{:?}",
            document.text()
        );
    };

    let typed = document.replace(11..11, " -- note").unwrap();
    check(&document);
    let removed = document.replace(0..5, "").unwrap();
    check(&document);
    // Undo both, then redo both.
    document
        .replace(removed.range.clone(), &removed.removed)
        .unwrap();
    check(&document);
    document
        .replace(typed.range.clone(), &typed.removed)
        .unwrap();
    check(&document);
    assert_eq!(document.text(), original);
    document.replace(0..5, "").unwrap();
    check(&document);
    // A reload replaces everything at once.
    let all = 0..document.text().len();
    document.replace(all, "function g() end\n").unwrap();
    check(&document);
}

#[test]
fn syntax_errors_leave_earlier_code_highlighted() {
    let document = CodeDocument::from_text("code/a.luau", "local ok = 1\nfunction (\n");
    assert_eq!(kind_at(&syntax(&document), 0), Some(SyntaxKind::Keyword));
}
