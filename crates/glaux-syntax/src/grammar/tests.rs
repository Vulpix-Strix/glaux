use super::*;
use crate::{Highlighter, SyntaxKind};

/// The grammars under test.
const GRAMMARS: [Grammar; 1] = [Grammar::Luau];

/// One program per grammar that uses every reserved word and literal. A word alone does
/// not parse as a keyword, because most are contextual in the grammar.
const LUAU: &str = "export type T = number\n\
    local function f(a) if a and not a or a then return nil elseif true then else end end\n\
    for i in x do break end\n\
    while false do continue end\n\
    repeat until x\n\
    do end\n\
    local t = 1";

fn words_styled_as(grammar: Grammar, program: &str, kind: SyntaxKind) -> Vec<String> {
    let mut highlighter = Highlighter::new(grammar).expect("bundled query compiles");
    highlighter.update(program);
    highlighter
        .spans(0..program.len())
        .into_iter()
        .filter(|span| span.kind == kind)
        .map(|span| program[span.range].to_owned())
        .collect()
}

fn program(grammar: Grammar) -> &'static str {
    match grammar {
        Grammar::Luau => LUAU,
    }
}

#[test]
fn every_keyword_is_styled_as_a_keyword() {
    for grammar in GRAMMARS {
        let styled = words_styled_as(grammar, program(grammar), SyntaxKind::Keyword);
        for word in grammar.keywords() {
            assert!(styled.iter().any(|s| s == word), "{grammar:?} `{word}`");
        }
    }
}

#[test]
fn every_literal_is_styled_as_a_constant() {
    for grammar in GRAMMARS {
        let styled = words_styled_as(grammar, program(grammar), SyntaxKind::Constant);
        for word in grammar.literals() {
            assert!(styled.iter().any(|s| s == word), "{grammar:?} `{word}`");
        }
    }
}

#[test]
fn word_lists_are_sorted_and_unique() {
    for grammar in GRAMMARS {
        for list in [grammar.keywords(), grammar.literals()] {
            assert!(
                list.windows(2).all(|w| w[0] < w[1]),
                "{grammar:?}: {list:?}"
            );
        }
    }
}
