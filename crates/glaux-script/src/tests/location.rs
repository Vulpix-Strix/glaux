use pretty_assertions::assert_eq;

use crate::{RequireError, RequireFailure, ScriptError};

#[test]
fn runtime_and_syntax_errors_give_their_leading_position() {
    let runtime = ScriptError::Runtime("code/main.luau:12: boom".into());
    let syntax = ScriptError::Syntax("main.rhai:3: Unexpected token".into());
    assert_eq!(runtime.location(), Some(("code/main.luau", 12)));
    assert_eq!(syntax.location(), Some(("main.rhai", 3)));
}

#[test]
fn a_failed_require_gives_the_line_that_called_it() {
    let error = ScriptError::Require(Box::new(RequireFailure {
        at: Some("code/main.luau:7".into()),
        error: RequireError::NotFound("code/x.luau".into()),
    }));
    assert_eq!(error.location(), Some(("code/main.luau", 7)));
}

#[test]
fn messages_without_a_position_give_none() {
    for error in [
        ScriptError::Runtime("attempt to call a nil value".into()),
        ScriptError::Runtime("main.luau: no line here".into()),
        ScriptError::Runtime("main.luau:0: not a line".into()),
        ScriptError::Runtime("Error: oops:3: spaced".into()),
        ScriptError::LimitExceeded,
        ScriptError::ModuleLimitExceeded("code/x.luau".into()),
        ScriptError::Require(Box::new(RequireFailure {
            at: None,
            error: RequireError::Cycle(vec!["a".into()]),
        })),
    ] {
        assert_eq!(error.location(), None, "{error:?}");
    }
}
