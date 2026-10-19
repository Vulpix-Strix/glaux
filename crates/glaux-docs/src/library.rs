use std::collections::{BTreeMap, BTreeSet};

use glaux_i18n::{FallbackChain, Language};
use rust_embed::RustEmbed;

use crate::{ApiCatalog, Diagnostic, DocId, IdsLock, Page};

mod search;

pub use search::{Hit, Rank};

/// Top-level directory holding the API catalog files.
const API_DIR: &str = "api";
/// Top-level directory holding shared images and other files.
const ASSETS_DIR: &str = "assets";
/// Directory names that are not languages.
const RESERVED_DIRS: [&str; 2] = [API_DIR, ASSETS_DIR];
/// The contributor guide at the documentation root, which is not a page.
const README: &str = "README.md";
/// The published ids file at the documentation root.
const IDS_LOCK: &str = "ids.lock";

/// The repository's `docs/` directory, embedded in the binary in every build
/// profile (`debug-embed`), so tests see exactly what ships.
#[derive(RustEmbed)]
#[folder = "../../docs/"]
struct Embedded;

/// A documentation page found for a request, and whether it is in the
/// language that was asked for.
#[derive(Debug, Clone, Copy)]
pub struct Resolved<'a> {
    pub page: &'a Page,
    /// `true` when the page comes from a later language in the chain.
    pub is_fallback: bool,
}

/// All documentation: pages in every language, the API catalog, and the
/// file inventory used to check links.
///
/// Loading never fails. Whatever cannot be used is left out and reported by
/// [`validate`](crate::validate::validate).
#[derive(Debug)]
pub struct Library {
    /// Pages by language tag, then id.
    pages: BTreeMap<String, BTreeMap<DocId, Page>>,
    catalog: ApiCatalog,
    /// Every file below the documentation root, as a `/` separated path.
    files: BTreeSet<String>,
    ids_lock: Option<IdsLock>,
    problems: Vec<Diagnostic>,
}

impl Library {
    /// The documentation embedded in this build.
    pub fn embedded() -> Self {
        let files = Embedded::iter().filter_map(|path| {
            let file = Embedded::get(&path)?;
            Some((path.into_owned(), file.data.into_owned()))
        });
        Self::from_files(files)
    }

    /// Builds a library from files, each a path below the documentation
    /// root and its bytes. Used for the embedded tree and for tests.
    pub fn from_files(files: impl IntoIterator<Item = (String, Vec<u8>)>) -> Self {
        let mut library = Self {
            pages: BTreeMap::new(),
            catalog: ApiCatalog::default(),
            files: BTreeSet::new(),
            ids_lock: None,
            problems: Vec::new(),
        };
        let mut catalog_files = Vec::new();
        for (path, bytes) in files {
            library.files.insert(path.clone());
            let Ok(text) = String::from_utf8(bytes) else {
                if is_text_path(&path) {
                    library.problems.push(Diagnostic::error(
                        "invalid-utf8",
                        format!("docs/{path}"),
                        "file is not valid UTF-8",
                    ));
                }
                continue;
            };
            library.add_text_file(path, text, &mut catalog_files);
        }
        let (catalog, diagnostics) = ApiCatalog::from_sources(
            catalog_files
                .iter()
                .map(|(path, text): &(String, String)| (path.as_str(), text.as_str())),
        );
        library.catalog = catalog;
        library
            .problems
            .extend(diagnostics.into_iter().map(prefix_docs));
        library
    }

