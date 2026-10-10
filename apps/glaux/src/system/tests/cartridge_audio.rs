use super::*;
use pretty_assertions::assert_eq;

/// Plays the tick already written into `psg`, then advances `system` one
/// tick and writes its sound, the way the main loop does. Returns the
/// samples played.
fn step(system: System, psg: &mut Psg) -> (System, Vec<i16>) {
    let samples = render_psg(psg);
    let mut system = system.update(&[]);
    system.audio(psg);
    (system, samples)
}

fn steps(mut system: System, psg: &mut Psg, n: usize) -> System {
    for _ in 0..n {
        system = step(system, psg).0;
    }
    system
}

/// Starts `script` and writes the sound of its start into `psg`.
fn started_audio(fixture: &Fixture, script: &str, psg: &mut Psg) -> System {
    let mut system = started(fixture, script);
    assert!(is_running(&system));
    system.audio(psg);
    system
}

/// Channel 0 holds a 100 ms attack, no decay and sustain 8: it reaches
/// Sustain within 6 ticks and has room to attack again from there.
const HELD_NOTE: &str = r#"
        audio.env(0, 100, 0, 8, 1000)
        audio.tone(0, 440, 12, "square")
        function boot() audio.gate(0, true) end
    "#;

#[test]
fn cartridge_audio_reaches_the_psg_in_samples() {
    let mut psg = Psg::new();
    let fixture = Fixture::new("sys-audio-units");
    let script = r#"
            audio.env(1, 10, 80, 8, 200)
            audio.tone(1, 660, 9, "triangle")
            audio.gate(1, true)
        "#;
    let _system = started_audio(&fixture, script, &mut psg);
    let channel = psg.channel(1).unwrap();
    assert_eq!(
        channel.adsr(),
        Adsr {
            attack: 480,
            decay: 3840,
            sustain: 8,
            release: 9600,
        }
    );
    assert_eq!(
        (channel.waveform(), channel.frequency(), channel.volume()),
        (Waveform::Triangle, 660, 9)
    );
    assert_eq!(channel.envelope_stage(), EnvelopeStage::Attack);
    assert!(render_psg(&mut psg).iter().any(|&sample| sample != 0));
}

#[test]
fn cartridge_gates_open_retrigger_and_release() {
    let mut psg = Psg::new();
    let fixture = Fixture::new("sys-audio-gate");
    let script = format!(
        r#"{HELD_NOTE}
            function tick()
                if frame() == 10 then audio.gate(0, true) end
                if frame() == 20 then audio.gate(0, false) end
            end
            "#
    );
    let system = started_audio(&fixture, &script, &mut psg);
    assert_eq!(stage(&psg, 0), EnvelopeStage::Attack);

    let system = steps(system, &mut psg, 10);
    assert_eq!(stage(&psg, 0), EnvelopeStage::Sustain);

    // Opening an open gate retriggers the attack from the held level.
    let system = steps(system, &mut psg, 1);
    assert_eq!(stage(&psg, 0), EnvelopeStage::Attack);
    assert!(psg.channel(0).unwrap().gate());

    let _system = steps(system, &mut psg, 10);
    assert_eq!(stage(&psg, 0), EnvelopeStage::Release);
    assert!(!psg.channel(0).unwrap().gate());
}

#[test]
fn cartridge_tone_changes_do_not_retrigger_the_envelope() {
    let mut psg = Psg::new();
    let fixture = Fixture::new("sys-audio-tone");
    let script = format!(
        r#"{HELD_NOTE}
            function tick()
                if frame() == 10 then audio.tone(0, 880, 5, "saw") end
            end
            "#
    );
    let system = steps(started_audio(&fixture, &script, &mut psg), &mut psg, 10);
    assert_eq!(stage(&psg, 0), EnvelopeStage::Sustain);
    let level = psg.channel(0).unwrap().envelope_level();

    let _system = steps(system, &mut psg, 1);
    let channel = psg.channel(0).unwrap();
    assert_eq!(
        (channel.waveform(), channel.frequency(), channel.volume()),
        (Waveform::Saw, 880, 5)
    );
    assert_eq!(channel.envelope_stage(), EnvelopeStage::Sustain);
    assert_eq!(channel.envelope_level(), level);
}

#[test]
fn gate_writes_within_one_tick_all_reach_the_psg() {
    let mut psg = Psg::new();
    let fixture = Fixture::new("sys-audio-blip");
    // Opening and closing in the same tick with an instant attack leaves
    // a release from full level; the final gate state alone would leave
    // the channel idle.
    let script = r#"
            audio.env(0, 0, 0, 15, 1000)
            audio.tone(0, 440, 12, "square")
            audio.gate(0, true)
            audio.gate(0, false)
        "#;
    let _system = started_audio(&fixture, script, &mut psg);
    let channel = psg.channel(0).unwrap();
    assert_eq!(channel.envelope_stage(), EnvelopeStage::Release);
    assert_eq!(channel.envelope_level(), ENVELOPE_FULL);
}

