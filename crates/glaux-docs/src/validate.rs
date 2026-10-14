//! Checks over the documentation, the API catalog and the UI catalogs.
//!
//! Every check returns [`Diagnostic`]s instead of failing, so one run lists
//! every problem. The tests, `mise run docs-check` and the app all use these
//! functions.

use std::collections::BTreeSet;

use glaux_i18n::validate::{CatalogIssue, Severity as CatalogSeverity};
use glaux_i18n::{Catalog, CatalogError, Language};

use crate::{Diagnostic, DocId, Library, Page, URI_SCHEME};

/// Checks the whole library: pages, links, images, translations, the API
/// catalog against its pages, and `ids.lock`.
pub fn validate(library: &Library) -> Vec<Diagnostic> {
    let mut diagnostics = library.load_problems().to_vec();
    let english = Language::english();
    let english_ids = library.page_ids(&english);
    if english_ids.is_empty() {
        diagnostics.push(Diagnostic::error(
            "missing-english",
            "docs/en",
            "there are no English pages, and English is the fallback for every language",
        ));
    }

    for language in library.languages() {
        for page in library.pages(&language) {
            check_page(library, page, &english_ids, &mut diagnostics);
        }
        if language != english {
            check_translation(library, &language, &english_ids, &mut diagnostics);
        }
    }
    check_catalog_pages(library, &english_ids, &mut diagnostics);
    check_ids_lock(library, &english_ids, &mut diagnostics);
    diagnostics
}

fn page_location(page: &Page) -> String {
    format!("docs/{}/{}.md", page.language(), page.id())
}

fn check_page(
    library: &Library,
    page: &Page,
    english_ids: &BTreeSet<DocId>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let location = page_location(page);
    if page.title().is_none() {
        diagnostics.push(Diagnostic::error(
            "missing-title",
            &location,
            "page has no level 1 heading",
        ));
    }
    if page.summary().is_none() {
        diagnostics.push(Diagnostic::error(
            "missing-summary",
            &location,
            "page has no paragraph to use as its summary",
        ));
    }
    for link in page.links() {
        if let Some(problem) = link_problem(&link.target, english_ids) {
            diagnostics.push(Diagnostic::error(
                problem.0,
                format!("{location}:{}", link.line),
                problem.1,
            ));
        }
    }
    for image in page.images() {
        let at = format!("{location}:{}", image.line);
        if image.alt.trim().is_empty() {
            diagnostics.push(Diagnostic::warning(
                "image-alt",
                &at,
                format!(
                    "image {:?} has no alt text, which is its fallback",
                    image.target
                ),
            ));
        }
        match image_problem(library, page, &image.target) {
            Some(Problem::Error(code, message)) => {
                diagnostics.push(Diagnostic::error(code, at, message));
            }
            Some(Problem::Warning(code, message)) => {
                diagnostics.push(Diagnostic::warning(code, at, message));
            }
            None => {}
        }
    }
    for line in page.html_lines() {
        diagnostics.push(Diagnostic::warning(
            "raw-html",
            format!("{location}:{line}"),
            "raw HTML is never rendered by Glaux; use Markdown",
        ));
    }
}

enum Problem {
    Error(&'static str, String),
    Warning(&'static str, String),
}

fn link_problem(target: &str, english_ids: &BTreeSet<DocId>) -> Option<(&'static str, String)> {
    if target.is_empty() {
        return Some(("empty-link", "link has no target".into()));
    }
    if target.starts_with('#') {
        return None;
    }
    if let Some(parsed) = DocId::from_uri(target) {
        return match parsed {
            Err(error) => Some(("bad-link", error.to_string())),
            Ok(id) if !english_ids.contains(&id) => Some((
                "broken-link",
                format!("no page has the id {id}, so {target} does not resolve"),
            )),
            Ok(_) => None,
        };
    }
    match scheme(target) {
        Some("http" | "https" | "mailto") => None,
        Some(other) => Some((
            "unsupported-link",
            format!("links with scheme `{other}:` are not supported"),
        )),
        None => Some((
            "relative-link",
            format!("{target:?} is a file path; link to a page with {URI_SCHEME}<id>"),
        )),
    }
}

fn scheme(target: &str) -> Option<&str> {
    let (scheme, _) = target.split_once(':')?;
    let valid = !scheme.is_empty()
        && scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "+-.".contains(c));
    valid.then_some(scheme)
}

fn image_problem(library: &Library, page: &Page, target: &str) -> Option<Problem> {
    if target.starts_with("http://") || target.starts_with("https://") {
        return Some(Problem::Warning(
            "external-image",
            format!("{target} is not available offline"),
        ));
    }
    if scheme(target).is_some() {
        return Some(Problem::Error(
            "bad-image",
            format!("image target {target:?} must be a file in the documentation tree"),
        ));
    }
    let path = target.split(['#', '?']).next().unwrap_or(target);
    let directory = format!("{}/{}", page.language(), page.id());
    let directory = directory.rsplit_once('/').map_or("", |(parent, _)| parent);
    let Some(resolved) = resolve_relative(directory, path) else {
        return Some(Problem::Error(
            "bad-image",
            format!("image target {target:?} leaves the documentation tree"),
        ));
    };
    (!library.has_file(&resolved)).then(|| {
        Problem::Error(
            "missing-image",
            format!("no file at docs/{resolved} for image {target:?}"),
        )
    })
}

/// Joins `target` onto `directory` and removes `.` and `..`. Returns `None`
/// when the result would go above the root.
fn resolve_relative(directory: &str, target: &str) -> Option<String> {
    let mut parts: Vec<&str> = directory.split('/').filter(|s| !s.is_empty()).collect();
    for segment in target.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            other => parts.push(other),
        }
    }
    Some(parts.join("/"))
}

