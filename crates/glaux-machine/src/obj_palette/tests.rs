use super::*;
use pretty_assertions::assert_eq;

fn bank(n: u8) -> ObjBank {
    ObjBank::new(n).unwrap()
}

#[test]
fn bank_numbers_stop_at_the_last_bank() {
    assert_eq!(ObjBank::new(7).map(ObjBank::get), Some(7));
    assert_eq!(ObjBank::new(8), None);
    assert_eq!(ObjBank::new(u8::MAX), None);
}

#[test]
fn linear_bank_zero_is_the_identity() {
    let palettes = ObjPalettes::linear();
    for local in 1..16 {
        assert_eq!(palettes.resolve(ObjBank::ZERO, local), Some(local));
    }
}

#[test]
fn linear_banks_cover_consecutive_master_rows() {
    let palettes = ObjPalettes::linear();
    assert_eq!(palettes.resolve(bank(1), 1), Some(17));
    assert_eq!(palettes.resolve(bank(7), 15), Some(127));
    assert_eq!(
        ObjPalettes::from_bytes(palettes.as_bytes()),
        Ok(palettes.clone())
    );
}

#[test]
fn local_zero_and_out_of_range_locals_never_resolve() {
    let palettes = ObjPalettes::linear();
    for n in 0..8 {
        assert_eq!(palettes.resolve(bank(n), 0), None);
        assert_eq!(palettes.resolve(bank(n), 16), None);
    }
}

#[test]
fn resolves_through_the_selected_bank() {
    let mut bytes = [0; OBJ_PALETTES_BYTES];
    bytes[3 * 16 + 5] = 200;
    bytes[5] = 9;
    let palettes = ObjPalettes::from_bytes(&bytes).unwrap();
    assert_eq!(palettes.resolve(bank(3), 5), Some(200));
    assert_eq!(palettes.resolve(ObjBank::ZERO, 5), Some(9));
}

#[test]
fn rejects_wrong_lengths_and_reserved_entries() {
    assert_eq!(
        ObjPalettes::from_bytes(&[0; 127]),
        Err(ObjPalettesError::Length { length: 127 })
    );
    assert_eq!(
        ObjPalettes::from_bytes(&[0; 129]),
        Err(ObjPalettesError::Length { length: 129 })
    );
    let mut bytes = [0; OBJ_PALETTES_BYTES];
    bytes[2 * 16] = 4;
    assert_eq!(
        ObjPalettes::from_bytes(&bytes),
        Err(ObjPalettesError::ReservedEntry { bank: 2, value: 4 })
    );
}
