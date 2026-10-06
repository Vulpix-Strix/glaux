//! The `audio` table.

use super::*;
use glaux_audio::{
    ChannelWrite, Envelope, EnvelopeWrite, MAX_ENVELOPE_MS, MAX_ENVELOPE_WRITES, MAX_FREQUENCY,
    MAX_SUSTAIN, MAX_VOLUME, Tone, Waveform,
};
use pretty_assertions::assert_eq;

fn gate_write(channel: usize, open: bool) -> ChannelWrite {
    ChannelWrite {
        channel,
        write: EnvelopeWrite::Gate(open),
    }
}

#[test]
fn audio_configures_each_channel() {
    let mut runtime = started(
        r#"
        audio.tone(0, 440, 15, "square")
        audio.tone(1, 660, 6, "triangle")
        audio.tone(2, 220, 5, "saw")
        audio.tone(3, 1200, 4, "noise")
        audio.env(2, 10, 80, 8, 200)
        audio.gate(1, true)
        "#,
    );
    let audio = runtime.audio_mut();
    let channels = audio.channels();
    assert_eq!(
        channels.map(|channel| channel.tone),
        [
            Tone {
                waveform: Waveform::Square,
                frequency: 440,
                volume: 15
            },
            Tone {
                waveform: Waveform::Triangle,
                frequency: 660,
                volume: 6
            },
            Tone {
                waveform: Waveform::Saw,
                frequency: 220,
                volume: 5
            },
            Tone {
                waveform: Waveform::Noise,
                frequency: 1200,
                volume: 4
            },
        ]
    );
    assert_eq!(
        channels[2].envelope,
        Envelope {
            attack_ms: 10,
            decay_ms: 80,
            sustain: 8,
            release_ms: 200,
        }
    );
    assert_eq!(
        channels.map(|channel| channel.gate),
        [false, true, false, false]
    );
}

#[test]
fn audio_accepts_the_range_limits() {
    let mut runtime = started(
        r#"
        audio.tone(3, 24000, 15, "noise")
        audio.tone(0, 0, 0, "square")
        audio.env(3, 10000, 10000, 0, 10000)
        audio.env(0, 0, 0, 15, 0)
        "#,
    );
    let audio = runtime.audio_mut();
    let channels = audio.channels();
    assert_eq!(channels[3].tone.frequency, MAX_FREQUENCY);
    assert_eq!(channels[3].tone.volume, MAX_VOLUME);
    assert_eq!(
        channels[3].envelope,
        Envelope {
            attack_ms: MAX_ENVELOPE_MS,
            decay_ms: MAX_ENVELOPE_MS,
            sustain: 0,
            release_ms: MAX_ENVELOPE_MS,
        }
    );
    assert_eq!(channels[0].envelope.sustain, MAX_SUSTAIN);
}

#[test]
fn audio_writes_in_every_phase_are_kept_in_order() {
    let mut runtime = started(
        r#"
        audio.gate(0, true)
        function boot() audio.gate(1, true) end
        function tick() audio.gate(0, false) end
        function draw() audio.gate(1, false) end
        "#,
    );
    runtime.tick(NO_BUTTONS).unwrap();
    assert_eq!(
        runtime.audio_mut().writes(),
        [
            gate_write(0, true),
            gate_write(1, true),
            gate_write(0, false),
            gate_write(1, false),
        ]
    );
}

#[test]
fn audio_state_persists_across_ticks() {
    let mut runtime = started(
        r#"
        function boot()
            audio.tone(2, 330, 9, "saw")
            audio.gate(2, true)
        end
        "#,
    );
    runtime.audio_mut().commit();
    for _ in 0..5 {
        runtime.tick(NO_BUTTONS).unwrap();
    }
    let audio = runtime.audio_mut();
    assert!(audio.writes().is_empty());
    assert_eq!(
        audio.channels()[2].tone,
        Tone {
            waveform: Waveform::Saw,
            frequency: 330,
            volume: 9
        }
    );
    assert!(audio.channels()[2].gate);
}

