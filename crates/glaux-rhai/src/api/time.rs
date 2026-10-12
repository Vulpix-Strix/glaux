//! `frame` and `t`.

use std::cell::Cell;
use std::rc::Rc;

use glaux_machine::machine::TICK_RATE;
use rhai::{Engine, FLOAT, INT};

/// Registers `frame` and `t`.
pub(crate) fn install(engine: &mut Engine, counter: &Rc<Cell<u64>>) {
    // Saturates instead of wrapping; no run reaches `INT::MAX` ticks.
    let ticks = Rc::clone(counter);
    engine.register_fn("frame", move || -> INT {
        INT::try_from(ticks.get()).unwrap_or(INT::MAX)
    });

    let ticks = Rc::clone(counter);
    engine.register_fn("t", move || -> FLOAT {
        ticks.get() as FLOAT / FLOAT::from(TICK_RATE)
    });
}
