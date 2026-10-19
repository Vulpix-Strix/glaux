use ScriptLanguage::{Luau, Rhai};
use glaux_docs::Library;
use pretty_assertions::assert_eq;

use super::*;

fn run_with(
    language: ScriptLanguage,
    marked: &str,
    trigger: Trigger,
    line_ending: &str,
    indent: &str,
) -> Completions {
    let caret = marked.find('|').expect("the text marks the caret with `|`");
    let text = marked.replacen('|', "", 1);
    let mut syntax = Highlighter::new(grammar(language)).expect("bundled query compiles");
    syntax.update(&text);
    let request = Request {
        language,
        text: &text,
        caret,
        revision: 7,
        trigger,
        line_ending,
        indent,
    };
    complete(&request, Library::embedded().api(), &syntax)
}

fn run(language: ScriptLanguage, marked: &str, trigger: Trigger) -> Completions {
    run_with(language, marked, trigger, "\n", "  ")
}

fn labels(completions: &Completions) -> Vec<&str> {
    completions.items.iter().map(|c| c.label.as_str()).collect()
}

fn find<'a>(completions: &'a Completions, label: &str) -> &'a Candidate {
    completions
        .items
        .iter()
        .find(|c| c.label == label)
        .unwrap_or_else(|| panic!("no candidate `{label}` in {:?}", labels(completions)))
}

#[test]
fn namespace_members_come_from_the_catalog() {
    let luau = run(Luau, "audio.|", Trigger::Member);
    assert_eq!(labels(&luau), ["env", "gate", "tone"]);
    assert_eq!(find(&luau, "tone").kind, CandidateKind::Function);
    assert_eq!(find(&luau, "tone").detail, "audio");
    assert_eq!(luau.revision, 7);
    assert_eq!(luau.anchor, 6..6);

    let rhai = run(Rhai, "audio::|", Trigger::Member);
    assert_eq!(labels(&rhai), ["env", "gate", "tone"]);
}

#[test]
fn the_separator_must_belong_to_the_language() {
    assert_eq!(run(Luau, "audio::|", Trigger::Manual).items, []);
    assert_eq!(run(Rhai, "audio.|", Trigger::Manual).items, []);
    assert_eq!(run(Luau, "nothing.|", Trigger::Manual).items, []);
    assert_eq!(run(Luau, "f().|", Trigger::Manual).items, []);
}

#[test]
fn constant_groups_list_their_members() {
    let keys = run(Luau, "Key.|", Trigger::Member);
    assert!(labels(&keys).contains(&"PageUp"));
    assert_eq!(find(&keys, "Space").kind, CandidateKind::Constant);
    assert_eq!(find(&keys, "Space").detail, "Key");
    assert_eq!(
        labels(&run(Rhai, "Button::|", Trigger::Member)),
        ["A", "B", "Down", "Left", "Right", "Up"]
    );
}

#[test]
fn broken_code_after_the_separator_still_completes() {
    let luau = run(Luau, "audio.|\nend", Trigger::Member);
    assert_eq!(labels(&luau), ["env", "gate", "tone"]);
    assert!(labels(&run(Luau, "if key(Key.|\nend", Trigger::Member)).contains(&"Space"));
    assert!(labels(&run(Rhai, "if key(Key::|", Trigger::Member)).contains(&"Space"));
    assert_eq!(
        labels(&run(Rhai, "audio::|", Trigger::Member)),
        ["env", "gate", "tone"]
    );
}

#[test]
fn members_of_a_shadowed_namespace_are_unknown() {
    assert_eq!(
        run(Luau, "local audio = {}\naudio.|", Trigger::Manual).items,
        []
    );
}

#[test]
fn matches_rank_by_how_closely_they_follow_what_was_typed() {
    let members = run(Luau, "Key.Pa|", Trigger::Manual);
    // Prefix matches first, then the names that merely contain the letters in order.
    assert_eq!(&labels(&members)[..2], ["PageDown", "PageUp"]);
    assert!(labels(&members).contains(&"Space"));

    let lowercase = run(Luau, "Key.up|", Trigger::Manual);
    assert_eq!(labels(&lowercase)[0], "Up");
    assert_eq!(find(&lowercase, "PageUp").origin, Origin::Api);
}

#[test]
fn locals_rank_before_api_names_and_keywords() {
    let found = run(Luau, "local color_a = 1\nco|", Trigger::Auto);
    let order: Vec<_> = found.items.iter().map(|c| c.origin).collect();
    assert!(order.windows(2).all(|pair| pair[0] <= pair[1]), "{order:?}");
    // `color_a` matches `co` by prefix, like `continue`, but it is local.
    assert_eq!(labels(&found)[0], "color_a");
    assert_eq!(find(&found, "color_a").detail, "local");
}

