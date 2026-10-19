use pretty_assertions::assert_eq;

use super::*;
use DefinitionKind::*;

/// The character marking the caret: `|`, or `¦` for text that uses `|` itself.
fn marker(marked: &str) -> char {
    if marked.contains('¦') { '¦' } else { '|' }
}

/// Parses `marked` with its caret marker removed and returns the highlighter and the caret.
fn parse(grammar: Grammar, marked: &str) -> (Highlighter, usize) {
    let mark = marker(marked);
    let caret = marked.find(mark).expect("the text marks the caret");
    let text = marked.replacen(mark, "", 1);
    let mut highlighter = Highlighter::new(grammar).expect("bundled query compiles");
    highlighter.update(&text);
    (highlighter, caret)
}

fn visible(grammar: Grammar, marked: &str) -> Vec<(String, DefinitionKind)> {
    let (highlighter, caret) = parse(grammar, marked);
    highlighter
        .definitions_at(caret)
        .into_iter()
        .map(|d| (d.name, d.kind))
        .collect()
}

fn names(grammar: Grammar, marked: &str) -> Vec<String> {
    visible(grammar, marked).into_iter().map(|d| d.0).collect()
}

fn site(grammar: Grammar, marked: &str) -> Site {
    let (highlighter, caret) = parse(grammar, marked);
    let text = marked.replacen(marker(marked), "", 1);
    let start = text[..caret]
        .rfind(|c: char| !(c.is_alphanumeric() || c == '_'))
        .map_or(0, |at| at + 1);
    highlighter.site_at(start..caret)
}

#[test]
fn luau_local_is_visible_only_after_its_statement() {
    assert_eq!(names(Grammar::Luau, "local x = x|"), Vec::<String>::new());
    assert_eq!(names(Grammar::Luau, "local x = 1\n|"), ["x"]);
    assert_eq!(names(Grammar::Luau, "local x = 1\nlocal y = |"), ["x"]);
}

#[test]
fn luau_local_x_equals_x_reads_the_outer_x() {
    let (highlighter, caret) = parse(Grammar::Luau, "local x = 1\nlocal x = x|");
    let outer = highlighter.definitions_at(caret);
    assert_eq!(outer.len(), 1);
    assert_eq!(outer[0].declared, 6..7);
}

#[test]
fn luau_shadowing_keeps_the_latest_declaration() {
    let (highlighter, caret) = parse(Grammar::Luau, "local x = 1\nlocal x = 2\n|");
    let found = highlighter.definitions_at(caret);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].declared, 18..19);
}

#[test]
fn luau_inner_scope_comes_first_and_hides_the_outer_name() {
    let (highlighter, caret) = parse(
        Grammar::Luau,
        "local a = 1\nlocal b = 1\nfunction f(a)\n  local c = 1\n  |\nend",
    );
    let found = highlighter.definitions_at(caret);
    let names: Vec<_> = found.iter().map(|d| (d.name.as_str(), d.kind)).collect();
    assert_eq!(
        names,
        [
            ("c", Variable),
            ("a", Parameter),
            ("f", Function),
            ("b", Variable)
        ]
    );
}

#[test]
fn luau_local_function_sees_itself_and_keeps_its_parameters() {
    let (highlighter, caret) = parse(Grammar::Luau, "local function f(a, b)\n  |\nend");
    let found = highlighter.definitions_at(caret);
    let f = found.iter().find(|d| d.name == "f").expect("f is visible");
    assert_eq!(f.kind, Function);
    assert_eq!(f.params, ["a", "b"]);
    assert!(found.iter().any(|d| d.name == "a" && d.kind == Parameter));
}

#[test]
fn luau_parameters_and_locals_end_with_their_function() {
    assert_eq!(
        names(Grammar::Luau, "function f(a)\n  local b = 1\nend\n|"),
        ["f"]
    );
}

#[test]
fn luau_a_global_function_is_visible_before_it_is_written() {
    assert_eq!(
        names(Grammar::Luau, "local a = 1\n|\nfunction later() end"),
        ["later", "a"]
    );
}

