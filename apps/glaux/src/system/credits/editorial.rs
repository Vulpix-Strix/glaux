use std::collections::BTreeMap;

use serde::Deserialize;

#[derive(Debug, Default, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Editorial {
    /// Sections in the order they roll. A section without items is not
    /// shown.
    #[serde(default)]
    pub(super) sections: Vec<Section>,
    /// Who made each piece of bundled art, by the name the inventory gives
    /// it. The inventory does not record authors.
    #[serde(default)]
    pub(super) authors: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Section {
    pub(super) title: String,
    #[serde(default)]
    pub(super) items: Vec<Item>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub(super) enum Item {
    Person { role: String, name: String },
    Message(String),
}

impl Editorial {
    /// Reads the credits file. A file that does not parse yields no
    /// credits, and the reason is logged, because the credits view should
    /// still open and show the legal notices.
    pub(super) fn parse(source: &str) -> Self {
        Self::strict(source).unwrap_or_else(|error| {
            tracing::error!(%error, "rom/credits.ron is invalid, showing no editorial credits");
            Self::default()
        })
    }

    pub(super) fn strict(source: &str) -> Result<Self, ron::error::SpannedError> {
        ron::from_str(source)
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn people_and_messages_share_a_section() {
        let credits = Editorial::strict(
            r#"(sections: [(title: "Thanks", items: [
                Person(role: "Music", name: "Ada"),
                Message("To everyone"),
            ])])"#,
        )
        .expect("valid credits");
        assert_eq!(
            credits.sections[0].items,
            [
                Item::Person {
                    role: "Music".to_owned(),
                    name: "Ada".to_owned()
                },
                Item::Message("To everyone".to_owned()),
            ]
        );
        assert!(credits.authors.is_empty());
    }

    #[test]
    fn a_file_that_does_not_parse_shows_no_credits() {
        assert_eq!(Editorial::parse("(sections: [oops"), Editorial::default());
    }

    #[test]
    fn unknown_fields_are_rejected_so_typos_do_not_pass_silently() {
        assert!(Editorial::strict(r#"(sections: [(titel: "x")])"#).is_err());
    }

    #[test]
    fn the_shipped_credits_parse() {
        Editorial::strict(crate::rom::CREDITS_RON).expect("rom/credits.ron is valid");
    }
}
