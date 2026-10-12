// Every sound effect in the game, played on the PSG through `audio::tone`,
// `audio::env` and `audio::gate`.
//
// Each effect owns one channel, retriggers it with a fresh envelope and
// sweeps its pitch linearly, one step per tick, from `from` to `to` Hz. The
// envelopes all have zero sustain, so a channel falls silent on its own even
// if its gate stays open; `update` still closes the gate when the sweep ends
// so no channel is left open between effects.
//
// A module cannot keep state of its own, so the sweeps in progress are a
// value the caller holds: one slot per channel, `()` when idle. Every
// function takes it and returns it updated.

const FIRE = 0;
const HIT = 1;
const PICKUP = 2;
const ALERT = 3;
const CHANNELS = 4;

const EFFECTS = #{
    fire: #{ channel: FIRE, wave: "square", from: 880, to: 420, volume: 7, ticks: 5, attack: 0, decay: 90, release: 20 },
    hit: #{ channel: HIT, wave: "noise", from: 3200, to: 1800, volume: 8, ticks: 4, attack: 0, decay: 70, release: 10 },
    kill: #{ channel: HIT, wave: "triangle", from: 320, to: 960, volume: 12, ticks: 7, attack: 0, decay: 140, release: 30 },
    coin: #{ channel: PICKUP, wave: "triangle", from: 1046, to: 1568, volume: 10, ticks: 6, attack: 0, decay: 120, release: 30 },
    heal: #{ channel: PICKUP, wave: "triangle", from: 392, to: 1175, volume: 11, ticks: 16, attack: 10, decay: 300, release: 40 },
    dash: #{ channel: PICKUP, wave: "noise", from: 7000, to: 1500, volume: 7, ticks: 9, attack: 0, decay: 160, release: 20 },
    hurt: #{ channel: ALERT, wave: "saw", from: 260, to: 70, volume: 13, ticks: 14, attack: 0, decay: 240, release: 40 },
    heavy: #{ channel: ALERT, wave: "square", from: 98, to: 82, volume: 9, ticks: 20, attack: 20, decay: 330, release: 60 },
    start: #{ channel: ALERT, wave: "triangle", from: 262, to: 1046, volume: 11, ticks: 20, attack: 0, decay: 350, release: 60 },
    gameover: #{ channel: ALERT, wave: "saw", from: 440, to: 55, volume: 13, ticks: 75, attack: 0, decay: 1300, release: 200 },
    crash: #{ channel: HIT, wave: "noise", from: 1200, to: 300, volume: 11, ticks: 30, attack: 0, decay: 520, release: 80 },
};

// No sweeps in progress.
fn create() {
    [(), (), (), ()]
}

// Starts effect `name`, cutting off whatever its channel was playing.
fn play(active, name) {
    let effect = global::EFFECTS[name];
    let channel = effect.channel;
    audio::env(channel, effect.attack, effect.decay, 0, effect.release);
    audio::tone(channel, effect.from, effect.volume, effect.wave);
    audio::gate(channel, true);
    active[channel] = #{ effect: effect, step: 0 };
    active
}

// Advances every sweep by one tick and releases channels whose sweep ended.
fn update(active) {
    for slot in 0..global::CHANNELS {
        if active[slot] != () {
            let effect = active[slot].effect;
            let step = active[slot].step + 1;
            if step >= effect.ticks {
                audio::gate(effect.channel, false);
                active[slot] = ();
            } else {
                active[slot].step = step;
                let f = effect.from + ((effect.to - effect.from) * step).to_float() / effect.ticks.to_float();
                audio::tone(effect.channel, f.floor().to_int(), effect.volume, effect.wave);
            }
        }
    }
    active
}

// Closes every gate and mutes every oscillator, dropping all sweeps.
fn silence() {
    for channel in 0..global::CHANNELS {
        audio::gate(channel, false);
        audio::tone(channel, 0, 0, "square");
    }
    create()
}