#[test]
fn luau_methods_have_self() {
    assert_eq!(
        names(Grammar::Luau, "function M:bar(d)\n  |\nend"),
        ["d", "self"]
    );
    assert_eq!(names(Grammar::Luau, "function M.foo(d)\n  |\nend"), ["d"]);
}

#[test]
fn luau_loop_variables_exist_only_in_the_body() {
    assert_eq!(
        names(Grammar::Luau, "for i = 1, | do end"),
        Vec::<String>::new()
    );
    assert_eq!(names(Grammar::Luau, "for i = 1, 3 do\n  |\nend"), ["i"]);
    assert_eq!(
        names(Grammar::Luau, "for k, v in pairs(t) do\n  |\nend"),
        ["k", "v"]
    );
    assert_eq!(
        names(Grammar::Luau, "for i = 1, 3 do end\n|"),
        Vec::<String>::new()
    );
}

#[test]
fn luau_until_sees_the_locals_of_the_loop_body() {
    assert_eq!(
        names(Grammar::Luau, "repeat\n  local z = 1\nuntil z|"),
        ["z"]
    );
}

#[test]
fn luau_attribute_does_not_become_a_name() {
    assert_eq!(
        names(Grammar::Luau, "local a <const>, b = 1, 2\n|"),
        ["b", "a"]
    );
}

#[test]
fn luau_incomplete_member_access_keeps_earlier_locals() {
    let found = names(Grammar::Luau, "local k = 1\nif key(Key.|\nend");
    assert!(found.contains(&"k".to_owned()), "{found:?}");
}

#[test]
fn luau_a_name_being_declared_is_not_offered() {
    assert_eq!(names(Grammar::Luau, "local co|"), Vec::<String>::new());
}

#[test]
fn rhai_let_is_visible_after_its_statement() {
    assert_eq!(names(Grammar::Rhai, "let x = x|;"), Vec::<String>::new());
    assert_eq!(names(Grammar::Rhai, "let x = 1;\n|"), ["x"]);
    assert_eq!(names(Grammar::Rhai, "const Y = 1;\n|"), ["Y"]);
}

#[test]
fn rhai_functions_do_not_see_the_locals_around_them() {
    let found = visible(
        Grammar::Rhai,
        "let outer = 1;\nfn f(a) {\n  let inner = 1;\n  |\n}",
    );
    assert_eq!(
        found,
        [
            ("inner".to_owned(), Variable),
            ("a".to_owned(), Parameter),
            ("f".to_owned(), Function)
        ]
    );
}

#[test]
fn rhai_closures_capture_the_locals_around_them() {
    assert_eq!(
        names(Grammar::Rhai, "let outer = 1;\nlet c = |p, q| { ¦ };"),
        ["p", "q", "outer"]
    );
}

#[test]
fn rhai_functions_can_call_functions_defined_later() {
    assert_eq!(names(Grammar::Rhai, "fn a() { | }\nfn b() {}"), ["b", "a"]);
}

#[test]
fn rhai_function_parameters_are_recorded() {
    let (highlighter, caret) = parse(Grammar::Rhai, "fn add(a, b) { a + b }\n|");
    let found = highlighter.definitions_at(caret);
    assert_eq!(found[0].params, ["a", "b"]);
}

#[test]
fn rhai_for_and_catch_bindings() {
    assert_eq!(names(Grammar::Rhai, "for i in 0..3 {\n  |\n}"), ["i"]);
    assert_eq!(
        names(Grammar::Rhai, "for (k, v) in m {\n  |\n}"),
        ["k", "v"]
    );
    assert_eq!(names(Grammar::Rhai, "for i in | {}"), Vec::<String>::new());
    assert_eq!(
        names(Grammar::Rhai, "try { | } catch (e) { }"),
        Vec::<String>::new()
    );
    assert_eq!(names(Grammar::Rhai, "try { } catch (e) { | }"), ["e"]);
}

