//! Applies an [`AudioState`] to the PSG. Cartridges and System ROM sounds
//! both reach the sound hardware through here.

use glaux_audio::{AudioState, ChannelState, Envelope, EnvelopeWrite};
use glaux_machine::psg::{self, Adsr, Psg, SAMPLES_PER_MS};

/// Writes `audio` into `psg` and commits it.
///
/// `fresh` means `psg` was just reset for this source: each channel is
/// first rebuilt from the registers as of the last commit, so a channel
/// whose gate was left open starts a new attack. Then every channel takes
/// its current oscillator registers, and the envelope and gate writes since
/// the last commit are replayed in order. Each gate opening reaches the PSG,
/// so an open gate is retriggered.
pub(super) fn apply(audio: &mut AudioState, psg: &mut Psg, fresh: bool) {
    if fresh {
        for (index, state) in audio.baseline().iter().enumerate() {
            let channel = psg_channel(psg, index);
            channel.set_adsr(adsr(state.envelope));
            if state.gate {
                channel.gate_on();
            }
        }
    }
    for (index, &ChannelState { tone, .. }) in audio.channels().iter().enumerate() {
        let channel = psg_channel(psg, index);
        channel.set_waveform(waveform(tone.waveform));
        channel.set_frequency(tone.frequency);
        channel.set_volume(tone.volume);
    }
    for write in audio.writes() {
        let channel = psg_channel(psg, write.channel);
        match write.write {
            EnvelopeWrite::Envelope(envelope) => channel.set_adsr(adsr(envelope)),
            EnvelopeWrite::Gate(true) => channel.gate_on(),
            EnvelopeWrite::Gate(false) => channel.gate_off(),
        }
    }
    audio.commit();
}

fn psg_channel(psg: &mut Psg, index: usize) -> &mut psg::Channel {
    let Some(channel) = psg.channel_mut(index) else {
        unreachable!("audio state and the PSG have the same channels");
    };
    channel
}

fn waveform(waveform: glaux_audio::Waveform) -> psg::Waveform {
    match waveform {
        glaux_audio::Waveform::Square => psg::Waveform::Square,
        glaux_audio::Waveform::Triangle => psg::Waveform::Triangle,
        glaux_audio::Waveform::Saw => psg::Waveform::Saw,
        glaux_audio::Waveform::Noise => psg::Waveform::Noise,
    }
}

/// Envelope times converted from milliseconds to PSG samples, exactly.
fn adsr(envelope: Envelope) -> Adsr {
    Adsr {
        attack: envelope.attack_ms * SAMPLES_PER_MS,
        decay: envelope.decay_ms * SAMPLES_PER_MS,
        sustain: envelope.sustain,
        release: envelope.release_ms * SAMPLES_PER_MS,
    }
}
