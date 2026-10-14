//! Maintenance commands for the documentation.
//!
//! ```text
//! glaux-docs check [--deny-warnings]
//! glaux-docs update-ids
//! ```
//!
//! Both are meant to be run through `mise run docs-check` and
//! `mise run docs-update-ids` from the repository root. They use the same
//! validation as the tests and add nothing of their own.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use glaux_docs::validate::{UiCatalogSource, ui_catalogs, validate};
use glaux_docs::{Diagnostic, Library};
use glaux_i18n::Language;

const USAGE: &str = "usage: glaux-docs <check [--deny-warnings] | update-ids>";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    match args.as_slice() {
        ["check"] => check(false),
        ["check", "--deny-warnings"] => check(true),
        ["update-ids"] => update_ids(),
        _ => {
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
    }
}

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn check(deny_warnings: bool) -> ExitCode {
    let library = Library::embedded();
    let mut diagnostics = validate(&library);
    diagnostics.extend(check_ui_catalogs(&repository_root().join("rom/lang")));
    report(&diagnostics, deny_warnings)
}

/// Reads every `<language>.ftl` in `directory` and checks them together.
fn check_ui_catalogs(directory: &Path) -> Vec<Diagnostic> {
    let location = "rom/lang";
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) => {
            return vec![Diagnostic::error(
                "ui-unreadable",
                location,
                error.to_string(),
            )];
        }
    };
    let mut files = Vec::new();
    let mut diagnostics = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("ftl") {
            continue;
        }
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        let shown = format!("{location}/{name}");
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_default();
        let language = match Language::parse(stem) {
            Ok(language) => language,
            Err(error) => {
                diagnostics.push(Diagnostic::error(
                    "ui-bad-language",
                    shown,
                    error.to_string(),
                ));
                continue;
            }
        };
        match fs::read_to_string(&path) {
            Ok(source) => files.push((language, shown, source)),
            Err(error) => {
                diagnostics.push(Diagnostic::error("ui-unreadable", shown, error.to_string()))
            }
        }
    }
    files.sort_by(|a, b| a.1.cmp(&b.1));
    let sources: Vec<_> = files
        .iter()
        .map(|(language, shown, source)| UiCatalogSource {
            language: language.clone(),
            location: shown,
            source,
        })
        .collect();
    diagnostics.extend(ui_catalogs(&sources));
    diagnostics
}

fn report(diagnostics: &[Diagnostic], deny_warnings: bool) -> ExitCode {
    for diagnostic in diagnostics {
        eprintln!("{diagnostic}");
    }
    let errors = diagnostics.iter().filter(|d| d.is_error()).count();
    let warnings = diagnostics.len() - errors;
    let failed = errors > 0 || (deny_warnings && warnings > 0);
    let verdict = if failed { "failed" } else { "ok" };
    eprintln!("docs-check {verdict}: {errors} error(s), {warnings} warning(s)");
    if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

fn update_ids() -> ExitCode {
    let library = Library::embedded();
    // Rewriting the file would silently drop a malformed line or hide a page
    // that failed to load, so refuse until those are fixed.
    let blocking: Vec<_> = validate(&library)
        .into_iter()
        .filter(|d| {
            matches!(
                d.code,
                "ids-lock-syntax" | "bad-page-id" | "bad-language-dir"
            )
        })
        .collect();
    if !blocking.is_empty() {
        for diagnostic in &blocking {
            eprintln!("{diagnostic}");
        }
        eprintln!("docs-update-ids failed: fix the problems above first");
        return ExitCode::FAILURE;
    }

    let current = library.page_ids(&Language::english());
    let lock = library.ids_lock().cloned().unwrap_or_default();
    let (updated, added) = lock.updated(&current);
    for id in lock.ids().difference(&current) {
        let diagnostic = Diagnostic::warning(
            "locked-id-kept",
            "docs/ids.lock",
            format!("{id} was published but has no English page; keeping it"),
        );
        eprintln!("{diagnostic}");
    }

    let path = repository_root().join("docs/ids.lock");
    let text = updated.render();
    if fs::read_to_string(&path).is_ok_and(|existing| existing == text) {
        eprintln!("docs/ids.lock is up to date");
        return ExitCode::SUCCESS;
    }
    if let Err(error) = fs::write(&path, &text) {
        eprintln!("cannot write {}: {error}", path.display());
        return ExitCode::FAILURE;
    }
    for id in &added {
        eprintln!("added {id}");
    }
    eprintln!("docs/ids.lock updated: {} id(s) added", added.len());
    ExitCode::SUCCESS
}
