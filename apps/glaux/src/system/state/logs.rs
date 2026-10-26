//! Feeding the log panel and giving it the input it takes.

use super::Context;
use crate::logging::Diagnostics;
use crate::system::logs::{LogStore, PanelAction, Severity, SourceLocation};
use glaux_input::InputEvent;

/// Most diagnostics taken into the log in one tick. The rest wait for the
/// next, in a channel that drops what does not fit.
const DIAGNOSTICS_PER_TICK: usize = 32;

/// Moves the diagnostics that arrived into the log store.
pub(in crate::system) fn receive_diagnostics(diagnostics: &Diagnostics, logs: &mut LogStore) {
    for diagnostic in diagnostics.drain(DIAGNOSTICS_PER_TICK) {
        let severity = match diagnostic.level {
            tracing::Level::ERROR => Severity::Error,
            tracing::Level::WARN => Severity::Warn,
            tracing::Level::INFO => Severity::Info,
            tracing::Level::DEBUG => Severity::Debug,
            _ => Severity::Trace,
        };
        logs.push_diagnostic(severity, diagnostic.message, diagnostic.fields);
    }
    let dropped = diagnostics.take_dropped();
    if dropped > 0 {
        logs.push_skipped(dropped);
    }
}

/// What is left of a tick's input after the log panel took its share.
pub(in crate::system) struct Routed<'a> {
    /// The events for whatever is under the panel.
    pub(in crate::system) forward: std::borrow::Cow<'a, [InputEvent]>,
    /// The panel opened or closed in this tick.
    pub(in crate::system) toggled: bool,
    /// The script line the user asked to show, which ends the events taken.
    pub(in crate::system) jump: Option<SourceLocation>,
}

/// Gives the log panel the input it takes: everything while it is open, and
/// the key that opens it. A tick the panel has no part in is passed on
/// untouched and unallocated.
pub(in crate::system) fn route_logs<'a>(
    ctx: &mut Context<'_>,
    input: &'a [InputEvent],
    can_jump: bool,
) -> Routed<'a> {
    if !ctx.log_panel.is_open() && !input.contains(&InputEvent::ToggleLogs) {
        return Routed {
            forward: input.into(),
            toggled: false,
            jump: None,
        };
    }
    let mut forward = Vec::new();
    let mut toggled = false;
    let mut jump = None;
    for &event in input {
        if ctx.log_panel.is_open() {
            let was_open = true;
            if let PanelAction::Jump(location) = ctx.log_panel.input(event, ctx.logs, can_jump) {
                jump = Some(location);
                break;
            }
            toggled |= was_open != ctx.log_panel.is_open();
        } else if event == InputEvent::ToggleLogs {
            ctx.log_panel.toggle();
            toggled = true;
        } else {
            forward.push(event);
        }
    }
    Routed {
        forward: forward.into(),
        toggled,
        jump,
    }
}
