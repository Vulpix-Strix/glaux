//! A cartridge kept alive without ticks, under the pause overlay or the shell.

use super::Context;
use super::running::{restart, stop};
use crate::system::pause::PauseOverlay;
use crate::system::running::{RunOrigin, RunningCartridge};
use crate::system::shell::{CartridgeSlot, Shell, ShellRequest, cartridge_identity};
use crate::system::{PausedView, StopReason, SystemState, paused_shell_hint};
use crate::ui_text;
use glaux_i18n::Arg;
use glaux_input::InputEvent;

/// Handles the pause screen. Break resumes the cartridge. For a run from
/// the shell Enter opens the shell, which takes the events after it. For a
/// run from Code Enter stops the run and returns to the editor, and R
/// restarts it. Other events are ignored, and only the overlay's own
/// animation advances.
pub(in crate::system) fn paused_game_tick(
    mut shell: Box<Shell>,
    running: RunningCartridge,
    mut overlay: PauseOverlay,
    input: &[InputEvent],
    ctx: &mut Context<'_>,
) -> SystemState {
    let from_code = running.origin() == RunOrigin::Code;
    for (index, &event) in input.iter().enumerate() {
        match event {
            InputEvent::Break => return resume(shell, running),
            InputEvent::Enter | InputEvent::Stop if from_code => {
                return stop(shell, running, StopReason::Stopped, ctx);
            }
            InputEvent::Text('r' | 'R') | InputEvent::Restart if from_code => {
                return restart(shell, running, ctx);
            }
            InputEvent::Enter => {
                let identity = cartridge_identity(running.cartridge().manifest());
                tracing::info!(cartridge = %identity, "shell opened with the cartridge paused");
                shell.report(paused_shell_hint());
                return paused_shell_tick(shell, running, &input[index + 1..], ctx);
            }
            _ => {}
        }
    }
    overlay.tick();
    SystemState::Paused {
        shell,
        running,
        view: PausedView::Game(overlay),
    }
}

/// Feeds `input` to the shell while the cartridge stays paused, and carries
/// out a `resume` or `stop`. Events after either in the same tick are
/// dropped.
pub(in crate::system) fn paused_shell_tick(
    mut shell: Box<Shell>,
    running: RunningCartridge,
    input: &[InputEvent],
    ctx: &mut Context<'_>,
) -> SystemState {
    shell.tick();
    for &event in input {
        match shell.handle_input(event, CartridgeSlot::Paused(running.cartridge())) {
            None => {}
            Some(ShellRequest::Resume) => return resume(shell, running),
            Some(ShellRequest::Stop) => return stop(shell, running, StopReason::Stopped, ctx),
            // `load`, `unload`, `new` and `run` refuse to replace a paused
            // cartridge before asking about unsaved edits.
            Some(
                _request @ (ShellRequest::Load { .. }
                | ShellRequest::Unload
                | ShellRequest::Run
                | ShellRequest::Credits
                | ShellRequest::Docs(_)
                | ShellRequest::ConfirmUnsaved { .. }),
            ) => {
                unreachable!("{_request:?} was requested while a cartridge is paused")
            }
        }
    }
    SystemState::Paused {
        shell,
        running,
        view: PausedView::Shell,
    }
}

/// Continues the paused cartridge with its VM as it was. Its buttons start
/// released, so nothing pressed or held around the pause reaches it as a
/// press. It is first ticked on the next update.
fn resume(mut shell: Box<Shell>, mut running: RunningCartridge) -> SystemState {
    running.reset_input();
    let identity = cartridge_identity(running.cartridge().manifest());
    tracing::info!(cartridge = %identity, "cartridge resumed");
    shell.report(ui_text::get_with(
        "system-resumed",
        &[("identity", Arg::Text(&identity.to_string()))],
    ));
    SystemState::Running { shell, running }
}
