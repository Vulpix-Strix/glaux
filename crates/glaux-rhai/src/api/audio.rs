//! The `audio` module, with `audio::tone`, `audio::env` and `audio::gate`.

use std::cell::RefCell;
use std::rc::Rc;

use glaux_audio::{
    AudioState, Envelope, MAX_ENVELOPE_MS, MAX_FREQUENCY, MAX_SUSTAIN, MAX_VOLUME, Tone,
};
use rhai::{Dynamic, Engine, FuncRegistration, Module, NativeCallContext, Shared};

use crate::args::{Result, bool_arg, bounded_arg, channel_arg, located, waveform_arg};

/// Registers the `audio` module.
pub(crate) fn install(engine: &mut Engine, audio_state: &Rc<RefCell<AudioState>>) {
    let mut audio = Module::new();

    let state = Rc::clone(audio_state);
    FuncRegistration::new("tone").set_into_module(
        &mut audio,
        move |ctx: NativeCallContext,
              channel: Dynamic,
              frequency: Dynamic,
              volume: Dynamic,
              waveform: Dynamic|
              -> Result<()> {
            const NAME: &str = "audio::tone";
            let channel = channel_arg(&ctx, NAME, &channel)?;
            let tone = Tone {
                frequency: bounded_arg(
                    &ctx,
                    NAME,
                    2,
                    &frequency,
                    "frequency",
                    MAX_FREQUENCY.into(),
                )? as u16,
                volume: bounded_arg(&ctx, NAME, 3, &volume, "volume", MAX_VOLUME.into())? as u8,
                waveform: waveform_arg(&ctx, NAME, 4, &waveform)?,
            };
            let written = state.borrow_mut().set_tone(channel, tone);
            written.map_err(|err| located(&ctx, err.to_string()))
        },
    );

    let state = Rc::clone(audio_state);
    FuncRegistration::new("env").set_into_module(
        &mut audio,
        move |ctx: NativeCallContext,
              channel: Dynamic,
              attack: Dynamic,
              decay: Dynamic,
              sustain: Dynamic,
              release: Dynamic|
              -> Result<()> {
            const NAME: &str = "audio::env";
            let channel = channel_arg(&ctx, NAME, &channel)?;
            let time = |position, value: &Dynamic| {
                bounded_arg(&ctx, NAME, position, value, "time in ms", MAX_ENVELOPE_MS)
            };
            let envelope = Envelope {
                attack_ms: time(2, &attack)?,
                decay_ms: time(3, &decay)?,
                sustain: bounded_arg(&ctx, NAME, 4, &sustain, "sustain", MAX_SUSTAIN.into())? as u8,
                release_ms: time(5, &release)?,
            };
            let written = state.borrow_mut().set_envelope(channel, envelope);
            written.map_err(|err| located(&ctx, err.to_string()))
        },
    );

    let state = Rc::clone(audio_state);
    FuncRegistration::new("gate").set_into_module(
        &mut audio,
        move |ctx: NativeCallContext, channel: Dynamic, enabled: Dynamic| -> Result<()> {
            const NAME: &str = "audio::gate";
            let channel = channel_arg(&ctx, NAME, &channel)?;
            let open = bool_arg(&ctx, NAME, 2, &enabled)?;
            let written = state.borrow_mut().set_gate(channel, open);
            written.map_err(|err| located(&ctx, err.to_string()))
        },
    );

    engine.register_static_module("audio", Shared::new(audio));
}