#[test]
fn rhai_import_alias_is_a_variable() {
    assert_eq!(names(Grammar::Rhai, "import \"m\" as mm;\n|"), ["mm"]);
}

#[test]
fn rhai_block_locals_end_with_the_block() {
    assert_eq!(
        names(Grammar::Rhai, "if true { let a = 1; }\n|"),
        Vec::<String>::new()
    );
}

#[test]
fn luau_global_functions_exclude_local_ones() {
    let (h, _) = parse(
        Grammar::Luau,
        "function tick() end\nlocal function draw() end\nfunction M.x() end|",
    );
    assert_eq!(h.global_functions(), ["tick"]);
}

#[test]
fn rhai_global_functions_exclude_private_ones() {
    let (h, _) = parse(Grammar::Rhai, "fn tick() {}\nprivate fn draw() {}|");
    assert_eq!(h.global_functions(), ["tick"]);
}

#[test]
fn declaration_names_are_recognised() {
    for (grammar, marked) in [
        (Grammar::Luau, "local co|"),
        (Grammar::Luau, "local a, co|"),
        (Grammar::Luau, "function f(a, co|) end"),
        (Grammar::Luau, "for co| = 1, 2 do end"),
        (Grammar::Luau, "for k, co| in pairs(t) do end"),
        (Grammar::Luau, "function co|() end"),
        (Grammar::Rhai, "let co|;"),
        (Grammar::Rhai, "const co| = 1;"),
        (Grammar::Rhai, "fn f(a, co|) {}"),
        (Grammar::Rhai, "let c = |p, co¦| p;"),
        (Grammar::Rhai, "for co| in x {}"),
        (Grammar::Rhai, "for (k, co|) in x {}"),
        (Grammar::Rhai, "try {} catch (co|) {}"),
        (Grammar::Rhai, "import \"m\" as co|;"),
    ] {
        assert!(site(grammar, marked).declares_name, "{grammar:?}: {marked}");
    }
}

#[test]
fn references_are_not_declaration_names() {
    for (grammar, marked) in [
        (Grammar::Luau, "local x = co|"),
        (Grammar::Luau, "x, co| = 1, 2"),
        (Grammar::Luau, "print(co|)"),
        (Grammar::Rhai, "let x = co|;"),
        (Grammar::Rhai, "print(co|);"),
    ] {
        assert!(
            !site(grammar, marked).declares_name,
            "{grammar:?}: {marked}"
        );
    }
}

#[test]
fn interpolation_holds_code() {
    assert!(site(Grammar::Luau, "x = `a {co|} b`").in_interpolation);
    assert!(!site(Grammar::Luau, "x = `a co| b`").in_interpolation);
    assert!(site(Grammar::Rhai, "x = `a ${co|} b`").in_interpolation);
    assert!(!site(Grammar::Rhai, "x = `a co| b`").in_interpolation);
}

#[test]
fn luau_a_half_typed_statement_keeps_the_block_open() {
    for marked in [
        "do\n  local mine = 1\n  mi|\nend",
        "if a then\n  local mine = 1\n  mi|\nend",
        "while a do\n  local mine = 1\n  mi|\nend",
    ] {
        assert_eq!(names(Grammar::Luau, marked), ["mine"], "{marked}");
    }
}

#[test]
fn luau_a_branch_does_not_see_the_locals_of_the_branch_before_it() {
    let marked = "if a then\n  local first = 1\nelse\n  local second = 1\n  |\nend";
    assert_eq!(names(Grammar::Luau, marked), ["second"]);
    let marked = "if a then\n  local first = 1\nelse\n  |\nend";
    assert_eq!(names(Grammar::Luau, marked), Vec::<String>::new());
}

#[test]
fn luau_elseif_branches_are_scopes_of_their_own() {
    let marked = "if a then\n  local first = 1\nelseif b then\n  local second = 1\n  |\nend";
    assert_eq!(names(Grammar::Luau, marked), ["second"]);
}
