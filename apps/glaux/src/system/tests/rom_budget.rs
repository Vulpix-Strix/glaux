//! Cart ROM use as the workspace shows it, through the whole system.

use super::*;
use crate::cartridge::rom::{CART_ROM_BUDGET, RomError};
use pretty_assertions::assert_eq;

fn write_asset(fixture: &Fixture, path: &str, size: usize) {
    let file = fixture.root().join("pong").join(path);
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(file, vec![0; size]).unwrap();
}

#[test]
fn opening_the_workspace_measures_the_cartridge() {
    let fixture = Fixture::new("sys-rom-open");
    write_asset(&fixture, "assets/hit.wav", 1000);
    write_asset(&fixture, "README.md", 5000);

    let system = press_break(loaded(&fixture));

    let manifest = std::fs::metadata(fixture.root().join("pong/cartridge.ron"))
        .unwrap()
        .len();
    let usage = system.workspace.rom().unwrap();
    assert_eq!(usage.used, manifest + 1000);
    assert!(!usage.incomplete());
}

#[test]
fn a_cartridge_over_the_budget_still_opens_and_runs() {
    let fixture = Fixture::new("sys-rom-over");
    write_asset(&fixture, "assets/big.bin", 2 * CART_ROM_BUDGET as usize);

    let system = press_break(loaded(&fixture));

    assert!(is_workspace(&system));
    let usage = system.workspace.rom().unwrap();
    assert!(usage.excess() >= CART_ROM_BUDGET);
    assert!(matches!(usage.check(), Err(RomError::OverBudget { .. })));
    let system = press_break(system);
    assert!(is_running(&enter(system, "run")));
}
