use std::panic::{AssertUnwindSafe, catch_unwind};

use glaux_machine::bg::BgVram;
use glaux_machine::palette::Palette;
use glaux_machine::sprite::ObjVram;
use pretty_assertions::assert_eq;

use super::*;

fn hardware() -> Box<CartridgeHardware> {
    Box::new(CartridgeHardware::new(
        Palette::default(),
        ObjVram::default(),
        BgVram::default(),
    ))
}

fn address(home: &Option<Box<CartridgeHardware>>) -> *const CartridgeHardware {
    &**home.as_ref().unwrap()
}

#[test]
fn a_slot_is_empty_until_a_loan() {
    let slot = HardwareSlot::new();
    assert_eq!(slot.with(|_| ()), None);
}

#[test]
fn writes_made_during_the_loan_are_in_the_owners_hardware_afterwards() {
    let slot = HardwareSlot::new();
    let mut home = Some(hardware());
    let before = address(&home);
    {
        let loan = Loan::new(&slot, &mut home).unwrap();
        assert_eq!(slot.with(|hardware| hardware.work_ram[7] = 9), Some(()));
        drop(loan);
    }
    assert_eq!(address(&home), before);
    assert_eq!(home.unwrap().work_ram[7], 9);
}

#[test]
fn the_owner_has_nothing_while_the_hardware_is_lent() {
    let slot = HardwareSlot::new();
    let mut home = Some(hardware());
    let loan = Loan::new(&slot, &mut home).unwrap();
    assert!(slot.with(|_| ()).is_some());
    drop(loan);
    assert!(home.is_some());
    assert_eq!(slot.with(|_| ()), None);
}

#[test]
fn a_loan_without_hardware_is_refused_and_moves_nothing() {
    let slot = HardwareSlot::new();
    let mut home = None;
    assert_eq!(
        Loan::new(&slot, &mut home).err(),
        Some(LoanError::Unavailable)
    );
    assert!(home.is_none());
    assert_eq!(slot.with(|_| ()), None);
}

#[test]
fn a_loan_into_a_full_slot_is_refused_and_keeps_both_boxes() {
    let slot = HardwareSlot::new();
    let mut first = Some(hardware());
    let first_address = address(&first);
    let _loan = Loan::new(&slot, &mut first).unwrap();

    let mut second = Some(hardware());
    let second_address = address(&second);
    assert_eq!(
        Loan::new(&slot, &mut second).err(),
        Some(LoanError::AlreadyLent)
    );
    assert_eq!(address(&second), second_address);
    assert_ne!(first_address, second_address);
    assert!(slot.with(|_| ()).is_some());
}

#[test]
fn the_hardware_returns_when_the_phase_panics() {
    let slot = HardwareSlot::new();
    let mut home = Some(hardware());
    let before = address(&home);
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        let _loan = Loan::new(&slot, &mut home).unwrap();
        panic!("the phase failed");
    }));
    assert!(outcome.is_err());
    assert_eq!(address(&home), before);
    assert_eq!(slot.with(|_| ()), None);
}

#[test]
fn a_refused_loan_reports_its_phase_as_an_internal_error() {
    let error = LoanError::Unavailable.in_phase(Phase::Draw);
    assert_eq!(error.phase, Phase::Draw);
    assert_eq!(
        error.error,
        ScriptError::Runtime(
            "internal error: the cartridge hardware is not available to lend".into()
        )
    );
}
