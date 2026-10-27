use crate::ui_text;
use glaux_i18n::Arg;

/// Keeps VM diagnostics opaque while localizing Glaux's own headlines.
pub(in crate::system) fn runtime(error: &glaux_script::RuntimeError) -> String {
    use glaux_script::{RequireError, ScriptError};
    let callback = error.phase.callback();
    let phase = Arg::Text(if callback.is_some() {
        "callback"
    } else {
        "entry"
    });
    let callback = Arg::Text(callback.unwrap_or_default());
    match &error.error {
        ScriptError::Syntax(detail) => ui_text::get_with(
            "error-script-syntax",
            &[
                ("phase", phase),
                ("callback", callback),
                ("detail", Arg::Text(detail)),
            ],
        ),
        ScriptError::Runtime(detail) => ui_text::get_with(
            "error-script-runtime",
            &[
                ("phase", phase),
                ("callback", callback),
                ("detail", Arg::Text(detail)),
            ],
        ),
        ScriptError::LimitExceeded => ui_text::get_with(
            "error-script-limit",
            &[("phase", phase), ("callback", callback)],
        ),
        ScriptError::ModuleLimitExceeded(path) => ui_text::get_with(
            "error-script-module-limit",
            &[
                ("phase", phase),
                ("callback", callback),
                ("path", Arg::Text(path)),
            ],
        ),
        ScriptError::Stopped => ui_text::get("error-script-stopped"),
        ScriptError::Require(failure) => {
            let at = failure.at.as_deref();
            let (kind, detail) = match &failure.error {
                RequireError::InvalidName { name, reason } => {
                    ("invalid", format!("{name:?} - {}", module_name(reason)))
                }
                RequireError::NotFound(path) => ("missing", path.clone()),
                RequireError::Unreadable(path) => ("unreadable", path.clone()),
                RequireError::Cycle(chain) => ("cycle", chain.join(" -> ")),
            };
            ui_text::get_with(
                "error-script-require",
                &[
                    ("phase", phase),
                    ("callback", callback),
                    ("kind", Arg::Text(kind)),
                    ("detail", Arg::Text(&detail)),
                    ("has-at", Arg::Text(if at.is_some() { "yes" } else { "no" })),
                    ("at", Arg::Text(at.unwrap_or_default())),
                ],
            )
        }
    }
}

fn module_name(error: &glaux_modules::ModuleNameError) -> String {
    use glaux_modules::ModuleNameError;
    match error {
        ModuleNameError::Empty => ui_text::get("error-module-name-empty"),
        ModuleNameError::EmptySegment => ui_text::get("error-module-name-empty-segment"),
        ModuleNameError::LeadingDigit(value) => ui_text::get_with(
            "error-module-name-leading-digit",
            &[("segment", Arg::Text(value))],
        ),
        ModuleNameError::InvalidCharacter(value) => ui_text::get_with(
            "error-module-name-invalid-character",
            &[("character", Arg::Text(&format!("{:?}", value)))],
        ),
        ModuleNameError::HasExtension { stem, extension } => ui_text::get_with(
            "error-module-name-has-extension",
            &[
                ("stem", Arg::Text(stem)),
                ("extension", Arg::Text(extension)),
            ],
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::runtime;
    use glaux_script::{Phase, RequireError, RequireFailure, RuntimeError, ScriptError};
    use pretty_assertions::assert_eq;
    #[test]
    fn runtime_headlines_preserve_callbacks_locations_and_vm_diagnostics() {
        for (phase, expected) in [
            (Phase::Entry, "Runtime error: code/main.luau:3: boom {x}"),
            (
                Phase::Boot,
                "Runtime error in boot: code/main.luau:3: boom {x}",
            ),
            (
                Phase::Tick,
                "Runtime error in tick: code/main.luau:3: boom {x}",
            ),
            (
                Phase::Draw,
                "Runtime error in draw: code/main.luau:3: boom {x}",
            ),
        ] {
            assert_eq!(
                runtime(&RuntimeError {
                    phase,
                    error: ScriptError::Runtime("code/main.luau:3: boom {x}".into())
                }),
                expected
            );
        }
        let failure = RuntimeError {
            phase: Phase::Tick,
            error: ScriptError::Require(Box::new(RequireFailure {
                at: Some("code/main.luau:3".into()),
                error: RequireError::NotFound("code/game/player.luau".into()),
            })),
        };
        assert_eq!(
            runtime(&failure),
            "Module not found in tick: code/game/player.luau (code/main.luau:3)"
        );
        let module = RuntimeError {
            phase: Phase::Entry,
            error: ScriptError::ModuleLimitExceeded("code/loop.rhai".into()),
        };
        assert_eq!(
            runtime(&module),
            "Execution limit exceeded in code/loop.rhai"
        );
    }
}