fn check_translation(
    library: &Library,
    language: &Language,
    english_ids: &BTreeSet<DocId>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let translated = library.page_ids(language);
    for id in translated.difference(english_ids) {
        diagnostics.push(Diagnostic::error(
            "orphan-translation",
            format!("docs/{language}/{id}.md"),
            "there is no English page with this id",
        ));
    }
    for id in english_ids.difference(&translated) {
        diagnostics.push(Diagnostic::warning(
            "missing-translation",
            format!("docs/{language}/{id}.md"),
            "page is not translated, so English is shown",
        ));
    }
}

fn check_catalog_pages(
    library: &Library,
    english_ids: &BTreeSet<DocId>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let english = Language::english();
    for entry in library.api().entries() {
        let Some(page) = library.page_in(&english, entry.id()) else {
            diagnostics.push(Diagnostic::error(
                "missing-api-page",
                format!("docs/en/{}.md", entry.id()),
                "the API catalog has this entry but there is no English page for it",
            ));
            continue;
        };
        let expected: Vec<&str> = entry.params().iter().map(|p| p.name.as_str()).collect();
        if let Some(documented) = page.parameter_names()
            && documented
                .iter()
                .map(String::as_str)
                .ne(expected.iter().copied())
        {
            diagnostics.push(Diagnostic::warning(
                "parameters-drift",
                page_location(page),
                format!(
                    "the Parameters table lists {documented:?} but the catalog has {expected:?}"
                ),
            ));
        }
    }
    for id in english_ids {
        let is_entry_page = id.section() == "api" && id.as_str().split('/').count() == 3;
        if is_entry_page && library.api().get(id).is_none() {
            diagnostics.push(Diagnostic::error(
                "orphan-api-page",
                format!("docs/en/{id}.md"),
                "the page is under api/<category>/ but the API catalog has no entry for it",
            ));
        }
    }
}

fn check_ids_lock(
    library: &Library,
    english_ids: &BTreeSet<DocId>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(lock) = library.ids_lock() else {
        diagnostics.push(Diagnostic::error(
            "missing-ids-lock",
            "docs/ids.lock",
            "file is missing; run `mise run docs-update-ids`",
        ));
        return;
    };
    for id in lock.ids().difference(english_ids) {
        diagnostics.push(Diagnostic::error(
            "locked-id-missing",
            "docs/ids.lock",
            format!("{id} was published but has no English page; restore the page or retire the id by editing ids.lock"),
        ));
    }
    for id in english_ids.difference(lock.ids()) {
        diagnostics.push(Diagnostic::warning(
            "unlocked-id",
            format!("docs/en/{id}.md"),
            "id is not in ids.lock; run `mise run docs-update-ids`",
        ));
    }
}

/// A UI catalog source to check: its language, a location to show in
/// diagnostics, and its Fluent text.
pub struct UiCatalogSource<'a> {
    pub language: Language,
    pub location: &'a str,
    pub source: &'a str,
}

/// Checks UI catalogs: syntax, that every message resolves, and, for each
/// language other than English, consistency with the English catalog.
pub fn ui_catalogs(sources: &[UiCatalogSource<'_>]) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut parsed = Vec::new();
    for source in sources {
        match Catalog::parse(source.language.clone(), source.source) {
            Ok(catalog) => parsed.push((source.location, catalog)),
            Err(CatalogError::Syntax { issues, .. }) => {
                for issue in issues {
                    diagnostics.push(Diagnostic::error(
                        "ui-syntax",
                        format!("{}:{}", source.location, issue.line),
                        issue.message,
                    ));
                }
            }
            Err(error @ CatalogError::DuplicateMessage { .. }) => {
                diagnostics.push(Diagnostic::error(
                    "ui-duplicate",
                    source.location,
                    error.to_string(),
                ));
            }
        }
    }

    let english = Language::english();
    let reference = parsed
        .iter()
        .find(|(_, catalog)| catalog.language() == &english)
        .map(|(_, catalog)| catalog);
    if reference.is_none() {
        diagnostics.push(Diagnostic::error(
            "ui-missing-english",
            "rom/lang/en.ftl",
            "the English catalog is missing or invalid, and English is the fallback",
        ));
    }
    for (location, catalog) in &parsed {
        let resolution = glaux_i18n::validate::check_resolution(catalog);
        let comparison = match reference {
            Some(reference) if catalog.language() != &english => {
                glaux_i18n::validate::check(reference, catalog)
            }
            _ => Vec::new(),
        };
        for issue in resolution.into_iter().chain(comparison) {
            diagnostics.push(ui_diagnostic(location, &issue));
        }
    }
    diagnostics
}

fn ui_diagnostic(location: &str, issue: &CatalogIssue) -> Diagnostic {
    let code = match issue {
        CatalogIssue::UnknownMessage { .. } => "ui-unknown-message",
        CatalogIssue::MissingTranslation { .. } => "ui-missing-translation",
        CatalogIssue::VariableMismatch { .. } => "ui-variables",
        CatalogIssue::Unresolvable { .. } => "ui-unresolvable",
    };
    match issue.severity() {
        CatalogSeverity::Error => Diagnostic::error(code, location, issue.to_string()),
        CatalogSeverity::Warning => Diagnostic::warning(code, location, issue.to_string()),
    }
}

#[cfg(test)]
mod tests;
