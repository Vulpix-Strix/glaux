// Signal Lost. survive the dungeon arena for as long as you can.
//
// A restart rebuilds the run's state inside the same runtime; only the
// PRNG stream, the best score and the run counter carry over.
//
// All game state lives in `this`. Module functions cannot reach it and get
// copies of what they are passed, so the helpers below that need the PRNG
// or the sound sweeps are methods of `this` (`this.burst(..)`,
// `this.play(..)`) and store what the module functions return.

import "arena" as arena;
import "enemies" as enemies;
import "particles" as particles;
import "pickups" as pickups;
import "player" as player;
import "projectiles" as projectiles;
import "audio" as sfx;
import "sheet" as sheet;
import "util" as util;

const SEED = 0x5167A1;
const MILESTONE_TICKS = 30 * 60;
const GAMEOVER_LOCK = 60;
const FLASK_CHANCE = 0.04;

const WHITE = 15;
const LIME = 7;
const GRAY = 13;
const DARK = 11;
const ORANGE = 5;
const PALE = 6;

// State that lives for one run.
fn new_run() {
    #{
        player: player::create(),
        enemies: enemies::create(),
        axes: [],
        pickups: [],
        sparks: [],
        remains: [],
        score: 0,
        kills: 0,
        start_frame: frame(),
        start_time: t(),
        elapsed: 0,
        survived: 0.0,
        level: 0,
        warned_cap: false,
    }
}

fn boot() {
    this.rng = util::rng_new(global::SEED);
    this.state = "title";
    this.run = ();
    this.best = 0;
    this.runs = 0;
    this.lock = 0;
    this.sfx = sfx::silence();
    log::info("Signal Lost ready");
}

// Starts sound effect `name`.
fn play(name) {
    this.sfx = sfx::play(this.sfx, name);
}

// Uniform integer in [lo, hi] from the game's PRNG.
fn rand_int(lo, hi) {
    this.rng = util::rng_next(this.rng);
    util::rng_int(this.rng, lo, hi)
}

// True with probability `p`, from the game's PRNG.
fn chance(p) {
    this.rng = util::rng_next(this.rng);
    util::rng_chance(this.rng, p)
}

// A burst of sparks in the current run; see `particles::burst`.
fn burst(x, y, count, speed, life, colors) {
    let r = particles::burst(this.run.sparks, this.rng, x, y, count, speed, life, colors);
    this.run.sparks = r.list;
    this.rng = r.rng;
}

fn start_run() {
    this.sfx = sfx::silence();
    this.runs += 1;
    this.run = new_run();
    this.state = "playing";
    this.play("start");
    if this.runs == 1 {
        log::info("run 1 started");
    } else {
        log::info(`restart: run ${this.runs}`);
    }
    // Luau's log functions join their arguments with tabs.
    log::trace("rng state\t" + this.rng);
}

fn game_over() {
    this.state = "gameover";
    this.lock = util::writable(global::GAMEOVER_LOCK);
    this.best = max(this.best, this.run.score);
    this.sfx = sfx::silence();
    this.play("gameover");
    this.play("crash");
    let c = player::center(this.run.player);
    this.burst(c[0], c[1], 40, 2.5, 30, [global::WHITE, global::PALE, global::ORANGE]);
    log::info(`signal lost: score ${this.run.score}, ${this.run.kills} kills, survived ${util::clock(this.run.elapsed)}`);
}

fn kill(e) {
    let kind = enemies::KINDS[e.kind];
    let c = enemies::center(e);
    let cx = c[0];
    let cy = c[1];
    this.run.score += kind.score;
    this.run.kills += 1;
    this.burst(cx, cy, 14, 2, 14, [global::WHITE, global::PALE, global::LIME, 8]);
    this.run.remains = particles::remains(this.run.remains, cx, cy);
    if e.kind == "ogre" {
        for i in 1..=5 {
            this.run.pickups = pickups::drop(this.run.pickups, "coin", cx + (i - 3) * 6, cy + this.rand_int(-6, 6));
        }
        this.run.pickups = pickups::drop(this.run.pickups, "flask", cx, cy + 10);
    } else if this.chance(global::FLASK_CHANCE) {
        this.run.pickups = pickups::drop(this.run.pickups, "flask", cx, cy);
    } else if e.kind == "orc" || this.chance(0.5) {
        this.run.pickups = pickups::drop(this.run.pickups, "coin", cx, cy);
    }
    this.play("kill");
}

