use super::*;
use crate::{CHANNELS, ChannelWrite, EnvelopeWrite, Waveform};
use pretty_assertions::assert_eq;

fn step(tick: u32, action: SfxAction) -> SfxStep {
    SfxStep { tick, action }
}

fn gate(channel: usize, open: bool) -> SfxAction {
    SfxAction::Gate { channel, open }
}

fn tone(channel: usize, frequency: u16) -> SfxAction {
    SfxAction::Tone {
        channel,
        tone: Tone {
            waveform: Waveform::Square,
            frequency,
            volume: 8,
        },
    }
}

fn sfx(steps: &[SfxStep]) -> Sfx {
    Sfx::new(steps.to_vec()).unwrap()
}

fn gate_write(channel: usize, open: bool) -> ChannelWrite {
    ChannelWrite {
        channel,
        write: EnvelopeWrite::Gate(open),
    }
}

/// Ticks `player` `n` times, committing after each tick, and returns the
/// envelope writes each tick made.
fn writes_per_tick(
    player: &mut SfxPlayer,
    audio: &mut AudioState,
    n: usize,
) -> Vec<Vec<ChannelWrite>> {
    (0..n)
        .map(|_| {
            player.tick(audio).unwrap();
            let writes = audio.writes().to_vec();
            audio.commit();
            writes
        })
        .collect()
}

#[test]
fn actions_apply_at_their_tick_offsets() {
    let mut player = SfxPlayer::new();
    player.play(sfx(&[
        step(0, gate(0, true)),
        step(2, gate(1, true)),
        step(5, gate(0, false)),
    ]));
    let mut audio = AudioState::new();
    let writes = writes_per_tick(&mut player, &mut audio, 6);
    assert_eq!(
        writes,
        [
            vec![gate_write(0, true)],
            vec![],
            vec![gate_write(1, true)],
            vec![],
            vec![],
            vec![gate_write(0, false)],
        ]
    );
}

#[test]
fn actions_on_one_tick_apply_in_order() {
    let mut player = SfxPlayer::new();
    player.play(sfx(&[
        step(0, tone(0, 440)),
        step(0, gate(0, true)),
        step(0, gate(0, false)),
        step(0, tone(0, 880)),
    ]));
    let mut audio = AudioState::new();
    player.tick(&mut audio).unwrap();
    assert_eq!(audio.writes(), [gate_write(0, true), gate_write(0, false)]);
    assert_eq!(audio.channels()[0].tone.frequency, 880);
}

#[test]
fn reopening_an_open_gate_is_kept_as_a_retrigger() {
    let mut player = SfxPlayer::new();
    player.play(sfx(&[step(0, gate(2, true)), step(3, gate(2, true))]));
    let mut audio = AudioState::new();
    let writes = writes_per_tick(&mut player, &mut audio, 4);
    assert_eq!(writes[0], [gate_write(2, true)]);
    assert_eq!(writes[3], [gate_write(2, true)]);
    assert!(audio.channels()[2].gate);
}

#[test]
fn the_player_stops_after_its_last_step() {
    let mut player = SfxPlayer::new();
    player.play(sfx(&[step(0, gate(0, true)), step(2, gate(0, false))]));
    let mut audio = AudioState::new();
    for _ in 0..2 {
        player.tick(&mut audio).unwrap();
        assert!(player.is_active());
    }
    player.tick(&mut audio).unwrap();
    assert!(!player.is_active());

    audio.commit();
    let before = audio.clone();
    player.tick(&mut audio).unwrap();
    assert_eq!(audio, before);
}

#[test]
fn an_empty_sound_ends_on_its_first_tick() {
    let mut player = SfxPlayer::new();
    player.play(Sfx::default());
    assert!(player.is_active());
    player.tick(&mut AudioState::new()).unwrap();
    assert!(!player.is_active());
}

#[test]
fn playing_again_restarts_from_tick_zero() {
    let first = sfx(&[step(0, gate(0, true)), step(10, gate(0, false))]);
    let second = sfx(&[step(0, gate(1, true)), step(2, gate(1, false))]);
    let mut player = SfxPlayer::new();
    let mut audio = AudioState::new();
    player.play(first);
    writes_per_tick(&mut player, &mut audio, 4);

    player.play(second);
    let writes = writes_per_tick(&mut player, &mut audio, 3);
    assert_eq!(
        writes,
        [
            vec![gate_write(1, true)],
            vec![],
            vec![gate_write(1, false)]
        ]
    );
    assert!(!player.is_active());
    // The first sound's later steps never play.
    assert!(audio.channels()[0].gate);
}

#[test]
fn steps_must_not_go_back_in_time() {
    let steps = vec![
        step(0, gate(0, true)),
        step(3, gate(0, false)),
        step(3, gate(1, true)),
        step(2, gate(1, false)),
    ];
    assert_eq!(Sfx::new(steps), Err(SfxError::Unordered { index: 3 }));
}

#[test]
fn a_rejected_write_stops_the_player() {
    let mut player = SfxPlayer::new();
    player.play(sfx(&[
        step(0, gate(0, true)),
        step(0, gate(CHANNELS, true)),
        step(1, gate(0, false)),
    ]));
    let mut audio = AudioState::new();
    assert_eq!(player.tick(&mut audio), Err(AudioError::Channel(CHANNELS)));
    assert!(!player.is_active());
    assert_eq!(audio.writes(), [gate_write(0, true)]);
}
