//! Transitions out of the shell, including the commands that start a
//! cartridge, load one or open another screen.

use super::running::start;
use super::workspace::open_workspace;
use super::{Context, Documents, discard_documents};
use crate::cartridge::LoadedCartridge;
use crate::system::credits::CreditsView;
use crate::system::running::RunOrigin;
use crate::system::shell::{CartridgeSlot, Shell, ShellRequest, cartridge_identity};
use crate::system::unsaved::{Pending, UnsavedFlow};
use crate::system::workspace::{DocsStart, Workspace};
use crate::system::{SystemState, no_cartridge_hint};
use crate::ui_text;
use glaux_i18n::Arg;
use glaux_input::InputEvent;
use glaux_machine::psg::Psg;

/// Writes the shell's diagnostic voices into `psg`.
pub(in crate::system) fn shell_audio(shell: &Shell, psg: &mut Psg) {
    for (index, voice) in shell.voices().into_iter().enumerate() {
        let Some(channel) = psg.channel_mut(index) else {
            unreachable!("the shell holds one voice per PSG channel");
        };
        channel.set_adsr(voice.adsr);
        let Some(tone) = voice.tone else {
            channel.set_volume(0);
            channel.gate_off();
            continue;
        };
        channel.set_waveform(tone.waveform);
        channel.set_frequency(tone.frequency);
        channel.set_volume(tone.volume);
        match (voice.gate, channel.gate()) {
            (true, false) => channel.gate_on(),
            (false, true) => channel.gate_off(),
            _ => {}
        }
    }
}

/// Feeds `input` to the shell and carries out the cartridge requests its
/// commands make. Break opens the workspace on the loaded cartridge, or
/// explains how to get one.
///
/// A started cartridge does not take input, so events that follow a
/// successful `run` in the same tick are dropped. So are events after a
/// Break that opens the workspace, so keys meant for the shell never act
/// on it, and events after a command that asks what to do with unsaved
/// edits, which the question takes instead.
///
/// Replacing or unloading the cartridge drops its documents, once the new
/// cartridge has loaded or the old one is unloaded. A `load` that fails
/// keeps them. While they hold unsaved edits, the shell asks first unless
/// told to discard them.
pub(in crate::system) fn shell_tick(
    mut shell: Box<Shell>,
    loaded: Option<LoadedCartridge>,
    input: &[InputEvent],
    ctx: &mut Context<'_>,
) -> SystemState {
    // Ticking before the input lets this tick's input restart the blink.
    shell.tick();
    shell_input(shell, loaded, input, ctx)
}

/// Feeds `input` to the shell, as [`shell_tick`] does, without ticking it.
fn shell_input(
    mut shell: Box<Shell>,
    mut loaded: Option<LoadedCartridge>,
    input: &[InputEvent],
    ctx: &mut Context<'_>,
) -> SystemState {
    // Input is deliberately not logged: user input stays out of
    // application logs.
    for (index, &event) in input.iter().enumerate() {
        if event == InputEvent::Break {
            match loaded {
                Some(cartridge) => {
                    let video_defaults = ctx.video_defaults;
                    return open_workspace(
                        shell,
                        cartridge,
                        ctx.workspace,
                        Documents {
                            sprites: ctx.sprites,
                            code: ctx.code,
                        },
                        video_defaults,
                    );
                }
                None => {
                    shell.report(no_cartridge_hint());
                    continue;
                }
            }
        }
        let slot = CartridgeSlot::loaded(loaded.as_ref(), ctx.has_unsaved());
        let request = shell.handle_input(event, slot);
        match shell_request(shell, loaded, request, &input[index + 1..], ctx) {
            (
                SystemState::Shell {
                    shell: kept,
                    loaded: still,
                },
                true,
            ) => {
                (shell, loaded) = (kept, still);
            }
            (state, _) => return state,
        }
    }
    SystemState::Shell { shell, loaded }
}

/// Carries out `request`, a shell command's cartridge request. `rest` is
/// the input that followed it in this tick. Returns the state that
/// follows, and whether the shell still takes the rest of this tick's
/// input.
pub(super) fn shell_request(
    mut shell: Box<Shell>,
    mut loaded: Option<LoadedCartridge>,
    request: Option<ShellRequest>,
    rest: &[InputEvent],
    ctx: &mut Context<'_>,
) -> (SystemState, bool) {
    match request {
        None => {}
        // The shell only asks to load a cartridge that already loaded.
        Some(ShellRequest::Load { cartridge, origin }) => {
            discard_documents(ctx.sprites, ctx.code, ctx.workspace);
            let cartridge = loaded.insert(*cartridge);
            shell.announce_loaded(cartridge.manifest(), origin);
        }
        Some(ShellRequest::Unload) => {
            let Some(cartridge) = loaded.take() else {
                unreachable!("UNLOAD was requested without a loaded cartridge");
            };
            discard_documents(ctx.sprites, ctx.code, ctx.workspace);
            let identity = cartridge_identity(cartridge.manifest());
            tracing::info!(cartridge = %identity, "cartridge unloaded");
            shell.report(ui_text::get_with(
                "system-unloaded",
                &[("identity", Arg::Text(&identity.to_string()))],
            ));
        }
        Some(ShellRequest::Run) => {
            // `run` only asks to start when a cartridge is loaded.
            let Some(cartridge) = loaded else {
                unreachable!("RUN was requested without a loaded cartridge");
            };
            let state = match start(shell, cartridge, RunOrigin::Shell, ctx) {
                SystemState::Shell { shell, loaded } => shell_input(shell, loaded, rest, ctx),
                state => state,
            };
            return (state, false);
        }
        Some(ShellRequest::ConfirmUnsaved { action, line }) => {
            let pending = Pending::Command {
                action,
                line: line.clone(),
            };
            match UnsavedFlow::new(pending, ctx.code.as_ref(), ctx.sprites.as_ref()) {
                Some(flow) => {
                    tracing::info!("asking what to do with unsaved edits");
                    *ctx.unsaved = Some(flow);
                    return (SystemState::Shell { shell, loaded }, false);
                }
                // Nothing is unsaved after all: run it as confirmed.
                None => {
                    let slot = CartridgeSlot::loaded(loaded.as_ref(), false);
                    let request = shell.run_confirmed(&line, slot);
                    return shell_request(shell, loaded, request, rest, ctx);
                }
            }
        }
        Some(ShellRequest::Credits) => {
            tracing::info!("opening the credits");
            let view = Box::new(CreditsView::open());
            return (
                SystemState::Credits {
                    shell,
                    loaded,
                    view,
                },
                false,
            );
        }
        Some(ShellRequest::Docs(start)) => {
            return (open_docs_apart(ctx.workspace, shell, loaded, start), false);
        }
        Some(_request @ (ShellRequest::Resume | ShellRequest::Stop)) => {
            unreachable!("{_request:?} was requested without a paused cartridge")
        }
    }
    (SystemState::Shell { shell, loaded }, true)
}

/// The documentation browser on `start`, filling the screen over the shell.
pub(in crate::system) fn open_docs_apart(
    workspace: &mut Workspace,
    shell: Box<Shell>,
    loaded: Option<LoadedCartridge>,
    start: DocsStart,
) -> SystemState {
    tracing::info!("opening the documentation");
    workspace.show_docs_apart(start);
    SystemState::Docs { shell, loaded }
}