// Axes against enemies: each axe stops at the first enemy it touches.
fn resolve_axes() {
    for a in 0..this.run.axes.len() {
        let axe = this.run.axes[a];
        if !axe.dead {
            let ax = axe.x + projectiles::HIT_X;
            let ay = axe.y + projectiles::HIT_Y;
            for i in 0..this.run.enemies.list.len() {
                let e = this.run.enemies.list[i];
                if e.hp > 0 {
                    let b = enemies::box(e);
                    if util::overlap(ax, ay, projectiles::HIT_W, projectiles::HIT_H, b[0], b[1], b[2], b[3]) {
                        this.run.axes[a].dead = true;
                        let d = util::normalize(axe.vx, axe.vy);
                        e = enemies::hit(e, d[0], d[1]);
                        this.run.enemies.list[i] = e;
                        if e.hp <= 0 {
                            this.kill(e);
                        } else {
                            this.burst(ax + 4, ay + 5, 5, 1.6, 8, [global::WHITE, global::ORANGE]);
                            this.play("hit");
                        }
                        break;
                    }
                }
            }
        }
    }
    this.run.enemies = enemies::remove_dead(this.run.enemies);
    this.run.axes.retain(|axe| !axe.dead);
}

// Enemies against the knight. Returns whether the knight died.
fn resolve_contact() {
    let p = this.run.player;
    if !player::vulnerable(p) {
        return false;
    }
    let bx = p.x + player::HIT_X;
    let by = p.y + player::HIT_Y;
    for e in this.run.enemies.list {
        let b = enemies::box(e);
        if util::overlap(bx, by, player::HIT_W, player::HIT_H, b[0], b[1], b[2], b[3]) {
            let c = enemies::center(e);
            this.run.player = player::damage(this.run.player, enemies::KINDS[e.kind].damage, c[0], c[1]);
            let pc = player::center(this.run.player);
            this.burst(pc[0], pc[1], 10, 1.8, 12, [global::ORANGE, 4, 3]);
            this.play("hurt");
            return this.run.player.hp == 0;
        }
    }
    false
}

fn tick_playing() {
    this.run.elapsed = frame() - this.run.start_frame;
    this.run.survived = t() - this.run.start_time;

    let moved = player::update(this.run.player);
    this.run.player = moved.player;
    let c = player::center(this.run.player);
    let px = c[0];
    let py = c[1];
    if moved.dashed {
        this.play("dash");
        this.burst(px, py + 8, 8, 1.2, 10, [global::GRAY, 14]);
    }
    if this.run.player.dash > 0 {
        this.run.sparks = particles::add(this.run.sparks, px + this.rand_int(-3, 3), py + 10, 0, 0, 8, 14);
    }
    if moved.fire {
        let p = this.run.player;
        let thrown = projectiles::throw_axe(this.run.axes, px, py, p.aim_x, p.aim_y);
        this.run.axes = thrown.list;
        if thrown.thrown {
            this.play("fire");
        }
    }

    let spawned = enemies::advance_spawns(this.run.enemies, this.rng, this.run.elapsed, px, py);
    this.run.enemies = spawned.state;
    this.rng = spawned.rng;
    for kind in spawned.appeared {
        if kind == "ogre" {
            this.play("heavy");
            debug(`an ogre entered at ${util::clock(this.run.elapsed)}`);
        }
    }
    if spawned.throttled && !this.run.warned_cap {
        this.run.warned_cap = true;
        log::warn(`enemy cap reached at ${util::clock(this.run.elapsed)}; spawns are being skipped`);
    }

    this.run.enemies = enemies::update(this.run.enemies, px, py);
    this.run.axes = projectiles::update(this.run.axes);
    this.resolve_axes();

    let p = this.run.player;
    let picked = pickups::update(this.run.pickups, px, py, p.x + player::HIT_X, p.y + player::HIT_Y, player::HIT_W, player::HIT_H);
    this.run.pickups = picked.list;
    for kind in picked.collected {
        if kind == "coin" {
            this.run.score += pickups::COIN_SCORE;
            this.play("coin");
        } else {
            this.run.player = player::heal(this.run.player, pickups::FLASK_HEAL);
            this.play("heal");
        }
        this.burst(px, py, 6, 1.4, 10, [global::PALE, global::LIME]);
    }

    this.run.sparks = particles::update(this.run.sparks);
    this.run.remains = particles::update_remains(this.run.remains);

    let level = this.run.elapsed / global::MILESTONE_TICKS;
    if level > this.run.level {
        this.run.level = level;
        let d = enemies::difficulty(this.run.elapsed);
        log::info(`difficulty ${level} at ${util::clock(this.run.elapsed)}: spawn every ${d.interval} ticks, ${d.batch} per wave`);
    }

    if this.resolve_contact() {
        this.game_over();
    }
}

