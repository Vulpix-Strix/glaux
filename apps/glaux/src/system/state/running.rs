//! Starting, running, pausing and stopping a cartridge.

use super::workspace::open_workspace;
use super::{Context, Documents};
use crate::cartridge::LoadedCartridge;
use crate::cartridge::source::SourcePath;
use crate::system::logs::{Category, LogStore, SourceLocation};
use crate::system::pause::PauseOverlay;
use crate::system::running::{FailedStart, RunOrigin, RunningCartridge, StartError};
use crate::system::shell::{Shell, cartridge_identity};
use crate::system::sprite_document::SpriteDocument;
use crate::system::{PausedView, StopReason, SystemState};
use crate::ui_text;
use glaux_i18n::Arg;
use glaux_input::InputEvent;
use glaux_script::ScriptError;

/// Moves what the cartridge logged into the log store and the shell.
fn drain_logs(shell: &mut Shell, logs: &mut LogStore, running: &mut RunningCartridge) {
    let entries = running.take_output();
    let dropped = running.take_dropped();
    if !entries.is_empty() || dropped > 0 {
        shell.log(logs.push_cartridge(entries, dropped));
    }
}

/// How a run ended in failure, as the editor shows it.
struct RunFailure {
    message: String,
    location: Option<SourceLocation>,
}

/// The script position `error` names, provided it is a script of
/// `cartridge`. Anything else is left unlocated rather than guessed.
fn verified_location(cartridge: &LoadedCartridge, error: &ScriptError) -> Option<SourceLocation> {
    let (file, line) = error.location()?;
    let path = SourcePath::parse(file).ok()?;
    cartridge.read_source(&path).ok()?;
    Some(SourceLocation { path, line })
}

/// What a failed script is about: its source not compiling, or its code
/// failing while it ran.
fn script_category(error: &ScriptError) -> Category {
    match error {
        ScriptError::Syntax(_) => Category::Compile,
        _ => Category::Runtime,
    }
}

/// Starts `cartridge` from `origin`, reporting its start or failure and
/// what it logged. A cartridge that fails to start stays loaded, in the
/// shell or in the Code view according to `origin`.
///
/// A run from the shell uses unsaved graphics changes instead of their
/// files. A run from Code uses the files only: saving what it should run
/// is [`save_before_running`]'s job.
pub(super) fn start(
    mut shell: Box<Shell>,
    cartridge: LoadedCartridge,
    origin: RunOrigin,
    ctx: &mut Context<'_>,
) -> SystemState {
    let identity = cartridge_identity(cartridge.manifest());
    let overrides = match origin {
        RunOrigin::Shell => ctx
            .sprites
            .as_ref()
            .map(SpriteDocument::unsaved)
            .unwrap_or_default(),
        RunOrigin::Code => Default::default(),
    };
    let edited = !overrides.is_empty();
    ctx.logs.begin_run();
    match RunningCartridge::start(cartridge, ctx.video_defaults, overrides) {
        Ok(running) => {
            let mut running = running.with_origin(origin);
            tracing::info!(cartridge = %identity, edited, ?origin, "cartridge started");
            if edited {
                shell.report(ui_text::get_with(
                    "system-running-with-unsaved-graphics-changes",
                    &[("identity", Arg::Text(&identity.to_string()))],
                ));
            } else {
                shell.report(ui_text::get_with(
                    "system-running",
                    &[("identity", Arg::Text(&identity.to_string()))],
                ));
            }
            shell.warn_if_experimental(running.cartridge().manifest().runtime.language);
            drain_logs(&mut shell, ctx.logs, &mut running);
            SystemState::Running { shell, running }
        }
        Err(failed) => {
            let FailedStart { cartridge, error } = *failed;
            let (message, category, location) = match error {
                StartError::Cartridge(err) => (
                    crate::system::ui_errors::cartridge(&err),
                    Category::Load,
                    None,
                ),
                StartError::Video(err) => (
                    crate::system::ui_errors::video::video_asset(&err),
                    Category::Load,
                    None,
                ),
                StartError::Script(failure) => {
                    tracing::info!(
                        cartridge = %identity,
                        error = %failure.error,
                        "cartridge failed to start"
                    );
                    shell.log(ctx.logs.push_cartridge(failure.output, failure.dropped));
                    (
                        crate::system::ui_errors::runtime::runtime(&failure.error),
                        script_category(&failure.error.error),
                        verified_location(&cartridge, &failure.error.error),
                    )
                }
            };
            shell.report_error(message.clone());
            ctx.logs
                .push_terminal(category, message.clone(), location.clone());
            match origin {
                RunOrigin::Shell => SystemState::Shell {
                    shell,
                    loaded: Some(cartridge),
                },
                RunOrigin::Code => {
                    let failure = RunFailure { message, location };
                    back_to_code(shell, cartridge, Some(failure), ctx)
                }
            }
        }
    }
}

