// Welcome to your new cartridge!
// A little owl waits under the night sky. Change anything you like,
// then type 'run' in the shell to see what you made.
//
// Rhai support is experimental: its scripting API may change.

// Colors are numbers from the 32-color system palette. Functions read
// these constants as `global::NAME`.
const SKY = 21; const STAR = 15; const MOON = 6;
const HILLS = 10; const GRASS = 9;
const FEATHER = 4; const SHADE = 3; const BELLY = 6;
const EYE = 16; const PUPIL = 11; const BEAK = 27; const FEET = 26;
const TITLE = 26; const TEXT = 16; const HINT = 14;

const SCREEN_W = 320; const SCREEN_H = 240;
const GROUND_Y = 212;
const CHAR_W = 6; // width of one character of text

// The owl is drawn on a 16x18 grid of big 2x2 pixels.
const SCALE = 2;
const OWL_W = 32; const OWL_H = 36;
const SPEED = 2;

const MESSAGES = [
    "Press Z to say hello!",
    "Hoot! Nice to meet you.",
    "Hoot hoot! Let's make a game.",
    "Every pixel here is yours to change.",
];

// Everything that changes while the cartridge runs lives in `this`, which
// boot(), tick() and draw() all share.
fn boot() {
    this.owl = #{ x: 144, y: global::GROUND_Y - global::OWL_H, facing: 1 };
    this.hoots = 0;
    this.hop_timer = 0; // ticks left in the current hop
    this.bubble_timer = 0; // ticks left to show "Hoot!"

    // Spread the stars over the sky. The same formula gives the same sky
    // every time the cartridge runs.
    this.stars = [];
    for i in 1..=40 {
        this.stars.push(#{ x: (i * 73) % global::SCREEN_W, y: 36 + (i * 37) % 120 });
    }
}

fn tick() {
    if btn(Button::Left) {
        this.owl.x -= global::SPEED;
        this.owl.facing = -1;
    }
    if btn(Button::Right) {
        this.owl.x += global::SPEED;
        this.owl.facing = 1;
    }
    if btn(Button::Up) {
        this.owl.y -= global::SPEED;
    }
    if btn(Button::Down) {
        this.owl.y += global::SPEED;
    }

    // Keep the whole owl on screen, between the title and the grass.
    this.owl.x = clamp(this.owl.x, 0, global::SCREEN_W - global::OWL_W);
    this.owl.y = clamp(this.owl.y, 40, global::GROUND_Y - global::OWL_H);

    if btnp(Button::A) {
        this.hoots += 1;
        this.hop_timer = 12;
        this.bubble_timer = 60;
    }
    this.hop_timer = max(this.hop_timer - 1, 0);
    this.bubble_timer = max(this.bubble_timer - 1, 0);
}

fn clamp(value, low, high) {
    if value < low { low } else if value > high { high } else { value }
}

// A filled circle, drawn one row at a time.
fn disc(cx, cy, r, color) {
    for dy in -r..=r {
        let half = (r * r - dy * dy).to_float().sqrt().floor().to_int();
        rectfill(cx - half, cy + dy, half * 2 + 1, 1, color);
    }
}

fn text_centered(message, y, color) {
    text(message, (global::SCREEN_W - message.len() * global::CHAR_W) / 2, y, color);
}

fn draw_sky() {
    cls(global::SKY);
    for (star, i) in this.stars {
        // Each star hides for a few ticks now and then, so the sky twinkles.
        if (frame() + (i + 1) * 13) % 90 > 5 {
            pset(star.x, star.y, global::STAR);
        }
    }
    disc(266, 56, 14, global::MOON);
    disc(272, 52, 12, global::SKY); // covers part of the moon to make a crescent
    disc(60, global::GROUND_Y + 70, 110, global::HILLS);
    disc(250, global::GROUND_Y + 90, 130, global::HILLS);
    rectfill(0, global::GROUND_Y, global::SCREEN_W, global::SCREEN_H - global::GROUND_Y, global::GRASS);
}

// Draws one block of the owl's 16x18 grid, with the owl at (x, y).
fn block(x, y, bx, by, w, h, color) {
    let s = global::SCALE;
    rectfill(x + bx * s, y + by * s, w * s, h * s, color);
}

fn draw_owl(x, y) {
    block(x, y, 1, 0, 3, 3, global::SHADE); // ear tufts
    block(x, y, 12, 0, 3, 3, global::SHADE);
    block(x, y, 0, 2, 16, 14, global::FEATHER); // head and body
    block(x, y, 0, 9, 2, 6, global::SHADE); // wings
    block(x, y, 14, 9, 2, 6, global::SHADE);
    block(x, y, 4, 10, 8, 6, global::BELLY);
    block(x, y, 7, 8, 2, 3, global::BEAK);
    block(x, y, 4, 16, 3, 2, global::FEET);
    block(x, y, 9, 16, 3, 2, global::FEET);

    // Blink for a moment every few seconds.
    if frame() % 180 < 6 {
        block(x, y, 2, 6, 5, 1, global::PUPIL);
        block(x, y, 9, 6, 5, 1, global::PUPIL);
    } else {
        block(x, y, 2, 4, 5, 5, global::EYE);
        block(x, y, 9, 4, 5, 5, global::EYE);
        // The pupils look the way the owl is facing.
        let look = if this.owl.facing > 0 { 2 } else { 1 };
        block(x, y, 2 + look, 5, 2, 2, global::PUPIL);
        block(x, y, 9 + look, 5, 2, 2, global::PUPIL);
    }
}

fn draw_bubble(x, y) {
    let w = 36;
    let h = 13;
    let bx = clamp(x + global::OWL_W / 2 - w / 2, 0, global::SCREEN_W - w);
    let by = y - h - 6;
    rectfill(bx, by, w, h, global::EYE);
    rectfill(x + global::OWL_W / 2 - 1, by + h, 3, 3, global::EYE); // the bubble's tail
    text("Hoot!", bx + 3, by + 2, global::PUPIL);
}

fn draw() {
    this.draw_sky();

    // Hop in a smooth arc while the hop timer runs.
    let hop = (sin(this.hop_timer.to_float() / 12.0 * PI()) * 8.0).floor().to_int();
    this.draw_owl(this.owl.x, this.owl.y - hop);
    if this.bubble_timer > 0 {
        draw_bubble(this.owl.x, this.owl.y - hop);
    }

    let messages = global::MESSAGES;
    text_centered("Welcome to Glaux!", 8, global::TITLE);
    text_centered(messages[this.hoots % messages.len()], 22, global::TEXT);
    text_centered("Arrows: move   Z: hoot", 224, global::HINT);
}
