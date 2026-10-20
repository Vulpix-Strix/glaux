use super::spdx::{Expression, Roles, Terms};
use crate::licenses::{Category, Entry, LicenseText, Licenses};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct LicenseSummary {
    pub(super) id: String,
    pub(super) required: usize,
    pub(super) offered: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum LicenseFilter {
    Id(String),
    Undeclared,
}

#[derive(Debug)]
pub(super) struct Catalog {
    licenses: Licenses,
    /// Each entry's parsed license, in entry order. `None` when the entry
    /// states none.
    expressions: Vec<Option<Expression>>,
    summary: Vec<LicenseSummary>,
    undeclared: usize,
}

impl Catalog {
    pub(super) fn new(licenses: Licenses) -> Self {
        let expressions: Vec<Option<Expression>> = licenses
            .entries
            .iter()
            .map(|entry| {
                (!entry.license.trim().is_empty()).then(|| Expression::parse(&entry.license))
            })
            .collect();
        let mut summary: Vec<LicenseSummary> = Vec::new();
        let mut bump = |id: &str, offered: bool| {
            let at = match summary.iter().position(|license| license.id == id) {
                Some(at) => at,
                None => {
                    summary.push(LicenseSummary {
                        id: id.to_owned(),
                        required: 0,
                        offered: 0,
                    });
                    summary.len() - 1
                }
            };
            if offered {
                summary[at].offered += 1;
            } else {
                summary[at].required += 1;
            }
        };
        for expression in expressions.iter().flatten() {
            let Roles { required, offered } = expression.roles();
            required.iter().for_each(|id| bump(id, false));
            offered.iter().for_each(|id| bump(id, true));
        }
        summary.sort_by(|a, b| {
            (b.required + b.offered)
                .cmp(&(a.required + a.offered))
                .then_with(|| a.id.cmp(&b.id))
        });
        let undeclared = expressions.iter().filter(|e| e.is_none()).count();
        Self {
            licenses,
            expressions,
            summary,
            undeclared,
        }
    }

    pub(super) fn entries(&self) -> &[Entry] {
        &self.licenses.entries
    }

    pub(super) fn texts_of(&self, entry: usize) -> Vec<&LicenseText> {
        self.licenses
            .texts_of(&self.licenses.entries[entry])
            .collect()
    }

    pub(super) fn summary(&self) -> &[LicenseSummary] {
        &self.summary
    }

    pub(super) fn undeclared(&self) -> usize {
        self.undeclared
    }

    pub(super) fn text_count(&self) -> usize {
        self.licenses.texts.len()
    }

    pub(super) fn count(&self, category: Option<Category>) -> usize {
        self.licenses
            .entries
            .iter()
            .filter(|entry| category.is_none_or(|wanted| entry.category == wanted))
            .count()
    }

    pub(super) fn terms(&self, entry: usize) -> Option<Terms> {
        self.expressions[entry].as_ref().map(Expression::terms)
    }

    pub(super) fn matches(&self, entry: usize, filter: &LicenseFilter) -> bool {
        match (filter, &self.expressions[entry]) {
            (LicenseFilter::Undeclared, expression) => expression.is_none(),
            (LicenseFilter::Id(id), Some(expression)) => {
                let roles = expression.roles();
                roles.required.contains(id) || roles.offered.contains(id)
            }
            (LicenseFilter::Id(_), None) => false,
        }
    }

    pub(super) fn find(&self, name: &str, category: Category) -> Option<&Entry> {
        self.licenses
            .entries
            .iter()
            .find(|entry| entry.category == category && entry.name == name)
    }
}

#[cfg(test)]
mod tests;