/// Returns to the Code view on `cartridge`, as the editor was left, showing
/// `failure` when the run that ended there failed.
fn back_to_code(
    shell: Box<Shell>,
    cartridge: LoadedCartridge,
    failure: Option<RunFailure>,
    ctx: &mut Context<'_>,
) -> SystemState {
    let state = open_workspace(
        shell,
        cartridge,
        ctx.workspace,
        Documents {
            sprites: ctx.sprites,
            code: ctx.code,
        },
        ctx.video_defaults,
    );
    if let (Some(failure), SystemState::Workspace { cartridge, .. }) = (failure, &state) {
        let code = ctx.code.get_or_insert_default();
        ctx.workspace
            .run_failed(code, cartridge, &failure.message, failure.location.as_ref());
    }
    state
}

/// Starts `cartridge` again from the beginning after a run from Code. A
/// save that fails returns to the Code view instead.
pub(super) fn restart(
    shell: Box<Shell>,
    mut running: RunningCartridge,
    ctx: &mut Context<'_>,
) -> SystemState {
    let mut shell = shell;
    drain_logs(&mut shell, ctx.logs, &mut running);
    let cartridge = running.stop();
    tracing::info!(
        cartridge = %cartridge_identity(cartridge.manifest()),
        "cartridge restarted"
    );
    if !save_before_running(&cartridge, ctx) {
        return back_to_code(shell, cartridge, None, ctx);
    }
    start(shell, cartridge, RunOrigin::Code, ctx)
}

/// Writes the modified documents when Save Before Running is on. Returns
/// whether the run may go ahead. A failure is shown by the workspace.
pub(super) fn save_before_running(cartridge: &LoadedCartridge, ctx: &mut Context<'_>) -> bool {
    if !ctx.workspace.editor_settings().save_before_running {
        return true;
    }
    let documents = Documents {
        sprites: ctx.sprites,
        code: ctx.code,
    };
    documents.save_all(cartridge, ctx.workspace, ctx.logs)
}

/// Advances the running cartridge by one tick, or pauses it on a break. A
/// break wins over any button events in the same tick, which are dropped.
/// A stop or restart asked for from outside ends the run in its place.
pub(in crate::system) fn running_tick(
    mut shell: Box<Shell>,
    mut running: RunningCartridge,
    input: &[InputEvent],
    ctx: &mut Context<'_>,
) -> SystemState {
    if running.origin() == RunOrigin::Code {
        if input.contains(&InputEvent::Stop) {
            return stop(shell, running, StopReason::Stopped, ctx);
        }
        if input.contains(&InputEvent::Restart) {
            return restart(shell, running, ctx);
        }
    }
    if input.contains(&InputEvent::Break) {
        return pause(shell, running, ctx);
    }
    match running.tick(input) {
        Ok(()) => {
            drain_logs(&mut shell, ctx.logs, &mut running);
            SystemState::Running { shell, running }
        }
        Err(err) => stop(shell, running, StopReason::Failed(err), ctx),
    }
}

/// Keeps the cartridge's VM without ticking it and shows the pause screen.
pub(super) fn pause(
    mut shell: Box<Shell>,
    mut running: RunningCartridge,
    ctx: &mut Context<'_>,
) -> SystemState {
    drain_logs(&mut shell, ctx.logs, &mut running);
    ctx.log_panel.close();
    let identity = cartridge_identity(running.cartridge().manifest());
    tracing::info!(cartridge = %identity, "cartridge paused");
    shell.report(ui_text::get_with(
        "system-paused",
        &[("identity", Arg::Text(&identity.to_string()))],
    ));
    let overlay = match running.origin() {
        RunOrigin::Shell => PauseOverlay::new(),
        RunOrigin::Code => PauseOverlay::with_controls(),
    };
    SystemState::Paused {
        shell,
        running,
        view: PausedView::Game(overlay),
    }
}

/// Disposes of the cartridge's VM and returns to where the run started,
/// the shell or the Code view, with the cartridge still loaded. Every way a
/// VM ends goes through here.
pub(super) fn stop(
    mut shell: Box<Shell>,
    mut running: RunningCartridge,
    reason: StopReason,
    ctx: &mut Context<'_>,
) -> SystemState {
    drain_logs(&mut shell, ctx.logs, &mut running);
    ctx.log_panel.close();
    let identity = cartridge_identity(running.cartridge().manifest());
    let origin = running.origin();
    let cartridge = running.stop();
    let failure = match reason {
        StopReason::Stopped => {
            tracing::info!(cartridge = %identity, "cartridge stopped");
            shell.report(ui_text::get_with(
                "system-stopped",
                &[("identity", Arg::Text(&identity.to_string()))],
            ));
            None
        }
        StopReason::Failed(err) => {
            tracing::info!(cartridge = %identity, error = %err, "cartridge failed");
            let message = crate::system::ui_errors::runtime::runtime(&err);
            shell.report_error(message.clone());
            let location = verified_location(&cartridge, &err.error);
            ctx.logs.push_terminal(
                script_category(&err.error),
                message.clone(),
                location.clone(),
            );
            Some(RunFailure { message, location })
        }
    };
    match origin {
        RunOrigin::Shell => SystemState::Shell {
            shell,
            loaded: Some(cartridge),
        },
        RunOrigin::Code => back_to_code(shell, cartridge, failure, ctx),
    }
}
