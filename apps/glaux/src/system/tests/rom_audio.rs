use super::*;
use pretty_assertions::assert_eq;

/// Boots `system` the way the main loop does, writing and rendering
/// the PSG after every update, until the shell takes over. Returns the
/// samples of each tick; tick `n` is the one that reaches frame `n + 1`.
fn boot_audio(mut system: System, psg: &mut Psg) -> (System, Vec<Vec<i16>>) {
    let mut ticks = Vec::new();
    while matches!(system.state(), SystemState::Boot { .. }) {
        system = system.update(&[]);
        system.audio(psg);
        ticks.push(render_psg(psg));
    }
    (system, ticks)
}

fn quiet(samples: &[i16]) -> bool {
    samples.iter().all(|&sample| sample == 0)
}

/// Ticks of the boot chime rendered on its own: long enough for every
/// release to finish.
const CHIME_TICKS: usize = 60;

fn chime_alone() -> Vec<Vec<i16>> {
    let mut sound = SystemSound::new(sounds());
    sound.play(SystemSfx::Boot);
    let mut psg = Psg::new();
    (0..CHIME_TICKS)
        .map(|_| {
            sound.tick();
            sound.apply(&mut psg, false);
            render_psg(&mut psg)
        })
        .collect()
}

#[test]
fn boot_plays_its_chime_once_through_the_psg() {
    let (_, ticks) = boot_audio(new_system(), &mut Psg::new());
    let start = boot::CHIME_FRAME as usize - 1;
    let chime = chime_alone();
    assert!(chime.iter().any(|samples| !quiet(samples)));
    assert!(quiet(chime.last().unwrap()), "the chime has faded out");

    assert!(ticks[..start].iter().all(|samples| quiet(samples)));
    assert_eq!(&ticks[start..start + CHIME_TICKS], chime.as_slice());
    assert!(
        ticks[start + CHIME_TICKS..]
            .iter()
            .all(|samples| quiet(samples))
    );
}

#[test]
fn a_rom_without_sounds_boots_silent() {
    let system = System::new(
        info(),
        SystemSounds::default(),
        video_defaults(),
        cartridges(),
        fake_rom().font(),
        fake_rom().palette().clone(),
    );
    let (_, ticks) = boot_audio(system, &mut Psg::new());
    assert!(ticks.iter().all(|samples| quiet(samples)));
}

#[test]
fn the_shell_starts_with_no_system_sound() {
    let mut psg = Psg::new();
    let (mut system, _) = boot_audio(new_system(), &mut psg);
    assert!(!system.sound.is_active());
    system.audio(&mut psg);
    assert_silent(&mut psg);

    let mut system = enter(enter(system, "tone 1 330 9"), "gate 1 on");
    assert_eq!(gates(&mut system, &mut psg), [false, true, false, false]);
    assert_eq!(voices(&mut system, &mut psg)[1], (Waveform::Square, 330, 9));
}

#[test]
fn a_chime_cut_off_by_the_shell_leaves_nothing_behind() {
    let mut psg = Psg::new();
    let mut sound = SystemSound::new(sounds());
    sound.play(SystemSfx::Boot);
    let mut system = System {
        state: SystemState::Boot {
            boot: Boot::at_frame(boot::FINISH_FRAME - 2),
            cartridges: cartridges(),
        },
        info: info(),
        audio_source: AudioSource::Silent,
        sound,
        video_defaults: video_defaults(),
        workspace: Workspace::new(fake_rom().font(), fake_rom().palette().clone()),
        sprites: None,
        code: None,
        logs: Default::default(),
        log_panel: Default::default(),
        diagnostics: None,
        host_requests: Default::default(),
        unsaved: None,
        exit_requested: false,
        exiting: false,
    };
    system = system.update(&[]);
    system.audio(&mut psg);
    assert!(gates(&mut system, &mut psg).iter().any(|&open| open));
    assert!(!quiet(&render_psg(&mut psg)));

    system = system.update(&[]);
    assert!(matches!(system.state(), SystemState::Shell { .. }));
    assert!(!system.sound.is_active());
    system.audio(&mut psg);
    assert_silent(&mut psg);
}

