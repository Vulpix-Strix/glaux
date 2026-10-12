// Dropped signal coins and healing flasks.

import "sheet" as sheet;
import "util" as util;

const MAX = 30;
const MAX_FLASKS = 3;
const LIFETIME = 600;
const BLINK_FROM = 120;
const MAGNET_RADIUS = 40;
const MAGNET_SPEED = 2.2;

export const COIN_SCORE = 5;
export const FLASK_HEAL = 4;

// Drops a pickup of `kind` ("coin" or "flask") centered on `(cx, cy)`.
// Returns the list.
fn drop(list, kind, cx, cy) {
    if list.len() >= global::MAX {
        return list;
    }
    if kind == "flask" {
        let flasks = 0;
        for p in list {
            if p.kind == "flask" {
                flasks += 1;
            }
        }
        if flasks >= global::MAX_FLASKS {
            return list;
        }
    }
    let size = if kind == "coin" { 8 } else { 16 };
    let half = size.to_float() / 2.0;
    list.push(#{ kind: kind, x: cx - half, y: cy - half, size: size, life: util::writable(global::LIFETIME) });
    list
}

// Ages pickups and pulls nearby ones toward `(px, py)`. Returns
// `#{ list, collected }`, `collected` holding the kinds collected by the
// box `(bx, by, bw, bh)` this tick.
fn update(list, px, py, bx, by, bw, bh) {
    let collected = [];
    for i in 0..list.len() {
        list[i].life -= 1;
        let half = list[i].size.to_float() / 2.0;
        let dx = px - (list[i].x + half);
        let dy = py - (list[i].y + half);
        if dx * dx + dy * dy < global::MAGNET_RADIUS * global::MAGNET_RADIUS {
            let n = util::normalize(dx, dy);
            list[i].x += n[0] * global::MAGNET_SPEED;
            list[i].y += n[1] * global::MAGNET_SPEED;
        }
        // The flask art leaves a margin; only its bottle counts.
        let p = list[i];
        let inset = if p.kind == "flask" { 4 } else { 1 };
        if util::overlap(p.x + inset, p.y + inset, p.size - inset * 2, p.size - inset * 2, bx, by, bw, bh) {
            collected.push(p.kind);
            list[i].life = 0;
        }
    }
    list.retain(|p| p.life > 0);
    #{ list: list, collected: collected }
}

fn draw(list, tick) {
    let coin = sheet::COIN[(tick / 6) % sheet::COIN.len()];
    for p in list {
        if p.life > global::BLINK_FROM || (tick / 4) % 2 == 0 {
            if p.kind == "coin" {
                spr(coin, p.x, p.y);
            } else {
                spr(sheet::FLASK, p.x, p.y, 2, 2);
            }
        }
    }
}
