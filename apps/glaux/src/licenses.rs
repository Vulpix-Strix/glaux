//! Licenses of the third-party software shipped in Glaux.
//!
//! The data lives in `apps/glaux/licenses/` and is embedded in the binary, so
//! no file is needed at run time. `third-party.json` is generated from
//! `Cargo.lock` with `mise run licenses`. `extra.json` lists what Cargo cannot
//! see: vendored native code and bundled art.

use serde::Deserialize;

const THIRD_PARTY: &str = include_str!("../licenses/third-party.json");
const EXTRA: &str = include_str!("../licenses/extra.json");

/// What kind of component an entry credits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Category {
    /// A Rust crate found by cargo-about.
    Library,
    /// Native or vendored code that is compiled into Glaux.
    Native,
    /// Fonts, icons, palettes and other bundled art.
    Art,
}

/// What a text contains.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum TextKind {
    /// A license as distributed with the component.
    License,
    /// A license text that still has unresolved placeholders such as `<year>`.
    Template,
    /// Credits or a note that is not itself a license.
    Notice,
}

/// A license, template or notice, exactly as embedded.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub(crate) struct LicenseText {
    pub(crate) kind: TextKind,
    pub(crate) title: String,
    /// The SPDX identifier when the identification is reliable.
    pub(crate) spdx: Option<String>,
    pub(crate) content: String,
}

/// One credited component. Its texts are looked up with [`Licenses::texts_of`].
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub(crate) struct Entry {
    pub(crate) name: String,
    pub(crate) version: String,
    /// The SPDX expression the component declares. Its alternatives can be
    /// missing from the texts, because cargo-about supplies only the ones it
    /// chose.
    pub(crate) license: String,
    pub(crate) repository: Option<String>,
    pub(crate) category: Category,
    text_ids: Vec<usize>,
}

/// Every credited component and the texts they share.
#[derive(Debug, Deserialize)]
pub(crate) struct Licenses {
    pub(crate) texts: Vec<LicenseText>,
    pub(crate) entries: Vec<Entry>,
}

impl Licenses {
    /// The texts that apply to `entry`, in the order the data lists them.
    pub(crate) fn texts_of<'a>(
        &'a self,
        entry: &'a Entry,
    ) -> impl Iterator<Item = &'a LicenseText> {
        entry.text_ids.iter().map(|&id| &self.texts[id])
    }

    /// Appends `other`, which keeps its own numbering, shifting its references.
    fn extend(&mut self, other: Licenses) {
        let offset = self.texts.len();
        self.texts.extend(other.texts);
        self.entries
            .extend(other.entries.into_iter().map(|mut entry| {
                entry.text_ids.iter_mut().for_each(|id| *id += offset);
                entry
            }));
    }

    /// Checks that every reference points at an existing text, which is what
    /// lets [`Licenses::texts_of`] index without failing.
    fn validate(&self) -> anyhow::Result<()> {
        for entry in &self.entries {
            if let Some(id) = entry.text_ids.iter().find(|&&id| id >= self.texts.len()) {
                anyhow::bail!("{} refers to missing license text {id}", entry.name);
            }
        }
        Ok(())
    }
}

/// Every credited component, generated crates first and then the extras.
pub(crate) fn third_party() -> anyhow::Result<Licenses> {
    let mut licenses: Licenses = serde_json::from_str(THIRD_PARTY)?;
    licenses.extend(serde_json::from_str(EXTRA)?);
    licenses.validate()?;
    Ok(licenses)
}

#[cfg(test)]
mod tests;
