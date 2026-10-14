use glaux_docs::validate::{UiCatalogSource, ui_catalogs};
use glaux_docs::{Diagnostic, Library};
use glaux_i18n::{Catalog, Language};
use glaux_machine::font::BitmapFont;
use pretty_assertions::assert_eq;

use super::*;

fn system_font() -> BitmapFont {
    crate::rom::load().unwrap().font().clone()
}

fn parsed(tag: &str, source: &str) -> Catalog {
    Catalog::parse(Language::parse(tag).unwrap(), source).unwrap()
}

/// Describes `ch` when the font cannot draw it as itself.
fn undrawable(font: &BitmapFont, ch: char) -> Option<String> {
    font.glyph_id(ch)
        .is_none()
        .then(|| format!("U+{:04X} {ch:?}", u32::from(ch)))
}

/// Warnings for characters in the documentation the system font cannot
/// draw. They never stop the library from loading, so a page with one is
/// still readable with a placeholder box in its place.
fn docs_glyph_report(library: &Library, font: &BitmapFont) -> Vec<Diagnostic> {
    let mut report = Vec::new();
    for language in library.languages() {
        for page in library.pages(&language) {
            let mut reported = Vec::new();
            for run in page.text_runs() {
                for ch in run.text.chars() {
                    let Some(description) = undrawable(font, ch) else {
                        continue;
                    };
                    if reported.contains(&ch) {
                        continue;
                    }
                    reported.push(ch);
                    report.push(Diagnostic::warning(
                        "undrawable-character",
                        format!("docs/{language}/{}.md:{}", page.id(), run.line),
                        format!("{description} has no glyph in the system font"),
                    ));
                }
            }
        }
    }
    report
}

#[test]
fn the_embedded_catalogs_are_valid() {
    let sources: Vec<_> = CATALOGS
        .iter()
        .map(|(tag, location, source)| UiCatalogSource {
            language: Language::parse(tag).unwrap(),
            location,
            source,
        })
        .collect();
    assert_eq!(ui_catalogs(&sources), []);
}

#[test]
fn ui_glyphs_every_text_a_message_can_produce_is_drawable() {
    let font = system_font();
    let mut missing = Vec::new();
    for (tag, location, source) in CATALOGS {
        let catalog = parsed(tag, source);
        for id in catalog.ids() {
            for text in catalog.resolve_samples(id).unwrap().texts {
                missing.extend(
                    text.chars()
                        .filter_map(|ch| undrawable(&font, ch))
                        .map(|ch| format!("{location}: {id}: {ch}")),
                );
            }
        }
    }
    assert_eq!(missing, Vec::<String>::new());
}

#[test]
fn ui_glyphs_a_combining_accent_is_caught() {
    // Decomposed letters draw as a letter followed by a placeholder box.
    let catalog = parsed("es", "tab-code = Co\u{301}digo\n");
    let font = system_font();
    let texts = catalog.resolve_samples("tab-code").unwrap().texts;
    assert!(texts[0].chars().any(|ch| undrawable(&font, ch).is_some()));
    // The precomposed spelling is fine, as are the Spanish punctuation marks.
    let fine = parsed("es", "q = ¿Guardar cambios? ¡Listo! Año Árbol\n");
    let texts = fine.resolve_samples("q").unwrap().texts;
    assert!(texts[0].chars().all(|ch| undrawable(&font, ch).is_none()));
}

#[test]
fn docs_glyphs_the_shipped_english_docs_are_drawable() {
    let report = docs_glyph_report(&Library::embedded(), &system_font());
    assert_eq!(report, []);
}

#[test]
fn docs_glyphs_unsupported_characters_are_diagnostics_not_load_failures() {
    let library = Library::from_files([
        (
            "en/guide/index.md".to_owned(),
            "# Guide\n\nFine text.\n\nA smile \u{1F600} and \u{1F600} again.\n".into(),
        ),
        (
            "es/guide/index.md".to_owned(),
            "# Gu\u{ED}a\n\nHola.\n".into(),
        ),
    ]);
    // The library loaded both pages.
    assert_eq!(library.languages().len(), 2);
    let report = docs_glyph_report(&library, &system_font());
    // One warning for the emoji, once per page however often it appears.
    assert_eq!(report.len(), 1, "{report:?}");
    assert_eq!(report[0].code, "undrawable-character");
    assert_eq!(report[0].location, "docs/en/guide/index.md:5");
    assert!(!report[0].is_error());
}

#[test]
fn the_interface_translator_resolves_the_tab_names() {
    let text = translator();
    let names: Vec<_> = ["tab-code", "tab-sprites", "tab-map", "tab-sfx", "tab-music"]
        .map(|id| text.get(id).into_owned())
        .into();
    assert_eq!(names, ["Code", "Sprites", "Map", "SFX", "Music"]);
}