#[test]
fn a_local_hides_the_api_name_it_shares() {
    let found = run(Luau, "local print = 1\npri|", Trigger::Auto);
    let prints: Vec<_> = found.items.iter().filter(|c| c.label == "print").collect();
    assert_eq!(prints.len(), 1);
    assert_eq!(prints[0].origin, Origin::Local);
}

#[test]
fn api_names_follow_the_language() {
    let luau = run(Luau, "pri|", Trigger::Auto);
    assert!(labels(&luau).contains(&"print"));
    assert!(!labels(&run(Luau, "tex|", Trigger::Auto)).contains(&"text"));
    assert!(labels(&run(Rhai, "tex|", Trigger::Auto)).contains(&"text"));
    assert!(labels(&run(Luau, "requ|", Trigger::Auto)).contains(&"require"));
    assert!(!labels(&run(Rhai, "requ|", Trigger::Auto)).contains(&"require"));

    let import = run(Rhai, "impo|", Trigger::Auto);
    let import = find(&import, "import");
    assert_eq!(import.origin, Origin::Api);
    assert_eq!(import.kind, CandidateKind::Keyword);
}

#[test]
fn keywords_and_literals_are_offered_by_language() {
    let luau = run(Luau, "ni|", Trigger::Auto);
    assert_eq!(find(&luau, "nil").kind, CandidateKind::Constant);
    assert!(!labels(&run(Rhai, "ni|", Trigger::Auto)).contains(&"nil"));
    let rhai = run(Rhai, "le|", Trigger::Auto);
    assert_eq!(find(&rhai, "let").kind, CandidateKind::Keyword);
    assert!(!labels(&run(Luau, "le|", Trigger::Auto)).contains(&"let"));
}

#[test]
fn locals_respect_scope_and_shadowing() {
    let found = run(
        Luau,
        "local x = 1\nfunction f(x)\n  |\nend",
        Trigger::Manual,
    );
    let xs: Vec<_> = found.items.iter().filter(|c| c.label == "x").collect();
    assert_eq!(xs.len(), 1);
    assert_eq!(xs[0].detail, "param");

    let outer = run(
        Luau,
        "function f()\n  local inner = 1\nend\nin|",
        Trigger::Auto,
    );
    assert!(!labels(&outer).contains(&"inner"));

    let rhai = run(Rhai, "let outer = 1;\nfn f() { ou| }", Trigger::Auto);
    assert!(!labels(&rhai).contains(&"outer"));
}

#[test]
fn a_local_function_documents_itself() {
    let found = run(Luau, "local function area(w, h) end\nar|", Trigger::Auto);
    let Doc::Local(definition) = &find(&found, "area").doc else {
        panic!("a local function carries its definition");
    };
    assert_eq!(definition.params, ["w", "h"]);
    assert_eq!(find(&found, "area").kind, CandidateKind::Function);
}

#[test]
fn api_candidates_point_to_their_documentation() {
    let found = run(Luau, "rectf|", Trigger::Auto);
    let rectfill = find(&found, "rectfill");
    assert_eq!(
        rectfill.doc,
        Doc::Api(DocId::parse("api/graphics/rectfill").unwrap())
    );
    assert_eq!(rectfill.detail, "graphics");
    assert_eq!(rectfill.insertion, Insertion::Name("rectfill".to_owned()));
}

#[test]
fn the_trigger_decides_when_to_answer() {
    assert_eq!(run(Luau, "c|", Trigger::Auto).items, []);
    assert!(!run(Luau, "co|", Trigger::Auto).items.is_empty());
    assert!(!run(Luau, "c|", Trigger::Manual).items.is_empty());
    assert!(!run(Luau, "|", Trigger::Manual).items.is_empty());
    assert_eq!(run(Luau, "x = |", Trigger::Auto).items, []);
    // A member trigger only answers right after a separator.
    assert_eq!(run(Luau, "co|", Trigger::Member).items, []);
    assert_eq!(run(Luau, "x = |", Trigger::Member).items, []);
    assert!(!run(Luau, "audio.|", Trigger::Member).items.is_empty());
}

#[test]
fn manual_completion_works_wherever_the_caret_is() {
    // The engine keeps no history, so a paste, a delete or a cursor move before the
    // request changes nothing about the answer.
    for marked in ["audio.|", "audio.to|", "x = |", "x = pr|int", "Key.|\nend"] {
        let found = run(Luau, marked, Trigger::Manual);
        assert!(!found.items.is_empty(), "{marked}");
    }
}

