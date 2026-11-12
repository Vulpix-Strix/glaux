//! Lending the cartridge hardware to a running script.
//!
//! The hardware has one owner at a time. A runtime borrows it for one phase
//! of the script by moving its `Box` into a [`HardwareSlot`] and moves it
//! back when the phase ends, so there is never a copy or a second source of
//! truth.

use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;

use glaux_machine::hardware::CartridgeHardware;

use crate::error::{Phase, RuntimeError, ScriptError};

/// Where a runtime keeps the hardware while a phase runs.
///
/// The slot is empty except during a [`Loan`]. It only lets a holder use the
/// hardware itself, so nothing that holds a clone of it can take the `Box`
/// out or put another one in.
#[derive(Clone, Default)]
pub struct HardwareSlot(Rc<RefCell<Option<Box<CartridgeHardware>>>>);

impl fmt::Debug for HardwareSlot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HardwareSlot")
            .field("lent", &self.0.borrow().is_some())
            .finish()
    }
}

impl HardwareSlot {
    pub fn new() -> Self {
        Self::default()
    }

    /// Runs `f` on the lent hardware, or returns `None` when none is lent.
    pub fn with<R>(&self, f: impl FnOnce(&mut CartridgeHardware) -> R) -> Option<R> {
        self.0.borrow_mut().as_deref_mut().map(f)
    }
}

/// Why the hardware could not be lent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum LoanError {
    #[error("the cartridge hardware is not available to lend")]
    Unavailable,
    #[error("the cartridge hardware is already lent")]
    AlreadyLent,
}

impl LoanError {
    /// The error a phase reports when it could not start for lack of
    /// hardware. It is a bug in who owns the hardware, not in the script.
    pub fn in_phase(self, phase: Phase) -> RuntimeError {
        RuntimeError {
            phase,
            error: ScriptError::Runtime(format!("internal error: {self}")),
        }
    }
}

/// The hardware, lent from its owner to a slot for as long as this lives.
///
/// - The slot is empty when the loan starts. Otherwise the `Box` already in
///   it would be replaced and lost, so the loan is refused.
/// - The owner must have the hardware. Otherwise the phase has nothing to
///   run on, and the loan is refused.
/// - A refused loan moves nothing.
/// - The `Box` moves twice and is never cloned: owner to slot here, slot to
///   owner on drop. The owner is borrowed exclusively until then, so
///   nothing can put hardware there in between.
/// - Dropping the loan returns the hardware also when the phase ends in an
///   error or unwinds from a panic.
pub struct Loan<'a> {
    slot: &'a HardwareSlot,
    home: &'a mut Option<Box<CartridgeHardware>>,
}

impl<'a> Loan<'a> {
    pub fn new(
        slot: &'a HardwareSlot,
        home: &'a mut Option<Box<CartridgeHardware>>,
    ) -> Result<Self, LoanError> {
        {
            let mut lent = slot.0.borrow_mut();
            if lent.is_some() {
                return Err(LoanError::AlreadyLent);
            }
            *lent = Some(home.take().ok_or(LoanError::Unavailable)?);
        }
        Ok(Self { slot, home })
    }
}

impl Drop for Loan<'_> {
    fn drop(&mut self) {
        // A borrow that is still open can only come from a leaked guard. The
        // hardware then stays in the slot and the next loan is refused,
        // which is better than panicking while unwinding.
        if let Ok(mut lent) = self.slot.0.try_borrow_mut() {
            *self.home = lent.take();
        }
    }
}

#[cfg(test)]
mod tests;
