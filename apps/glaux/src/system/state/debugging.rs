//! Serving a debugged cartridge while it is stopped.

use glaux_input::InputEvent;
use glaux_script::debug::{DebugReply, DebugRequest, Resume, Step, Stop};

use super::Context;
use super::running::{drain_logs, launch, restart, save_for_debugging, stop};
use super::workspace::{serve_host, serve_navigation, serve_scripts};
use crate::system::clipboard::Clipboard;
use crate::system::running::{RunOrigin, RunningCartridge, Tick};
use crate::system::shell::Shell;
use crate::system::workspace::{DebugAction, WorkspaceRequest, WorkspaceVideo};
use crate::system::{HostRequest, StopReason, SystemState};

/// Shows where a debugged cartridge stopped, in the Code view, and keeps its
/// runtime waiting there.
pub(in crate::system) fn stopped(
    mut shell: Box<Shell>,
    mut running: RunningCartridge,
    stop: Stop,
    ctx: &mut Context<'_>,
) -> SystemState {
    running.pull_stopped_output();
    drain_logs(&mut shell, ctx.logs, &mut running);
    ctx.log_panel.close();
    let video = match ctx.sprites.as_ref() {
        Some(document) => ctx.workspace.prepare_video(document),
        None => WorkspaceVideo::unavailable(String::new()),
    };
    let code = ctx.code.get_or_insert_default();
    ctx.workspace.show_stop(code, running.cartridge(), stop);
    SystemState::Debugging {
        shell,
        running,
        video,
    }
}

/// Starts the workspace's cartridge under the debugger. The modified
/// documents are written first whatever Save Before Running says, and the
/// run is cancelled, with the failure shown, if one cannot be.
pub(in crate::system) fn debug_from_workspace(
    state: SystemState,
    ctx: &mut Context<'_>,
) -> SystemState {
    let SystemState::Workspace {
        shell,
        cartridge,
        video,
    } = state
    else {
        return state;
    };
    ctx.workspace.end_stroke(ctx.sprites.as_mut());
    if !save_for_debugging(&cartridge, ctx) {
        return SystemState::Workspace {
            shell,
            cartridge,
            video,
        };
    }
    launch(shell, cartridge, RunOrigin::Code, true, ctx)
}

/// What a tick of the stopped state decided.
enum Outcome {
    Stay,
    /// The cartridge went on and its tick finished.
    Run,
    /// The cartridge went on and stopped again.
    Stopped,
    /// The user ended the run.
    Ended,
    Failed(glaux_script::RuntimeError),
}