#[test]
fn the_word_around_the_caret_is_replaced_whole() {
    let found = run(Luau, "x = pri|nt(1)", Trigger::Manual);
    assert_eq!(found.anchor, 4..9);
    assert_eq!(found.prefix, "pri");
    assert_eq!(labels(&found)[0], "print");
}

#[test]
fn the_only_candidate_equal_to_the_text_is_not_worth_showing() {
    assert_eq!(run(Luau, "audio.tone|", Trigger::Auto).items, []);
    assert_eq!(run(Luau, "audio.tone|", Trigger::Member).items, []);
    assert_eq!(labels(&run(Luau, "audio.tone|", Trigger::Manual)), ["tone"]);
}

#[test]
fn comments_strings_and_numbers_offer_nothing() {
    for (language, marked) in [
        (Luau, "-- co|"),
        (Luau, "--[[ audio.|"),
        (Luau, "x = \"co|"),
        (Luau, "x = [[co|"),
        (Luau, "x = `co|"),
        (Luau, "x = 12|"),
        (Rhai, "// co|"),
        (Rhai, "/* audio::|"),
        (Rhai, "x = \"co|"),
    ] {
        assert_eq!(run(language, marked, Trigger::Manual).items, [], "{marked}");
    }
}

#[test]
fn interpolated_code_completes() {
    assert!(labels(&run(Luau, "x = `a {pri|} b`", Trigger::Auto)).contains(&"print"));
    assert!(labels(&run(Rhai, "x = `a ${tex|} b`", Trigger::Auto)).contains(&"text"));
    assert_eq!(run(Luau, "x = `a pri|`", Trigger::Auto).items, []);
}

#[test]
fn declaration_names_offer_nothing() {
    for (language, marked) in [
        (Luau, "local co|"),
        (Luau, "local function co|"),
        (Luau, "function f(a, co|) end"),
        (Rhai, "let co|"),
        (Rhai, "fn f(a, co|) {}"),
    ] {
        assert_eq!(run(language, marked, Trigger::Manual).items, [], "{marked}");
    }
}

#[test]
fn callbacks_are_declared_whole_at_the_start_of_a_top_level_statement() {
    let found = run(Luau, "local x = 1\ndr|", Trigger::Auto);
    let draw = find(&found, "draw");
    assert_eq!(draw.kind, CandidateKind::Callback);
    assert_eq!(draw.detail, "callback");
    let Insertion::Declaration {
        text,
        caret_in_text,
    } = &draw.insertion
    else {
        panic!("a callback is declared");
    };
    assert_eq!(text, "function draw()\n  \nend");
    assert_eq!(&text[..*caret_in_text], "function draw()\n  ");

    let found = run(Rhai, "dr|", Trigger::Auto);
    let Insertion::Declaration {
        text,
        caret_in_text,
    } = &find(&found, "draw").insertion
    else {
        panic!("a callback is declared");
    };
    assert_eq!(text, "fn draw() {\n  \n}");
    assert_eq!(&text[..*caret_in_text], "fn draw() {\n  ");
}

#[test]
fn declarations_follow_the_documents_line_ending_and_indent() {
    let found = run_with(Luau, "  dr|", Trigger::Auto, "\r\n", "\t");
    let Insertion::Declaration {
        text,
        caret_in_text,
    } = &find(&found, "draw").insertion
    else {
        panic!("a callback is declared");
    };
    assert_eq!(text, "function draw()\r\n  \t\r\n  end");
    assert_eq!(&text[..*caret_in_text], "function draw()\r\n  \t");
}

#[test]
fn a_typed_function_keyword_is_not_repeated() {
    let found = run(Luau, "function dr|", Trigger::Auto);
    assert_eq!(labels(&found), ["draw"]);
    assert_eq!(found.anchor, 9..11);
    let Insertion::Declaration { text, .. } = &found.items[0].insertion else {
        panic!("a callback is declared");
    };
    assert_eq!(text, "draw()\n  \nend");

    let found = run(Rhai, "fn dr|", Trigger::Auto);
    let Insertion::Declaration { text, .. } = &found.items[0].insertion else {
        panic!("a callback is declared");
    };
    assert_eq!(text, "draw() {\n  \n}");

    // The body is indented from the line that has the keyword.
    let found = run(Luau, "  function dr|", Trigger::Auto);
    let Insertion::Declaration { text, .. } = &found.items[0].insertion else {
        panic!("a callback is declared");
    };
    assert_eq!(text, "draw()\n    \n  end");
}

