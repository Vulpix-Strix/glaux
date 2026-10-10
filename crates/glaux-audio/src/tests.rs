use super::*;
use pretty_assertions::assert_eq;

fn gate(channel: usize, open: bool) -> ChannelWrite {
    ChannelWrite {
        channel,
        write: EnvelopeWrite::Gate(open),
    }
}

fn envelope(attack_ms: u32, decay_ms: u32, sustain: u8, release_ms: u32) -> Envelope {
    Envelope {
        attack_ms,
        decay_ms,
        sustain,
        release_ms,
    }
}

#[test]
fn starts_silent_with_instant_envelopes_and_closed_gates() {
    let state = AudioState::new();
    let silent = ChannelState {
        tone: Tone {
            waveform: Waveform::Square,
            frequency: 0,
            volume: 0,
        },
        envelope: envelope(0, 0, MAX_SUSTAIN, 0),
        gate: false,
    };
    assert_eq!(state.channels(), &[silent; CHANNELS]);
    assert_eq!(state.baseline(), &[silent; CHANNELS]);
    assert!(state.writes().is_empty());
}

#[test]
fn waveform_names_round_trip() {
    for waveform in Waveform::ALL {
        assert_eq!(Waveform::from_name(waveform.name()), Some(waveform));
    }
    assert_eq!(
        Waveform::ALL.map(Waveform::name),
        ["square", "triangle", "saw", "noise"]
    );
    for name in ["", "Square", "SAW", "sine", "noise "] {
        assert_eq!(Waveform::from_name(name), None, "{name:?}");
    }
}

#[test]
fn writes_accept_the_range_limits() {
    let mut state = AudioState::new();
    let loudest = Tone {
        waveform: Waveform::Noise,
        frequency: MAX_FREQUENCY,
        volume: MAX_VOLUME,
    };
    let slowest = envelope(MAX_ENVELOPE_MS, MAX_ENVELOPE_MS, 0, MAX_ENVELOPE_MS);
    state.set_tone(CHANNELS - 1, loudest).unwrap();
    state.set_envelope(CHANNELS - 1, slowest).unwrap();
    state.set_gate(CHANNELS - 1, true).unwrap();
    let channel = state.channels()[CHANNELS - 1];
    assert_eq!(channel.tone, loudest);
    assert_eq!(channel.envelope, slowest);
    assert!(channel.gate);
}

#[test]
fn writes_reject_values_past_the_limits_and_change_nothing() {
    let mut state = AudioState::new();
    let tone = Tone::default();
    let too_long = MAX_ENVELOPE_MS + 1;
    let errors = [
        state.set_tone(CHANNELS, tone),
        state.set_envelope(CHANNELS, Envelope::default()),
        state.set_gate(CHANNELS, true),
        state.set_tone(
            0,
            Tone {
                frequency: MAX_FREQUENCY + 1,
                ..tone
            },
        ),
        state.set_tone(
            0,
            Tone {
                volume: MAX_VOLUME + 1,
                ..tone
            },
        ),
        state.set_envelope(0, envelope(too_long, 0, 0, 0)),
        state.set_envelope(0, envelope(0, too_long, 0, 0)),
        state.set_envelope(0, envelope(0, 0, 0, too_long)),
        state.set_envelope(0, envelope(0, 0, MAX_SUSTAIN + 1, 0)),
    ];
    assert_eq!(
        errors,
        [
            Err(AudioError::Channel(CHANNELS)),
            Err(AudioError::Channel(CHANNELS)),
            Err(AudioError::Channel(CHANNELS)),
            Err(AudioError::Frequency(MAX_FREQUENCY + 1)),
            Err(AudioError::Volume(MAX_VOLUME + 1)),
            Err(AudioError::Time(too_long)),
            Err(AudioError::Time(too_long)),
            Err(AudioError::Time(too_long)),
            Err(AudioError::Sustain(MAX_SUSTAIN + 1)),
        ]
    );
    assert_eq!(state, AudioState::new());
}