fn tick() {
    this.sfx = sfx::update(this.sfx);
    if this.state == "title" {
        if btnp(Button::A) {
            this.start_run();
        }
    } else if this.state == "playing" {
        this.tick_playing();
    } else {
        this.run.sparks = particles::update(this.run.sparks);
        if this.lock > 0 {
            this.lock -= 1;
        } else if btnp(Button::A) {
            this.start_run();
        }
    }
}

// Text with a one-pixel drop shadow, readable over sprites and bricks.
fn label(message, x, y, color) {
    text(message, x + 1, y + 1, global::DARK);
    text(message, x, y, color);
}

fn centered(message, y, color) {
    label(message, (320 - message.len() * 6) / 2, y, color);
}

fn draw_hud() {
    rectfill(0, 0, 320, 16, global::DARK);

    let hp = this.run.player.hp;
    for i in 1..=(player::MAX_HP / 2) {
        let tile = sheet::HEART_EMPTY;
        if hp >= i * 2 {
            tile = sheet::HEART_FULL;
        } else if hp == i * 2 - 1 {
            tile = sheet::HEART_HALF;
        }
        spr(tile, 2 + (i - 1) * 14, 0, 2, 2);
    }
    label("SCORE " + util::pad(this.run.score, 5), 84, 3, global::WHITE);
    label("TIME " + util::clock((this.run.survived * 60.0).floor().to_int()), 168, 3, global::WHITE);

    let cooldown = this.run.player.dash_cooldown;
    if cooldown == 0 {
        label("DASH", 276, 3, global::LIME);
    } else {
        label("DASH", 276, 3, global::GRAY);
        // Recharge bar under the label.
        let filled = (24.0 * (1.0 - cooldown.to_float() / player::DASH_COOLDOWN.to_float())).floor().to_int();
        if filled > 0 {
            rectfill(276, 13, filled, 1, global::LIME);
        }
    }
}

fn draw_world(tick) {
    particles::draw_remains(this.run.remains, tick);
    pickups::draw(this.run.pickups, tick);
    enemies::draw(this.run.enemies, tick);
    if this.state == "playing" {
        player::draw(this.run.player, tick);
    }
    projectiles::draw(this.run.axes, tick);
    particles::draw(this.run.sparks);
    this.draw_hud();
}

fn draw_title(tick) {
    // A small cast lineup: the knight between the three enemy kinds.
    let step = (tick / 7) % 4;
    spr(sheet::OGRE_RUN[step], 64, 108, 4, 4);
    spr(sheet::ORC_RUN[step], 108, 116, 2, 3);
    spr(sheet::GOBLIN_RUN[step], 132, 124, 2, 2);
    spr(sheet::KNIGHT_IDLE[(tick / 8) % 4], 196, 116, 2, 3, true);
    spr(sheet::COIN[(tick / 6) % 4], 176, 128);

    centered("SIGNAL LOST", 64, global::ORANGE);
    centered("SURVIVE THE ARENA", 80, global::PALE);
    // Prompts name the default desktop keys (Z = A, X = B), not the
    // virtual buttons the code reads.
    if (tick / 30) % 2 == 0 {
        centered("PRESS Z TO START", 164, global::WHITE);
    }
    centered("ARROWS MOVE   Z THROW   X DASH", 190, global::GRAY);
    centered("HOLD Z TO LOCK YOUR AIM", 202, global::GRAY);
    if this.best > 0 {
        centered("BEST " + this.best, 220, global::LIME);
    }
}

fn draw_gameover(tick) {
    centered("SIGNAL LOST", 92, global::ORANGE);
    centered(`SCORE ${this.run.score}   KILLS ${this.run.kills}   TIME ${util::clock(this.run.elapsed)}`, 110, global::WHITE);
    centered("BEST " + this.best, 124, global::LIME);
    if this.lock == 0 && (tick / 30) % 2 == 0 {
        centered("PRESS Z TO RETRY", 146, global::WHITE);
    }
}

fn draw() {
    let tick = frame();
    arena::draw();
    if this.state == "title" {
        this.draw_title(tick);
    } else {
        this.draw_world(tick);
        if this.state == "gameover" {
            this.draw_gameover(tick);
        }
    }
}
