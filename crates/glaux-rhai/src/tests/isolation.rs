//! What a script cannot reach: host files, `eval`, and modules outside
//! its cartridge.

use super::*;
use pretty_assertions::assert_eq;

#[test]
fn host_paths_are_never_handed_to_the_module_source() {
    for name in [
        "/etc/passwd",
        "../secret",
        "code/x",
        "C:\\x",
        "a/../../b",
        "~/.ssh/id",
    ] {
        let files = Files::default();
        let requested = Rc::clone(&files.requested);
        let failure = start_failure_with(&format!("import {name:?} as m;"), files);
        assert!(
            matches!(failure.error.error, ScriptError::Require(ref f) if matches!(f.error, glaux_script::RequireError::InvalidName { .. })),
            "{name}: {:?}",
            failure.error
        );
        assert!(requested.borrow().is_empty(), "{name}");
    }
}

#[test]
fn modules_resolve_only_through_the_cartridge_source() {
    // A file that exists on every host is still just a missing module.
    let failure = start_failure("import \"etc.passwd\" as p;");
    assert_eq!(
        failure.error.to_string(),
        "Module not found: code/etc/passwd.rhai (main.rhai:1)"
    );
}

#[test]
fn eval_is_unavailable() {
    assert_eq!(
        start_failure("eval(\"1\");").error,
        error(
            Phase::Entry,
            ScriptError::Syntax("main.rhai:1: reserved keyword 'eval' is disabled".into())
        )
    );
}

#[test]
fn print_and_debug_go_to_the_log_not_the_host() {
    // Rhai prints to standard output by default; captured output proves the
    // hooks replaced that.
    assert_eq!(start_output("print(\"a\"); debug(1);"), ["a", "1"]);
}
