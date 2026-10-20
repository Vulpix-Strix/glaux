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