/// Feeds `input` to the Code view of a stopped cartridge and carries out
/// what it asks. The keys that go on with the cartridge, stop it or restart
/// it act first, and events after one of them are dropped, as after any
/// change of screen. The scripts can be read, searched and navigated, but
/// not edited.
pub(in crate::system) fn debugging_tick(
    mut shell: Box<Shell>,
    mut running: RunningCartridge,
    video: WorkspaceVideo,
    ctx: &mut Context<'_>,
    input: &[InputEvent],
    clipboard: &mut dyn Clipboard,
) -> (SystemState, Vec<HostRequest>) {
    let mut host_requests = Vec::new();
    ctx.workspace
        .set_entry_script(&running.cartridge().manifest().runtime.entry);
    ctx.workspace.tick(ctx.code.as_mut());
    let mut outcome = Outcome::Stay;
    for &event in input {
        let action = match event {
            InputEvent::Stop => {
                end_stop(ctx);
                return (
                    stop(shell, running, StopReason::Stopped, ctx),
                    host_requests,
                );
            }
            InputEvent::Restart => {
                end_stop(ctx);
                return (restart(shell, running, ctx), host_requests);
            }
            InputEvent::Run => Some(DebugAction::Resume(Resume::Continue)),
            InputEvent::StepOver => Some(DebugAction::Resume(Resume::Step(Step::Over))),
            InputEvent::StepInto => Some(DebugAction::Resume(Resume::Step(Step::Into))),
            InputEvent::StepOut => Some(DebugAction::Resume(Resume::Step(Step::Out))),
            InputEvent::NextView | InputEvent::PreviousView | InputEvent::Debug => continue,
            InputEvent::Break if !ctx.workspace.claims_break() => continue,
            _ => None,
        };
        let action = match action {
            Some(action) => action,
            None => {
                let code = ctx.code.get_or_insert_default();
                match ctx.workspace.handle_input(event, None, Some(code)) {
                    None => continue,
                    Some(WorkspaceRequest::Stopped(action)) => action,
                    Some(WorkspaceRequest::SaveEditorSettings(settings)) => {
                        host_requests.push(HostRequest::SaveEditorSettings(settings));
                        continue;
                    }
                    Some(WorkspaceRequest::Scripts(request)) => {
                        serve_scripts(request, ctx.workspace, code, running.cartridge());
                        continue;
                    }
                    Some(WorkspaceRequest::Navigate(request)) => {
                        serve_navigation(request, ctx.workspace, code, running.cartridge());
                        continue;
                    }
                    Some(WorkspaceRequest::Host(request)) => {
                        serve_host(request, ctx.workspace, code, clipboard, &mut host_requests);
                        continue;
                    }
                    Some(
                        WorkspaceRequest::Run
                        | WorkspaceRequest::Debug
                        | WorkspaceRequest::Assets(_),
                    ) => continue,
                }
            }
        };
        match serve(action, &mut running, ctx) {
            Outcome::Stay => {}
            done => {
                outcome = done;
                break;
            }
        }
    }
    let state = match outcome {
        Outcome::Stay => SystemState::Debugging {
            shell,
            running,
            video,
        },
        Outcome::Stopped => {
            running.pull_stopped_output();
            drain_logs(&mut shell, ctx.logs, &mut running);
            SystemState::Debugging {
                shell,
                running,
                video,
            }
        }
        Outcome::Run => {
            end_stop(ctx);
            drain_logs(&mut shell, ctx.logs, &mut running);
            SystemState::Running { shell, running }
        }
        Outcome::Ended => {
            end_stop(ctx);
            stop(shell, running, StopReason::Stopped, ctx)
        }
        Outcome::Failed(err) => {
            end_stop(ctx);
            stop(shell, running, StopReason::Failed(err), ctx)
        }
    };
    (state, host_requests)
}

fn end_stop(ctx: &mut Context<'_>) {
    ctx.workspace.end_stop(ctx.code.as_mut());
}

/// Carries out what the Code view or a key asked of the stopped cartridge.
fn serve(action: DebugAction, running: &mut RunningCartridge, ctx: &mut Context<'_>) -> Outcome {
    match action {
        DebugAction::Resume(resume) => {
            if let Some(code) = ctx.code.as_ref() {
                running.set_breakpoints(code.breakpoints());
            }
            match running.resume(resume) {
                Ok(Tick::Stopped(stop)) => {
                    let code = ctx.code.get_or_insert_default();
                    ctx.workspace.show_stop(code, running.cartridge(), stop);
                    Outcome::Stopped
                }
                Ok(Tick::Done) => Outcome::Run,
                Err(err) => Outcome::Failed(err),
            }
        }
        DebugAction::Stop => Outcome::Ended,
        DebugAction::SelectFrame(frame) => {
            select_frame(frame, running, ctx);
            Outcome::Stay
        }
        DebugAction::Toggle(path) => {
            toggle(&path, running, ctx);
            Outcome::Stay
        }
    }
}

fn select_frame(frame: usize, running: &mut RunningCartridge, ctx: &mut Context<'_>) {
    let reply = running.ask(DebugRequest::Variables { frame });
    let Some(view) = ctx.workspace.stop_view_mut() else {
        return;
    };
    view.select(frame);
    match reply {
        DebugReply::Variables(variables) => view.set_variables(variables),
        _ => view.set_unavailable(),
    }
    let code = ctx.code.get_or_insert_default();
    ctx.workspace.show_frame(code, running.cartridge());
}

fn toggle(path: &[usize], running: &mut RunningCartridge, ctx: &mut Context<'_>) {
    let Some(view) = ctx.workspace.stop_view_mut() else {
        return;
    };
    if !view.toggle(path) {
        return;
    }
    let frame = view.selected();
    let reply = running.ask(DebugRequest::Expand {
        frame,
        path: path.to_vec(),
    });
    if let (DebugReply::Variables(entries), Some(view)) = (reply, ctx.workspace.stop_view_mut()) {
        view.set_children(path, entries);
    }
}