#[test]
fn bad_audio_arguments_are_runtime_errors() {
    for (call, detail) in [
        (
            r#"audio.tone(4, 440, 8, "square")"#,
            "#1 to 'audio.tone' (channel 0-3 expected, got 4)",
        ),
        (
            r#"audio.tone(-1, 440, 8, "square")"#,
            "#1 to 'audio.tone' (channel 0-3 expected, got -1)",
        ),
        (
            r#"audio.tone(0.5, 440, 8, "square")"#,
            "#1 to 'audio.tone' (channel 0-3 expected, got 0.5)",
        ),
        (
            r#"audio.tone("0", 440, 8, "square")"#,
            "#1 to 'audio.tone' (number expected, got string)",
        ),
        (
            r#"audio.tone(0, 24001, 8, "square")"#,
            "#2 to 'audio.tone' (frequency 0-24000 expected, got 24001)",
        ),
        (
            r#"audio.tone(0, 440.5, 8, "square")"#,
            "#2 to 'audio.tone' (frequency 0-24000 expected, got 440.5)",
        ),
        (
            r#"audio.tone(0, 0/0, 8, "square")"#,
            "#2 to 'audio.tone' (frequency 0-24000 expected, got NaN)",
        ),
        (
            r#"audio.tone(0, 440, 16, "square")"#,
            "#3 to 'audio.tone' (volume 0-15 expected, got 16)",
        ),
        (
            r#"audio.tone(0, 440, 8, "sine")"#,
            "#4 to 'audio.tone' (unknown waveform 'sine'; expected square, triangle, saw or noise)",
        ),
        (
            r#"audio.tone(0, 440, 8, "SQUARE")"#,
            "#4 to 'audio.tone' (unknown waveform 'SQUARE'; expected square, triangle, saw or noise)",
        ),
        (
            "audio.tone(0, 440, 8)",
            "#4 to 'audio.tone' (string expected, got nil)",
        ),
        (
            "audio.tone(0, 440, 8, 1)",
            "#4 to 'audio.tone' (string expected, got number)",
        ),
        (
            "audio.env(0, 10001, 0, 15, 0)",
            "#2 to 'audio.env' (time in ms 0-10000 expected, got 10001)",
        ),
        (
            "audio.env(0, 0, -1, 15, 0)",
            "#3 to 'audio.env' (time in ms 0-10000 expected, got -1)",
        ),
        (
            "audio.env(0, 0, 0, 16, 0)",
            "#4 to 'audio.env' (sustain 0-15 expected, got 16)",
        ),
        (
            "audio.env(0, 0, 0, 15, 2.5)",
            "#5 to 'audio.env' (time in ms 0-10000 expected, got 2.5)",
        ),
        (
            "audio.env(0, 0, 0, 15)",
            "#5 to 'audio.env' (number expected, got nil)",
        ),
        (
            "audio.gate(4, true)",
            "#1 to 'audio.gate' (channel 0-3 expected, got 4)",
        ),
        (
            "audio.gate(0, 1)",
            "#2 to 'audio.gate' (boolean expected, got number)",
        ),
        (
            "audio.gate(0)",
            "#2 to 'audio.gate' (boolean expected, got nil)",
        ),
    ] {
        assert_eq!(
            runtime_message(call),
            format!("main.luau:1: bad argument {detail}"),
            "{call}"
        );
    }
}

#[test]
fn rejected_audio_calls_change_nothing() {
    let mut runtime = started(
        r#"
        audio.tone(0, 440, 8, "saw")
        pcall(audio.tone, 0, 99999, 8, "square")
        pcall(audio.env, 0, 0, 0, 99, 0)
        pcall(audio.gate, 0, "yes")
        "#,
    );
    let audio = runtime.audio_mut();
    assert_eq!(
        audio.channels()[0].tone,
        Tone {
            waveform: Waveform::Saw,
            frequency: 440,
            volume: 8
        }
    );
    assert_eq!(audio.channels()[0].envelope, Envelope::default());
    assert!(audio.writes().is_empty());
}

#[test]
fn too_many_envelope_writes_in_one_tick_fail() {
    let mut runtime = started(&format!(
        "function tick() for i = 1, {} do audio.gate(0, i % 2 == 1) end end",
        MAX_ENVELOPE_WRITES + 1
    ));
    assert_eq!(
        runtime.tick(NO_BUTTONS),
        Err(error(
            Phase::Tick,
            ScriptError::Runtime(format!(
                "main.luau:1: too many envelope and gate writes in one tick (limit {MAX_ENVELOPE_WRITES})"
            ))
        ))
    );
}

#[test]
fn committed_writes_free_the_budget() {
    let mut runtime = started(&format!(
        "function tick() for i = 1, {MAX_ENVELOPE_WRITES} do audio.gate(0, i % 2 == 1) end end",
    ));
    for _ in 0..3 {
        runtime.tick(NO_BUTTONS).unwrap();
        assert_eq!(runtime.audio_mut().writes().len(), MAX_ENVELOPE_WRITES);
        runtime.audio_mut().commit();
    }
}
