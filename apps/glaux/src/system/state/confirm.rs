//! The question about unsaved edits that holds up quitting or a shell command.

use super::running::pause;
use super::shell::shell_request;
use super::{Context, cartridge_of, report};
use crate::system::shell::CartridgeSlot;
use crate::system::unsaved::{Edits, Outcome, Pending, UnsavedFlow};
use crate::system::{PausedView, SystemState};
use glaux_input::InputEvent;

/// Asks what to do with unsaved edits before quitting, or confirms
/// quitting at once without any. A running cartridge is paused and the
/// question asked over the shell, so cancelling loses nothing. An
/// unfinished sprite stroke is ended first so its edit is saved whole.
pub(in crate::system) fn guard_exit(
    state: SystemState,
    ctx: &mut Context<'_>,
    exiting: &mut bool,
) -> SystemState {
    if ctx.unsaved.is_some() {
        return state;
    }
    if matches!(state, SystemState::Workspace { .. }) {
        ctx.workspace.end_stroke(ctx.sprites.as_mut());
    }
    let Some(flow) = UnsavedFlow::new(Pending::Exit, ctx.code.as_ref(), ctx.sprites.as_ref())
    else {
        *exiting = true;
        return state;
    };
    *ctx.unsaved = Some(flow);
    let (shell, running) = match state {
        SystemState::Running { shell, running } => match pause(shell, running, ctx) {
            SystemState::Paused { shell, running, .. } => (shell, running),
            state => return state,
        },
        SystemState::Paused { shell, running, .. } => (shell, running),
        state => return state,
    };
    SystemState::Paused {
        shell,
        running,
        view: PausedView::Shell,
    }
}

/// Feeds `input` to the open question about unsaved edits, and once it is
/// answered carries out what waited on it. Events after the answer are
/// dropped, as after any change of screen.
pub(in crate::system) fn unsaved_tick(
    mut state: SystemState,
    input: &[InputEvent],
    ctx: &mut Context<'_>,
    exiting: &mut bool,
) -> SystemState {
    for &event in input {
        let Some(flow) = ctx.unsaved.as_mut() else {
            break;
        };
        let whole_screen = !matches!(state, SystemState::Workspace { .. });
        let layout = ctx.workspace.dialog_layout(flow.dialog(), whole_screen);
        let Some(cartridge) = cartridge_of(&state) else {
            // Unsaved edits always belong to a cartridge the system holds.
            *ctx.unsaved = None;
            break;
        };
        let edits = Edits {
            cartridge,
            code: ctx.code.get_or_insert_default(),
            sprites: ctx.sprites.as_mut(),
            defaults: ctx.video_defaults,
        };
        match flow.handle_input(event, &layout, edits) {
            Outcome::Open => continue,
            Outcome::Stopped(message) => {
                tracing::info!("unsaved edits kept, nothing went ahead");
                if let Some(message) = message {
                    report(&mut state, ctx.workspace, message);
                }
            }
            Outcome::Proceed(Pending::Exit) => {
                tracing::info!("quitting");
                *exiting = true;
            }
            Outcome::Proceed(Pending::Command { line, .. }) => {
                *ctx.unsaved = None;
                state = run_confirmed(state, &line, ctx);
            }
        }
        *ctx.unsaved = None;
        break;
    }
    if let SystemState::Workspace { video, .. } = &mut state
        && let Some(document) = ctx.sprites.as_ref()
    {
        ctx.workspace.refresh_video(video, document);
    }
    state
}

/// Runs the shell command `line` once its unsaved edits were saved or
/// discarded.
pub(super) fn run_confirmed(state: SystemState, line: &str, ctx: &mut Context<'_>) -> SystemState {
    let SystemState::Shell { mut shell, loaded } = state else {
        tracing::warn!("a confirmed shell command found no shell to run in");
        return state;
    };
    let slot = CartridgeSlot::loaded(loaded.as_ref(), ctx.has_unsaved());
    let request = shell.run_confirmed(line, slot);
    shell_request(shell, loaded, request, &[], ctx).0
}
