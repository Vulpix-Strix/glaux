use pretty_assertions::assert_eq;

use super::*;

fn license(id: &str) -> Expression {
    Expression::License(id.to_owned())
}

#[test]
fn a_lone_license_is_required() {
    let expression = Expression::parse("MIT");
    assert_eq!(expression, license("MIT"));
    assert_eq!(expression.terms(), Terms::Single);
    assert_eq!(
        expression.roles(),
        Roles {
            required: vec!["MIT".to_owned()],
            offered: vec![],
        }
    );
}

#[test]
fn or_offers_every_alternative() {
    let expression = Expression::parse("MIT OR Apache-2.0");
    assert_eq!(expression.terms(), Terms::Choice(2));
    assert_eq!(
        expression.roles(),
        Roles {
            required: vec![],
            offered: vec!["MIT".to_owned(), "Apache-2.0".to_owned()],
        }
    );
}

#[test]
fn and_requires_every_part() {
    let expression = Expression::parse("MIT AND Zlib");
    assert_eq!(expression.terms(), Terms::All(2));
    assert_eq!(expression.roles().required, ["MIT", "Zlib"]);
    assert!(expression.roles().offered.is_empty());
}

#[test]
fn and_binds_tighter_than_or() {
    assert_eq!(
        Expression::parse("A OR B AND C"),
        Expression::Or(vec![
            license("A"),
            Expression::And(vec![license("B"), license("C")]),
        ])
    );
}

#[test]
fn everything_under_an_or_is_offered_even_inside_an_and() {
    let roles = Expression::parse("(A AND B) OR C").roles();
    assert!(roles.required.is_empty());
    assert_eq!(roles.offered, ["A", "B", "C"]);
}

#[test]
fn an_or_inside_an_and_leaves_the_other_part_required() {
    let roles = Expression::parse("(A OR B) AND C").roles();
    assert_eq!(roles.required, ["C"]);
    assert_eq!(roles.offered, ["A", "B"]);
}

#[test]
fn an_exception_stays_with_its_license() {
    let expression = Expression::parse("Apache-2.0 WITH LLVM-exception OR MIT");
    assert_eq!(
        expression,
        Expression::Or(vec![
            license("Apache-2.0 WITH LLVM-exception"),
            license("MIT"),
        ])
    );
}

#[test]
fn a_license_repeated_in_one_expression_counts_once() {
    assert_eq!(Expression::parse("MIT OR MIT").roles().offered, ["MIT"]);
}

#[test]
fn malformed_text_is_kept_whole_as_one_license() {
    for text in ["MIT OR", "(MIT", "MIT)", "WITH", "A WITH"] {
        assert_eq!(Expression::parse(text), license(text), "{text}");
    }
}