#[test]
fn boot_runs_the_same_without_audio_output() {
    let mut psg = Psg::new();
    let mut heard = new_system();
    let mut unheard = new_system();
    while let SystemState::Boot { boot, .. } = heard.state() {
        let SystemState::Boot { boot: other, .. } = unheard.state() else {
            panic!("boot ended early without audio output");
        };
        assert_eq!(boot, other);
        heard = heard.update(&[]);
        heard.audio(&mut psg);
        unheard = unheard.update(&[]);
    }
    assert_eq!(shell_of(&heard), shell_of(&unheard));
}

#[test]
fn shell_tones_reach_the_psg_until_turned_off() {
    let mut psg = Psg::new();
    let mut system = shell();
    assert!(is_silent(&mut system, &mut psg));

    let system = enter(system, "tone 0 440 12");
    let mut system = enter(system, "tone 2 220 5 saw");
    let playing = voices(&mut system, &mut psg);
    assert_eq!(playing[0], (Waveform::Square, 440, 12));
    assert_eq!(playing[1].2, 0);
    assert_eq!(playing[2], (Waveform::Saw, 220, 5));
    assert_eq!(playing[3].2, 0);

    let mut system = enter(system, "tone 0 off");
    let playing = voices(&mut system, &mut psg);
    assert_eq!(playing[0].2, 0);
    assert_eq!(playing[2], (Waveform::Saw, 220, 5));

    let mut system = enter(system, "tone off");
    assert!(is_silent(&mut system, &mut psg));
}

#[test]
fn shell_gates_and_envelopes_reach_the_psg() {
    let mut psg = Psg::new();
    let mut system = enter(shell(), "tone 0 440 12");
    assert_eq!(gates(&mut system, &mut psg), [false; CHANNELS]);
    assert_eq!(render_psg(&mut psg), vec![0; SAMPLES_PER_TICK]);

    let system = enter(system, "env 0 10 80 8 200");
    let mut system = enter(system, "gate 0 on");
    assert_eq!(gates(&mut system, &mut psg), [true, false, false, false]);
    let channel = psg.channel(0).unwrap();
    assert_eq!(
        channel.adsr(),
        Adsr {
            attack: 480,
            decay: 3840,
            sustain: 8,
            release: 9600,
        }
    );
    assert_eq!(channel.envelope_stage(), EnvelopeStage::Attack);

    // Rewriting the same voice every tick lets the envelope run on
    // instead of restarting it.
    for _ in 0..5 {
        gates(&mut system, &mut psg);
        render_psg(&mut psg);
    }
    assert_eq!(
        psg.channel(0).unwrap().envelope_stage(),
        EnvelopeStage::Sustain
    );

    let mut system = enter(system, "gate 0 off");
    assert_eq!(gates(&mut system, &mut psg), [false; CHANNELS]);
    assert_eq!(
        psg.channel(0).unwrap().envelope_stage(),
        EnvelopeStage::Release
    );

    let mut system = enter(enter(system, "gate 0 on"), "tone 0 off");
    assert_eq!(gates(&mut system, &mut psg), [false; CHANNELS]);
    assert_eq!(psg.channel(0).unwrap().volume(), 0);
}

#[test]
fn tones_are_muted_while_a_cartridge_runs_or_shows_its_pause_screen() {
    let mut psg = Psg::new();
    let fixture = Fixture::new("sys-tone");
    let system = enter(shell_with_cartridge(&fixture, ""), "tone 0 440 12");
    let system = enter(system, "tone 3 1200 4 noise");
    let mut system = enter(enter(system, "gate 0 on"), "gate 3 on");
    assert_eq!(gates(&mut system, &mut psg), [true, false, false, true]);

    let mut system = enter(enter(system, "load pong"), "run");
    assert!(is_running(&system));
    assert!(is_silent(&mut system, &mut psg));
    assert_eq!(gates(&mut system, &mut psg), [false; CHANNELS]);

    let mut system = press_break(system);
    assert!(is_paused_game(&system));
    assert!(is_silent(&mut system, &mut psg));
    assert_eq!(gates(&mut system, &mut psg), [false; CHANNELS]);

    let mut system = open_shell(system);
    let playing = voices(&mut system, &mut psg);
    assert_eq!(playing[0], (Waveform::Square, 440, 12));
    assert_eq!(playing[3], (Waveform::Noise, 1200, 4));
    assert_eq!(gates(&mut system, &mut psg), [true, false, false, true]);
}

fn is_silent(system: &mut System, psg: &mut Psg) -> bool {
    voices(system, psg)
        .iter()
        .all(|&(_, _, volume)| volume == 0)
}
