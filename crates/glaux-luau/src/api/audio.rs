//! The `audio` table: `audio.tone`, `audio.env` and `audio.gate`.

use std::cell::RefCell;
use std::rc::Rc;

use glaux_audio::{
    AudioState, Envelope, MAX_ENVELOPE_MS, MAX_FREQUENCY, MAX_SUSTAIN, MAX_VOLUME, Tone,
};
use mlua::{Lua, Value};

use crate::args::{
    bad_argument, bounded_arg, channel_arg, located, script_type_name, waveform_arg,
};

/// Installs the `audio` table.
pub(crate) fn install(lua: &Lua, audio_state: &Rc<RefCell<AudioState>>) -> mlua::Result<()> {
    let audio = lua.create_table()?;

    let state = Rc::clone(audio_state);
    let tone = lua.create_function(
        move |lua, (channel, frequency, volume, waveform): (Value, Value, Value, Value)| {
            const NAME: &str = "audio.tone";
            let channel = channel_arg(lua, NAME, &channel)?;
            let tone = Tone {
                frequency: bounded_arg(lua, NAME, 2, &frequency, "frequency", MAX_FREQUENCY.into())?
                    as u16,
                volume: bounded_arg(lua, NAME, 3, &volume, "volume", MAX_VOLUME.into())? as u8,
                waveform: waveform_arg(lua, NAME, 4, &waveform)?,
            };
            let written = state.borrow_mut().set_tone(channel, tone);
            written.map_err(|err| located(lua, err.to_string()))
        },
    )?;
    audio.raw_set("tone", tone)?;

    let state = Rc::clone(audio_state);
    let env =
        lua.create_function(
            move |lua,
                  (channel, attack, decay, sustain, release): (
                Value,
                Value,
                Value,
                Value,
                Value,
            )| {
                const NAME: &str = "audio.env";
                let channel = channel_arg(lua, NAME, &channel)?;
                let time = |position, value: &Value| {
                    bounded_arg(lua, NAME, position, value, "time in ms", MAX_ENVELOPE_MS)
                };
                let envelope = Envelope {
                    attack_ms: time(2, &attack)?,
                    decay_ms: time(3, &decay)?,
                    sustain: bounded_arg(lua, NAME, 4, &sustain, "sustain", MAX_SUSTAIN.into())?
                        as u8,
                    release_ms: time(5, &release)?,
                };
                let written = state.borrow_mut().set_envelope(channel, envelope);
                written.map_err(|err| located(lua, err.to_string()))
            },
        )?;
    audio.raw_set("env", env)?;

    let state = Rc::clone(audio_state);
    let gate = lua.create_function(move |lua, (channel, enabled): (Value, Value)| {
        const NAME: &str = "audio.gate";
        let channel = channel_arg(lua, NAME, &channel)?;
        let Value::Boolean(open) = enabled else {
            return Err(bad_argument(
                lua,
                NAME,
                2,
                &format!("boolean expected, got {}", script_type_name(&enabled)),
            ));
        };
        let written = state.borrow_mut().set_gate(channel, open);
        written.map_err(|err| located(lua, err.to_string()))
    })?;
    audio.raw_set("gate", gate)?;

    lua.globals().raw_set("audio", audio)
}