    fn add_text_file(
        &mut self,
        path: String,
        text: String,
        catalog_files: &mut Vec<(String, String)>,
    ) {
        let segments: Vec<&str> = path.split('/').collect();
        match segments.as_slice() {
            [name] if *name == IDS_LOCK => {
                let (lock, errors) = IdsLock::parse(&text);
                for error in errors {
                    self.problems.push(Diagnostic::error(
                        "ids-lock-syntax",
                        format!("docs/{IDS_LOCK}:{}", error.line),
                        error.source.to_string(),
                    ));
                }
                self.ids_lock = Some(lock);
            }
            [name] if *name == README => {}
            [dir, file] if *dir == API_DIR && file.ends_with(".ron") => {
                catalog_files.push((path, text));
            }
            [dir, ..] if *dir == ASSETS_DIR => {}
            [dir, rest @ ..] if !RESERVED_DIRS.contains(dir) && path.ends_with(".md") => {
                self.add_page(&path, dir, rest, text);
            }
            _ => {
                // Images next to a page are fine. Anything else loose at the
                // root or in `api/` is probably misplaced.
                let loose = segments.len() == 1 || segments[0] == API_DIR;
                if loose {
                    self.problems.push(Diagnostic::warning(
                        "unrecognized-file",
                        format!("docs/{path}"),
                        "file is not part of the documentation layout",
                    ));
                }
            }
        }
    }

    fn add_page(&mut self, path: &str, dir: &str, rest: &[&str], text: String) {
        let location = format!("docs/{path}");
        let language = match Language::parse(dir) {
            Ok(language) => language,
            Err(error) => {
                self.problems.push(Diagnostic::error(
                    "bad-language-dir",
                    location,
                    error.to_string(),
                ));
                return;
            }
        };
        let id_text = rest.join("/");
        let id_text = id_text.strip_suffix(".md").unwrap_or(&id_text);
        let id = match DocId::parse(id_text) {
            Ok(id) => id,
            Err(error) => {
                self.problems.push(Diagnostic::error(
                    "bad-page-id",
                    location,
                    error.to_string(),
                ));
                return;
            }
        };
        let page = Page::new(id.clone(), language.clone(), text);
        self.pages
            .entry(language.to_string())
            .or_default()
            .insert(id, page);
    }

    /// The crate version, which is the Glaux release the documentation
    /// ships with.
    pub fn version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }

    /// The script API, merged from every category file.
    pub fn api(&self) -> &ApiCatalog {
        &self.catalog
    }

    /// Languages that have at least one page, ordered by tag.
    pub fn languages(&self) -> Vec<Language> {
        self.pages
            .keys()
            .filter_map(|tag| Language::parse(tag).ok())
            .collect()
    }

    /// Every page in `language`, ordered by id.
    pub fn pages(&self, language: &Language) -> impl Iterator<Item = &Page> {
        self.pages
            .get(&language.to_string())
            .into_iter()
            .flat_map(BTreeMap::values)
    }

    /// The page for `id` in exactly `language`, without fallback.
    pub fn page_in(&self, language: &Language, id: &DocId) -> Option<&Page> {
        self.pages.get(&language.to_string())?.get(id)
    }

    /// The page for `id` in the first language of `chain` that has it.
    pub fn page(&self, id: &DocId, chain: &FallbackChain) -> Option<Resolved<'_>> {
        chain.languages().iter().find_map(|language| {
            self.page_in(language, id).map(|page| Resolved {
                page,
                is_fallback: language != chain.requested(),
            })
        })
    }

    /// The ids of the pages in `language`.
    pub fn page_ids(&self, language: &Language) -> BTreeSet<DocId> {
        self.pages(language).map(|page| page.id().clone()).collect()
    }

    /// Whether the documentation root contains the file at `path`.
    pub fn has_file(&self, path: &str) -> bool {
        self.files.contains(path)
    }

    /// The contents of `ids.lock`, or `None` when the file is missing.
    pub fn ids_lock(&self) -> Option<&IdsLock> {
        self.ids_lock.as_ref()
    }

    pub(crate) fn load_problems(&self) -> &[Diagnostic] {
        &self.problems
    }
}

/// Whether a file is expected to be text. Images and other assets are not
/// read, so they cannot be invalid UTF-8.
fn is_text_path(path: &str) -> bool {
    [".md", ".ron", ".lock"]
        .iter()
        .any(|extension| path.ends_with(extension))
}

fn prefix_docs(mut diagnostic: Diagnostic) -> Diagnostic {
    diagnostic.location = format!("docs/{}", diagnostic.location);
    diagnostic
}

#[cfg(test)]
mod tests;
