use super::*;
use crate::memory_map::DISPLAY_ENABLE;
use pretty_assertions::assert_eq;

/// Offsets from `DISPLAY_ENABLE` of the 17 bytes that hold registers.
const REGISTER_OFFSETS: [usize; 17] = [
    0x00, 0x01, 0x10, 0x11, 0x12, 0x13, 0x14, 0x20, 0x21, 0x22, 0x23, 0x24, 0x30, 0x31, 0x32, 0x33,
    0x34,
];

#[test]
fn registers_sit_at_the_approved_addresses() {
    assert_eq!(DISPLAY_ENABLE, 0x3C000);
    assert_eq!(BACKDROP_COLOR, 0x3C001);
    assert_eq!(BG_REGISTER_BLOCKS, [0x3C010, 0x3C020, 0x3C030]);
    assert_eq!((BG_SCROLL_X, BG_SCROLL_Y, BG_PRIORITY), (0, 2, 4));
    assert_eq!(REGISTER_BYTES, 0x35);
}

#[test]
fn only_the_seventeen_register_bytes_exist() {
    let mut registers = GraphicsRegisters::new();
    for register in 0..REGISTER_BYTES + 8 {
        let exists = REGISTER_OFFSETS.contains(&register);
        assert_eq!(registers.byte(register).is_some(), exists, "{register:#x}");
        assert_eq!(registers.set_byte(register, 0xFF), exists, "{register:#x}");
    }
}

#[test]
fn registers_start_at_their_power_on_values() {
    let registers = GraphicsRegisters::new();
    assert_eq!(registers.display_enable(), 0);
    assert_eq!(registers.backdrop_color(), 0);
    for (layer, priority) in BgLayer::ALL.into_iter().zip([3, 2, 1]) {
        assert_eq!(registers.bg_scroll_x(layer), 0);
        assert_eq!(registers.bg_scroll_y(layer), 0);
        assert_eq!(registers.bg_priority(layer), priority);
    }
    for register in REGISTER_OFFSETS {
        let initial = match register {
            0x14 => 3,
            0x24 => 2,
            0x34 => 1,
            _ => 0,
        };
        assert_eq!(registers.byte(register), Some(initial), "{register:#x}");
    }
}

#[test]
fn reserved_bits_read_as_zero_and_writes_keep_the_rest() {
    let mut registers = GraphicsRegisters::new();
    registers.set_byte(0, 0xFF);
    assert_eq!(registers.display_enable(), 0x0F);
    registers.set_byte(0, 0b1010_0101);
    assert_eq!(registers.display_enable(), 0b0101);

    registers.set_byte(1, 0xFF);
    assert_eq!(registers.backdrop_color(), 0xFF);

    registers.set_byte(0x14, 0xFF);
    assert_eq!(registers.bg_priority(BgLayer::Bg0), 3);
    registers.set_byte(0x14, 0b100);
    assert_eq!(registers.bg_priority(BgLayer::Bg0), 0);
}

#[test]
fn scrolls_keep_their_full_sixteen_bit_value() {
    let mut registers = GraphicsRegisters::new();
    for (offset, byte) in [(0x20, 0x58), (0x21, 0x02), (0x22, 0xFF), (0x23, 0xFF)] {
        registers.set_byte(offset, byte);
    }
    assert_eq!(registers.bg_scroll_x(BgLayer::Bg1), 600);
    assert_eq!(registers.bg_scroll_y(BgLayer::Bg1), 0xFFFF);
}

#[test]
fn the_display_bits_select_the_layers_and_objects() {
    assert_eq!(
        [DISPLAY_BG0, DISPLAY_BG1, DISPLAY_BG2, DISPLAY_OBJ],
        [0b0001, 0b0010, 0b0100, 0b1000]
    );
    assert_eq!(DISPLAY_ENABLE_MASK, 0x0F);
    let mut registers = GraphicsRegisters::new();
    registers.set_byte(0, 0xFF);
    assert_eq!(registers.display_enable(), DISPLAY_ENABLE_MASK);
}
