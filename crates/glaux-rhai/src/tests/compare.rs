//! Comparison operators on floats.

use super::*;
use pretty_assertions::assert_eq;

/// The text each of `expressions` logs when evaluated in `tick()`.
fn evaluated(expressions: &[&str]) -> Vec<String> {
    let body: String = expressions
        .iter()
        .map(|expression| format!("log::info({expression});"))
        .collect();
    let mut runtime = started(&format!("fn tick() {{ {body} }}"));
    runtime.tick(NO_INPUT).unwrap();
    texts(runtime.take_output())
}

#[test]
fn floats_compare_exactly_with_floats_and_integers() {
    // Each pair differs by less than Rhai's relative epsilon, which its own
    // operators would treat as equal.
    let below = "29.999999999999996";
    let cases = [
        (format!("{below} < 30"), "true"),
        (format!("{below} < 30.0"), "true"),
        (format!("30 > {below}"), "true"),
        (format!("30.0 > {below}"), "true"),
        (format!("{below} <= 30"), "true"),
        (format!("30 >= {below}"), "true"),
        (format!("{below} >= 30"), "false"),
        (format!("30.0 <= {below}"), "false"),
        (format!("{below} == 30"), "false"),
        (format!("30 == {below}"), "false"),
        (format!("{below} != 30.0"), "true"),
        ("0.1 + 0.2 == 0.3".to_owned(), "false"),
        ("2.0 == 2".to_owned(), "true"),
        ("2 != 2.0".to_owned(), "false"),
    ];
    let expressions: Vec<&str> = cases.iter().map(|(e, _)| e.as_str()).collect();
    let expected: Vec<&str> = cases.iter().map(|(_, r)| *r).collect();
    assert_eq!(evaluated(&expressions), expected);
}

#[test]
fn a_clamp_written_in_rhai_reaches_its_bound() {
    let mut runtime = started(
        r#"
        fn clamp(v, lo, hi) { if v < lo { lo } else if v > hi { hi } else { v } }
        fn tick() {
            log::info(clamp(30.989949493661165 - 0.98994949366116647, 30, 200));
        }
        "#,
    );
    runtime.tick(NO_INPUT).unwrap();
    assert_eq!(texts(runtime.take_output()), ["30"]);
}
