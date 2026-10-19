//! Finding the call a caret is typing arguments for.
//!
//! A half-typed call is the common case while someone writes code, and neither grammar
//! parses it into a call node. Rhai keeps an `ExprCall` whose `)` is missing. Luau leaves
//! the loose tokens of the call, `name ( arg ,`, inside an `ERROR` node. Arguments that
//! are already complete are single nodes in both, so their commas never show up.

use tree_sitter::Node;

use crate::grammar::Grammar;
use crate::highlighter::Highlighter;

/// The call whose argument list holds the caret.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallSite {
    /// The callee as written, split at `.` and `::`. `audio.tone(` gives `["audio", "tone"]`.
    pub callee: Vec<String>,
    /// The callee is a Luau method (`object:method(`), whose receiver cannot be resolved.
    pub method: bool,
    /// The offset of the opening parenthesis.
    pub open: usize,
    /// The index of the argument holding the caret. `None` when the caret is inside the
    /// body of an anonymous function that has no `end` yet, where commas belong to the
    /// body and not to the call.
    pub active: Option<usize>,
}

impl Highlighter {
    /// The innermost call whose parentheses contain `offset`, or `None` when the caret is
    /// not typing arguments. Declarations such as `function f(a,` and `fn f(a,` do not
    /// count, and neither do grouping parentheses or calls on call results like `f(x)(`.
    pub fn call_at(&self, offset: usize) -> Option<CallSite> {
        let (root, source) = self.parsed()?;
        let grammar = self.grammar();
        let anchor = last_token_end(root, source, offset)?;
        let mut node = root.descendant_for_byte_range(anchor - 1, anchor)?;
        loop {
            let found = if is_arguments(node.kind()) {
                from_arguments(grammar, node, source, offset)
            } else if node.is_error() {
                from_loose_tokens(grammar, node, source, offset)
            } else {
                None
            };
            if found.is_some() {
                return found;
            }
            node = node.parent()?;
        }
    }
}

fn is_arguments(kind: &str) -> bool {
    matches!(kind, "arguments" | "ArgList")
}

/// The end of the last token before `offset`, skipping spaces and comments.
fn last_token_end(root: Node, source: &str, offset: usize) -> Option<usize> {
    let mut end = offset.min(source.len());
    loop {
        while end > 0 && source.as_bytes()[end - 1].is_ascii_whitespace() {
            end -= 1;
        }
        if end == 0 {
            return None;
        }
        let at = root.descendant_for_byte_range(end - 1, end)?;
        let mut top = at;
        while let Some(parent) = top.parent().filter(|p| is_comment(*p)) {
            top = parent;
        }
        if !is_comment(top) && at.kind() != "--" {
            return Some(end);
        }
        end = top.start_byte().min(at.start_byte());
    }
}

fn is_comment(node: Node) -> bool {
    node.kind().contains("comment")
}

/// A well-formed argument list, or one whose `)` is missing, around the caret.
fn from_arguments(grammar: Grammar, list: Node, source: &str, caret: usize) -> Option<CallSite> {
    let mut open = None;
    let mut commas = 0;
    let mut cursor = list.walk();
    for child in list.children(&mut cursor) {
        match child.kind() {
            "(" if open.is_none() => open = Some(child),
            ")" if !child.is_missing() && child.start_byte() < caret => return None,
            _ => {}
        }
        if child.end_byte() > caret {
            break;
        }
        if child.kind() == "," {
            commas += 1;
        } else if child.is_error() {
            let mut inner = child.walk();
            commas += child
                .children(&mut inner)
                .filter(|token| token.kind() == "," && token.end_byte() <= caret)
                .count();
        }
    }
    let open = open.filter(|open| open.end_byte() <= caret)?;
    let parent = list.parent()?;
    let mut siblings = parent.walk();
    let callee = parent
        .children(&mut siblings)
        .take_while(|child| child.id() != list.id())
        .last()?;
    callee_path(grammar, callee, source).map(|(callee, method)| CallSite {
        callee,
        method,
        open: open.start_byte(),
        active: Some(commas),
    })
}