#[test]
fn errors_name_the_range() {
    assert_eq!(
        AudioError::Frequency(30_000).to_string(),
        "frequency 0-24000 expected, got 30000"
    );
    assert_eq!(
        AudioError::Channel(4).to_string(),
        "channel 0-3 expected, got 4"
    );
}

#[test]
fn tone_writes_record_no_envelope_write() {
    let mut state = AudioState::new();
    state.set_gate(0, true).unwrap();
    state
        .set_tone(
            0,
            Tone {
                waveform: Waveform::Saw,
                frequency: 440,
                volume: 8,
            },
        )
        .unwrap();
    assert_eq!(state.writes(), [gate(0, true)]);
}

#[test]
fn envelope_writes_are_kept_in_order() {
    let mut state = AudioState::new();
    let shape = envelope(10, 20, 5, 30);
    state.set_gate(1, true).unwrap();
    state.set_envelope(1, shape).unwrap();
    state.set_gate(1, false).unwrap();
    assert_eq!(
        state.writes(),
        [
            gate(1, true),
            ChannelWrite {
                channel: 1,
                write: EnvelopeWrite::Envelope(shape),
            },
            gate(1, false),
        ]
    );
    assert_eq!(state.channels()[1].envelope, shape);
    assert!(!state.channels()[1].gate);
}

#[test]
fn only_an_immediate_repeat_is_compacted() {
    let mut state = AudioState::new();
    for open in [true, true, false, true] {
        state.set_gate(0, open).unwrap();
    }
    assert_eq!(
        state.writes(),
        [gate(0, true), gate(0, false), gate(0, true)]
    );

    let mut state = AudioState::new();
    for channel in [0, 1, 0] {
        state.set_gate(channel, true).unwrap();
    }
    assert_eq!(
        state.writes(),
        [gate(0, true), gate(1, true), gate(0, true)]
    );

    let mut state = AudioState::new();
    let shape = envelope(1, 2, 3, 4);
    state.set_envelope(2, shape).unwrap();
    state.set_envelope(2, shape).unwrap();
    state.set_envelope(2, envelope(1, 2, 3, 5)).unwrap();
    assert_eq!(state.writes().len(), 2);
}

#[test]
fn compaction_does_not_reach_across_a_commit() {
    let mut state = AudioState::new();
    state.set_gate(0, true).unwrap();
    state.commit();
    state.set_gate(0, true).unwrap();
    assert_eq!(state.writes(), [gate(0, true)]);
}

#[test]
fn pending_writes_are_capped() {
    let mut state = AudioState::new();
    let open = |index: usize| index.is_multiple_of(2);
    for index in 0..MAX_ENVELOPE_WRITES {
        state.set_gate(0, open(index)).unwrap();
    }
    let last = open(MAX_ENVELOPE_WRITES - 1);
    let before = state.clone();
    assert_eq!(state.set_gate(0, !last), Err(AudioError::TooManyWrites));
    assert_eq!(state, before);
    // A repeat of the last write records nothing, so it still fits.
    assert_eq!(state.set_gate(0, last), Ok(()));
    // Committing frees the budget.
    state.commit();
    assert_eq!(state.set_gate(0, true), Ok(()));
}

#[test]
fn commit_moves_the_registers_into_the_baseline() {
    let mut state = AudioState::new();
    let tone = Tone {
        waveform: Waveform::Triangle,
        frequency: 220,
        volume: 9,
    };
    state.set_tone(3, tone).unwrap();
    state.set_gate(3, true).unwrap();
    assert_eq!(state.baseline(), &[ChannelState::default(); CHANNELS]);

    state.commit();
    assert_eq!(state.baseline(), state.channels());
    assert_eq!(state.baseline()[3].tone, tone);
    assert!(state.baseline()[3].gate);
    assert!(state.writes().is_empty());
}
