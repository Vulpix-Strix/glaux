use ScriptLanguage::{Luau, Rhai};
use pretty_assertions::assert_eq;

use super::*;

fn state(language: ScriptLanguage, text: &str) -> State {
    scan(language, text).state
}

#[test]
fn luau_comments() {
    assert_eq!(state(Luau, "x = 1 -- co"), State::Comment);
    assert_eq!(state(Luau, "-- note\nco"), State::Code);
    assert_eq!(state(Luau, "--[[ co"), State::Comment);
    assert_eq!(state(Luau, "--[==[ ]] co"), State::Comment);
    assert_eq!(state(Luau, "--[==[ ]] ]==] co"), State::Code);
    assert_eq!(state(Luau, "--[[ one\ntwo ]] co"), State::Code);
    // A comment that only looks like a block comment is a line comment.
    assert_eq!(state(Luau, "--[ co\nco"), State::Code);
    assert_eq!(state(Luau, "--[ co"), State::Comment);
}

#[test]
fn luau_strings() {
    assert_eq!(state(Luau, "x = \"co"), State::String);
    assert_eq!(state(Luau, "x = \"a\" .. co"), State::Code);
    assert_eq!(state(Luau, "x = 'it\\'s co"), State::String);
    // An unfinished Luau string ends with its line.
    assert_eq!(state(Luau, "x = \"co\nco"), State::Code);
    assert_eq!(state(Luau, "x = [[co"), State::String);
    assert_eq!(state(Luau, "x = [=[ ]] co"), State::String);
    assert_eq!(state(Luau, "x = [[a\nb]] co"), State::Code);
    assert_eq!(state(Luau, "x = `a co"), State::Template);
    assert_eq!(state(Luau, "x = `a` .. co"), State::Code);
}

#[test]
fn luau_markers_inside_strings_and_comments_are_inert() {
    let text = "-- \"\nlocal s = \"--[[\" .. [[ ` ]]\nco";
    assert_eq!(state(Luau, text), State::Code);
}

#[test]
fn rhai_comments_and_strings() {
    assert_eq!(state(Rhai, "x = 1; // co"), State::Comment);
    assert_eq!(state(Rhai, "// note\nco"), State::Code);
    assert_eq!(state(Rhai, "/* co"), State::Comment);
    assert_eq!(state(Rhai, "/* a /* b */ co"), State::Comment);
    assert_eq!(state(Rhai, "/* a /* b */ c */ co"), State::Code);
    assert_eq!(state(Rhai, "x = \"co"), State::String);
    // A Rhai string may span lines. A char literal may not.
    assert_eq!(state(Rhai, "x = \"a\nco"), State::String);
    assert_eq!(state(Rhai, "x = 'a\nco"), State::Code);
    assert_eq!(state(Rhai, "x = `a ${co"), State::Template);
    assert_eq!(state(Rhai, "x = `a` + co"), State::Code);
    assert_eq!(state(Rhai, "x = #\"a \" co"), State::String);
    assert_eq!(state(Rhai, "x = #\"a\"# + co"), State::Code);
    assert_eq!(state(Rhai, "x = ##\"a\"# co"), State::String);
}

#[test]
fn rhai_division_and_map_literals_are_code() {
    assert_eq!(state(Rhai, "x = a / b; let m = #{ a: 1 }; co"), State::Code);
}

#[test]
fn luau_blocks_open_and_close() {
    let blocks = |text| scan(Luau, text).blocks;
    assert_eq!(blocks(""), 0);
    assert_eq!(blocks("function f()\n"), 1);
    assert_eq!(blocks("function f()\nend\n"), 0);
    assert_eq!(blocks("if a then\n  for i = 1, 2 do\n"), 2);
    assert_eq!(blocks("if a then\nelseif b then\nelse\nend\n"), 0);
    assert_eq!(blocks("while a do\n  repeat\n"), 2);
    assert_eq!(blocks("repeat\nuntil a\n"), 0);
    assert_eq!(blocks("do\n"), 1);
    assert_eq!(blocks("end\nend\n"), 0);
}

#[test]
fn luau_if_expressions_open_no_block() {
    let blocks = |text| scan(Luau, text).blocks;
    assert_eq!(blocks("local a = if x then 1 else 2\n"), 0);
    assert_eq!(blocks("f(if x then 1 else 2)\n"), 0);
    assert_eq!(blocks("return if x then 1 else 2\n"), 0);
    assert_eq!(blocks("local a = if x then 1 else 2\nif y then\n"), 1);
}

#[test]
fn luau_keywords_inside_strings_comments_and_fields_do_not_count() {
    let blocks = |text| scan(Luau, text).blocks;
    assert_eq!(blocks("local s = \"function\" -- do\n"), 0);
    assert_eq!(blocks("local s = [[ if ]]\n"), 0);
    assert_eq!(blocks("x = t.end + t:do_it()\n"), 0);
    assert_eq!(blocks("local functions, doing = 1, 2\n"), 0);
}

#[test]
fn rhai_blocks_are_braces() {
    let blocks = |text| scan(Rhai, text).blocks;
    assert_eq!(blocks("fn f() {\n"), 1);
    assert_eq!(blocks("fn f() {\n  if x {\n  }\n"), 1);
    assert_eq!(blocks("fn f() {}\n"), 0);
    assert_eq!(blocks("let s = \"{\"; // {\n"), 0);
}

#[test]
fn groups_count_open_brackets() {
    assert_eq!(scan(Luau, "f(a,\n").groups, 1);
    assert_eq!(scan(Luau, "local t = {\n  a = f(\n").groups, 2);
    assert_eq!(scan(Luau, "local t = { a = 1 }\n").groups, 0);
    assert_eq!(scan(Rhai, "let a = [1,\n").groups, 1);
    // Rhai braces are blocks, not groups.
    assert_eq!(scan(Rhai, "fn f() {\n").groups, 0);
}
