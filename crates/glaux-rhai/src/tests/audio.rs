//! `audio::tone`, `audio::env` and `audio::gate`.

use super::*;
use glaux_audio::{ChannelState, Envelope, EnvelopeWrite, Tone, Waveform};
use pretty_assertions::assert_eq;

#[test]
fn audio_functions_write_the_audio_state_in_any_phase() {
    let mut runtime = started(
        r#"
        audio::tone(1, 440, 15, "saw");
        fn boot() { audio::env(1, 10, 20, 8, 30); }
        fn tick() { audio::gate(1, true); }
        fn draw() { audio::gate(1, false); }
        "#,
    );
    runtime.tick(NO_INPUT).unwrap();
    let audio = runtime.audio_mut();
    let ChannelState { tone, envelope, .. } = audio.channels()[1];
    assert_eq!(
        tone,
        Tone {
            frequency: 440,
            volume: 15,
            waveform: Waveform::Saw
        }
    );
    let expected = Envelope {
        attack_ms: 10,
        decay_ms: 20,
        sustain: 8,
        release_ms: 30,
    };
    assert_eq!(envelope, expected);
    let writes: Vec<_> = audio
        .writes()
        .iter()
        .map(|write| (write.channel, write.write))
        .collect();
    assert_eq!(
        writes,
        [
            (1, EnvelopeWrite::Envelope(expected)),
            (1, EnvelopeWrite::Gate(true)),
            (1, EnvelopeWrite::Gate(false)),
        ]
    );
}

#[test]
fn out_of_range_values_and_unknown_waveforms_are_rejected() {
    let cases = [
        (
            "audio::tone(4, 440, 1, \"saw\");",
            "bad argument #1 to 'audio::tone' (channel 0-3 expected, got 4)",
        ),
        (
            "audio::tone(0, 24001, 1, \"saw\");",
            "bad argument #2 to 'audio::tone' (frequency 0-24000 expected, got 24001)",
        ),
        (
            "audio::tone(0, 440, 16, \"saw\");",
            "bad argument #3 to 'audio::tone' (volume 0-15 expected, got 16)",
        ),
        (
            "audio::tone(0, 440, 1, \"sine\");",
            "bad argument #4 to 'audio::tone' (unknown waveform 'sine'; expected square, triangle, saw or noise)",
        ),
        (
            "audio::tone(0, 440, 1, 2);",
            "bad argument #4 to 'audio::tone' (string expected, got i64)",
        ),
        (
            "audio::env(0, 10001, 0, 0, 0);",
            "bad argument #2 to 'audio::env' (time in ms 0-10000 expected, got 10001)",
        ),
        (
            "audio::env(0, 0, 0, 16, 0);",
            "bad argument #4 to 'audio::env' (sustain 0-15 expected, got 16)",
        ),
        (
            "audio::gate(0, 1);",
            "bad argument #2 to 'audio::gate' (bool expected, got i64)",
        ),
    ];
    for (source, message) in cases {
        assert_eq!(
            start_message(source),
            format!("main.rhai:1: {message}"),
            "{source}"
        );
    }
}

#[test]
fn a_rejected_call_writes_nothing() {
    let failure =
        start_failure("audio::tone(0, 440, 1, \"square\");\naudio::tone(0, 880, 16, \"saw\");");
    assert_eq!(failure.error.phase, Phase::Entry);
    let mut runtime = started(
        "audio::tone(0, 440, 1, \"square\");\ntry { audio::tone(0, 880, 16, \"saw\"); } catch {}",
    );
    assert_eq!(runtime.audio_mut().channels()[0].tone.frequency, 440);
}