#[test]
fn cartridge_audio_persists_across_ticks() {
    let mut psg = Psg::new();
    let fixture = Fixture::new("sys-audio-persist");
    let script = r#"
            function boot()
                audio.tone(2, 330, 7, "noise")
                audio.gate(2, true)
            end
            function tick() end
        "#;
    let mut system = started_audio(&fixture, script, &mut psg);
    for _ in 0..30 {
        let (next, samples) = step(system, &mut psg);
        system = next;
        assert!(samples.iter().any(|&sample| sample != 0));
    }
    let channel = psg.channel(2).unwrap();
    assert_eq!(
        (channel.waveform(), channel.frequency(), channel.volume()),
        (Waveform::Noise, 330, 7)
    );
    assert!(channel.gate());
}

#[test]
fn pausing_silences_the_cartridge_and_resuming_reattacks_open_gates() {
    let fixture = Fixture::new("sys-audio-pause");
    let resumes: [fn(System) -> System; 2] =
        [press_break, |system| enter(open_shell(system), "resume")];
    for resume in resumes {
        let mut psg = Psg::new();
        let system = steps(started_audio(&fixture, HELD_NOTE, &mut psg), &mut psg, 10);
        assert_eq!(stage(&psg, 0), EnvelopeStage::Sustain);

        let mut system = press_break(system);
        system.audio(&mut psg);
        assert!(is_paused_game(&system));
        assert_silent(&mut psg);

        let mut system = resume(system);
        assert!(is_running(&system));
        system.audio(&mut psg);
        let channel = psg.channel(0).unwrap();
        assert_eq!(
            (channel.waveform(), channel.frequency(), channel.volume()),
            (Waveform::Square, 440, 12)
        );
        assert_eq!(channel.adsr().sustain, 8);
        assert_eq!(channel.envelope_stage(), EnvelopeStage::Attack);
        assert_eq!(channel.envelope_level(), 0);
    }
}

#[test]
fn stopping_or_failing_leaves_the_psg_silent() {
    let fixture = Fixture::new("sys-audio-stop");
    let mut psg = Psg::new();
    let system = steps(started_audio(&fixture, HELD_NOTE, &mut psg), &mut psg, 3);
    let mut system = stop_cartridge(system);
    system.audio(&mut psg);
    assert_eq!(loaded_name(&system), Some("Pong"));
    assert_silent(&mut psg);

    let mut psg = Psg::new();
    let script =
        format!(r#"{HELD_NOTE} function tick() if frame() == 3 then error("boom") end end"#);
    let system = steps(started_audio(&fixture, &script, &mut psg), &mut psg, 4);
    assert!(!is_running(&system));
    assert_silent(&mut psg);
}

#[test]
fn shell_and_cartridge_audio_stay_isolated() {
    let mut psg = Psg::new();
    let fixture = Fixture::new("sys-audio-isolated");
    let script = r#"
            audio.tone(1, 660, 9, "saw")
            audio.gate(1, true)
        "#;
    let system = enter(shell_with_cartridge(&fixture, script), "tone 0 440 12");
    let mut system = enter(system, "gate 0 on");
    assert_eq!(gates(&mut system, &mut psg), [true, false, false, false]);

    // The shell's voice does not play over the cartridge.
    let mut system = enter(enter(system, "load pong"), "run");
    assert_eq!(gates(&mut system, &mut psg), [false, true, false, false]);
    assert_eq!(voices(&mut system, &mut psg)[0].2, 0);

    // The cartridge's voice does not play over the shell, and the shell's
    // own voice returns.
    let mut system = open_shell(press_break(system));
    assert_eq!(gates(&mut system, &mut psg), [true, false, false, false]);
    assert_eq!(voices(&mut system, &mut psg)[1].2, 0);

    let mut system = enter(system, "stop");
    assert_eq!(gates(&mut system, &mut psg), [true, false, false, false]);
    assert_eq!(voices(&mut system, &mut psg)[1].2, 0);
}

#[test]
fn same_cartridge_writes_produce_identical_psg_state() {
    let script = r#"
            audio.env(0, 5, 40, 6, 120)
            audio.env(3, 0, 20, 4, 60)
            audio.tone(0, 440, 12, "square")
            audio.tone(1, 660, 6, "triangle")
            audio.tone(2, 220, 5, "saw")
            audio.tone(3, 1200, 4, "noise")
            function tick()
                local f = frame()
                audio.gate(f % 4, f % 3 ~= 0)
                if f % 5 == 0 then audio.tone(2, 220 + f, 5, "saw") end
            end
        "#;
    let run = |fixture: &Fixture, shell_tones: bool| {
        let mut psg = Psg::new();
        let mut system = shell_with_cartridge(fixture, script);
        if shell_tones {
            system = enter(enter(system, "tone 2 1000 15 noise"), "gate 2 on");
            system = steps(system, &mut psg, 7);
        }
        let mut system = enter(enter(system, "load pong"), "run");
        system.audio(&mut psg);
        let mut samples = Vec::new();
        for _ in 0..30 {
            let (next, played) = step(system, &mut psg);
            system = next;
            samples.extend(played);
        }
        (samples, psg)
    };
    let (samples, psg) = run(&Fixture::new("sys-audio-determinism-a"), false);
    assert!(samples.iter().any(|&sample| sample != 0));
    assert_eq!(
        run(&Fixture::new("sys-audio-determinism-b"), false),
        (samples.clone(), psg.clone())
    );
    assert_eq!(
        run(&Fixture::new("sys-audio-determinism-c"), true),
        (samples, psg)
    );
}