#[test]
fn only_the_missing_callbacks_are_offered() {
    let found = run(Luau, "function tick() end\n|", Trigger::Manual);
    let callbacks: Vec<_> = found
        .items
        .iter()
        .filter(|c| c.kind == CandidateKind::Callback)
        .map(|c| c.label.as_str())
        .collect();
    assert_eq!(callbacks, ["boot", "draw"]);

    let found = run(Luau, "function ti|", Trigger::Auto);
    assert_eq!(labels(&found), ["tick"]);
    let found = run(Luau, "function tick() end\nfunction ti|", Trigger::Auto);
    assert!(found.items.is_empty());
}

#[test]
fn local_and_private_functions_do_not_count_as_callbacks() {
    let declares_tick = |language, marked| {
        let found = run(language, marked, Trigger::Auto);
        found
            .items
            .iter()
            .any(|c| c.label == "tick" && c.kind == CandidateKind::Callback)
    };
    assert!(declares_tick(Luau, "local function tick() end\nti|"));
    assert!(declares_tick(Rhai, "private fn tick() {}\nti|"));
    assert!(!declares_tick(Luau, "function tick() end\nti|"));
    assert!(!declares_tick(Rhai, "fn tick() {}\nti|"));
}

#[test]
fn callbacks_are_not_offered_outside_the_top_level() {
    for (language, marked) in [
        (Luau, "function f()\n  dr|"),
        (Luau, "if a then\n  dr|\nend"),
        (Luau, "x = dr|"),
        (Luau, "f(a,\n  dr|"),
        (Luau, "function f()\n  function dr|"),
        (Luau, "local function dr|"),
        (Rhai, "fn f() {\n  dr|"),
        (Rhai, "x = dr|"),
        (Rhai, "private fn dr|"),
    ] {
        let found = run(language, marked, Trigger::Manual);
        assert!(
            found
                .items
                .iter()
                .all(|c| c.kind != CandidateKind::Callback),
            "{marked}: {:?}",
            labels(&found)
        );
    }
}

#[test]
fn a_reference_to_a_callback_name_is_not_a_declaration() {
    let found = run(Luau, "x = ti|", Trigger::Manual);
    assert!(!labels(&found).contains(&"tick"));
}

#[test]
fn nothing_is_offered_for_a_stale_tree_or_a_bad_caret() {
    let mut syntax = Highlighter::new(Grammar::Luau).expect("bundled query compiles");
    syntax.update("print");
    let library = Library::embedded();
    let api = library.api();
    let request = |text, caret| Request {
        language: Luau,
        text,
        caret,
        revision: 1,
        trigger: Trigger::Manual,
        line_ending: "\n",
        indent: "  ",
    };
    assert!(
        !complete(&request("print", 5), api, &syntax)
            .items
            .is_empty()
    );
    assert_eq!(complete(&request("print(", 6), api, &syntax).items, []);
    assert_eq!(complete(&request("print", 99), api, &syntax).items, []);
    let mut wrong_language = request("print", 5);
    wrong_language.language = Rhai;
    assert_eq!(complete(&wrong_language, api, &syntax).items, []);
}

#[test]
fn a_large_script_completes_with_the_right_scope() {
    let mut text = String::new();
    for i in 0..4000 {
        text.push_str(&format!(
            "local function helper_{i}(a, b)\n  local sum_{i} = a + b\n  return sum_{i}\nend\n"
        ));
    }
    assert!(text.len() > 200_000);
    text.push_str("local function last()\n  local mine = 1\n  hel|\nend");

    // Far more locals match than an answer holds.
    let found = run(Luau, &text, Trigger::Auto);
    assert_eq!(found.items.len(), MAX_CANDIDATES);
    assert!(found.items.iter().all(|c| c.origin == Origin::Local));
    assert!(!labels(&found).contains(&"sum_0"));
    let found = run(Luau, &text.replace("hel|", "mi|"), Trigger::Auto);
    assert_eq!(labels(&found)[0], "mine");
}

mod properties {
    use proptest::prelude::*;

    use super::*;

    proptest! {
        #[test]
        fn any_caret_in_any_text_is_answered_without_panicking(
            text in "[a-zA-Z0-9_ .:\\n\\t\"'`{}()\\[\\]#/*-]{0,80}",
            at in 0usize..100,
            rhai in any::<bool>(),
        ) {
            let language = if rhai { Rhai } else { Luau };
            let caret = at.min(text.len());
            let mut syntax = Highlighter::new(grammar(language)).expect("bundled query compiles");
            syntax.update(&text);
            let request = Request {
                language,
                text: &text,
                caret,
                revision: 0,
                trigger: Trigger::Manual,
                line_ending: "\n",
                indent: "  ",
            };
            let answer = complete(&request, Library::embedded().api(), &syntax);
            prop_assert!(answer.items.len() <= MAX_CANDIDATES);
            prop_assert!(answer.anchor.end <= text.len());
        }
    }
}