/// One open bracket while walking the loose tokens of an `ERROR` node.
struct Frame {
    /// The callee, when the bracket is the parenthesis of a call.
    callee: Option<(Vec<String>, bool)>,
    open: usize,
    commas: usize,
    /// `function` tokens of Luau with no `end` yet.
    open_functions: usize,
}

fn from_loose_tokens(
    grammar: Grammar,
    error: Node,
    source: &str,
    caret: usize,
) -> Option<CallSite> {
    let mut stack: Vec<Frame> = Vec::new();
    let mut previous = error.prev_sibling();
    let mut cursor = error.walk();
    for token in error.children(&mut cursor) {
        if token.end_byte() > caret {
            break;
        }
        match token.kind() {
            "(" | "[" | "{" | "#{" => {
                let callee = (token.kind() == "(")
                    .then_some(previous)
                    .flatten()
                    .and_then(|callee| adjacent_callee(grammar, callee, token, source));
                stack.push(Frame {
                    callee,
                    open: token.start_byte(),
                    commas: 0,
                    open_functions: 0,
                });
            }
            ")" | "]" | "}" => {
                stack.pop();
            }
            "," => {
                if let Some(top) = stack.last_mut() {
                    top.commas += 1;
                }
            }
            "function" if grammar == Grammar::Luau => {
                if let Some(top) = stack.last_mut() {
                    top.open_functions += 1;
                }
            }
            "end" if grammar == Grammar::Luau => {
                if let Some(top) = stack.last_mut() {
                    top.open_functions = top.open_functions.saturating_sub(1);
                }
            }
            _ => {}
        }
        previous = Some(token);
    }
    stack.into_iter().rev().find_map(|frame| {
        let (callee, method) = frame.callee?;
        Some(CallSite {
            callee,
            method,
            open: frame.open,
            active: (frame.open_functions == 0).then_some(frame.commas),
        })
    })
}

/// The callee written right before `paren`, or `None` when something else precedes it.
fn adjacent_callee(
    grammar: Grammar,
    callee: Node,
    paren: Node,
    source: &str,
) -> Option<(Vec<String>, bool)> {
    let between = source.get(callee.end_byte()..paren.start_byte())?;
    if !between.trim().is_empty() || matches!(callee.kind(), "," | "(" | "[" | "{" | "#{") {
        return None;
    }
    callee_path(grammar, callee, source)
}

/// The path a callee node ends with, and whether it is a Luau method.
///
/// Returns `None` for a callee that is not a plain path, such as the result of another
/// call, a keyword (`return (`) or the name of a declaration (`function f(`).
fn callee_path(grammar: Grammar, callee: Node, source: &str) -> Option<(Vec<String>, bool)> {
    let text = source.get(callee.byte_range())?.trim_end();
    let start = text
        .rfind(|c: char| !(c.is_alphanumeric() || matches!(c, '_' | '.' | ':')))
        .map_or(0, |at| at + 1);
    let path = &text[start..];
    let first = path.chars().next()?;
    if !(first.is_alphabetic() || first == '_') || grammar.keywords().contains(&path) {
        return None;
    }
    let path_start = callee.start_byte() + (text.len() - path.len());
    if declares(&source[..path_start]) {
        return None;
    }
    let method = path.contains(':') && !path.contains("::");
    let segments = path
        .split(['.', ':'])
        .filter(|segment| !segment.is_empty())
        .map(str::to_owned)
        .collect();
    Some((segments, method))
}

/// Whether the word just before `before`'s end is `function` or `fn`, which makes the
/// parenthesis that follows a parameter list.
fn declares(before: &str) -> bool {
    let before = before.trim_end();
    let start = before
        .rfind(|c: char| !(c.is_alphanumeric() || c == '_'))
        .map_or(0, |at| at + 1);
    matches!(&before[start..], "function" | "fn")
}

#[cfg(test)]
mod tests;
