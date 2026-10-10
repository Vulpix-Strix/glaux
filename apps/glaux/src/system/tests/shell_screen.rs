use super::*;
use pretty_assertions::assert_eq;

#[test]
fn typed_text_reaches_shell() {
    let mut system = shell();
    system = system.update(&[InputEvent::Text('H'), InputEvent::Text('I')]);
    system = system.update(&[InputEvent::Left { select: false }]);

    let mut expected = Shell::new(info(), cartridges());
    for event in [
        InputEvent::Text('H'),
        InputEvent::Text('I'),
        InputEvent::Left { select: false },
    ] {
        expected.handle_input(event, CartridgeSlot::Empty);
    }
    assert_eq!(shell_of(&system), &expected);
}

#[test]
fn typed_text_renders_after_prompt() {
    let mut system = shell();
    let empty = render(&system);
    system = system.update(&[InputEvent::Text('A')]);
    let typed = render(&system);

    // Cell columns 2 and 3 follow the two-char prompt. The cursor starts
    // in column 2; typing puts 'A' there and moves the cursor to 3. Fake
    // glyphs leave column 3 of each cell empty, so only the cursor block
    // fills it.
    let y = 2 * CELL_H;
    let glyph = |fb: &Framebuffer, col: i32| fb.pixel((col + 1) * CELL_W, y);
    let block = |fb: &Framebuffer, col: i32| fb.pixel((col + 1) * CELL_W + 3, y);
    assert_eq!(
        (block(&empty, 2), block(&empty, 3)),
        (Some(TEXT), Some(BACKGROUND))
    );
    assert_eq!(glyph(&typed, 2), Some(TEXT));
    assert_eq!(
        (block(&typed, 2), block(&typed, 3)),
        (Some(BACKGROUND), Some(TEXT))
    );
}

#[test]
fn cursor_cell_blinks_until_input_restarts_it() {
    // The cursor cell after the prompt on row 1, outside the 3×3 area
    // the fake font inks for every character, space included, so only
    // the block's fill shows there.
    let cell = |fb: &Framebuffer| {
        let (x, y) = (3 * CELL_W, 2 * CELL_H);
        (0..CELL_H)
            .flat_map(|dy| (0..CELL_W).map(move |dx| (dx, dy)))
            .filter(|&(dx, dy)| dx >= 3 || dy >= 3)
            .map(|(dx, dy)| fb.pixel(x + dx, y + dy))
            .collect::<Vec<_>>()
    };
    let all = |color| vec![Some(color); (CELL_W * CELL_H - 9) as usize];

    let mut system = shell();
    assert_eq!(cell(&render(&system)), all(TEXT));
    for _ in 0..shell::BLINK_TICKS {
        system = system.update(&[]);
    }
    assert_eq!(cell(&render(&system)), all(BACKGROUND));

    system = system.update(&[InputEvent::Text('A'), InputEvent::Backspace]);
    assert_eq!(cell(&render(&system)), all(TEXT));
}

#[test]
fn submitted_line_stays_on_screen_above_a_new_prompt() {
    let mut system = shell();
    system = system.update(&[InputEvent::Text('A'), InputEvent::Enter]);

    let fb = render(&system);
    // Row 1 keeps "> A"; the error follows on row 2 and the new, empty
    // prompt on row 3 with its cursor block in column 2.
    let glyph = |col: i32, row: i32| fb.pixel((col + 1) * CELL_W, (row + 1) * CELL_H);
    assert_eq!(glyph(2, 1), Some(TEXT));
    assert_eq!(glyph(0, 2), Some(ERROR));
    assert_eq!(glyph(0, 3), Some(TEXT));
    assert_eq!(fb.pixel(3 * CELL_W + 3, 4 * CELL_H), Some(TEXT));
    assert!(!row_has_ink(&fb, 4));
}

#[test]
fn selected_cells_render_inverse() {
    let mut system = shell();
    system = system.update(&[
        InputEvent::Text('A'),
        InputEvent::Text('B'),
        InputEvent::Left { select: false },
        InputEvent::Right { select: true },
    ]);
    let fb = render(&system);

    // Fake glyphs leave column 3 of each 4-pixel cell empty.
    let y = 2 * CELL_H;
    let gap = |cell: i32| fb.pixel((cell + 1) * CELL_W + 3, y);
    assert_eq!(gap(2), Some(BACKGROUND), "unselected 'A'");
    assert_eq!(gap(3), Some(TEXT), "selected 'B'");
    assert_eq!(fb.pixel(4 * CELL_W, y), Some(BACKGROUND), "glyph of 'B'");
}

#[test]
fn text_rows_leave_a_cell_of_margin_top_and_bottom() {
    assert_eq!(text_rows(CELL_H), 240 / 5 - 2);
    assert_eq!(text_rows(10), 22);
    assert_eq!(text_rows(400), 0);
}

#[test]
fn text_columns_leave_a_cell_of_margin_on_each_side() {
    assert_eq!(text_columns(CELL_W), 320 / 4 - 2);
    assert_eq!(text_columns(5), 62);
    assert_eq!(text_columns(400), 0);
}

#[test]
fn long_command_line_wraps_onto_next_row() {
    let mut system = shell();
    let fits = text_columns(CELL_W) - 2;
    let mut input = vec![InputEvent::Text('o'); fits];
    system = system.update(&input);
    // The cursor sits in the first cell of row 2 once row 1 is full.
    assert!(row_has_ink(&render(&system), 2));

    // Glyph pixels of the first cell on row 2 (margin + row offset).
    let first_cell_of_row_2 = |fb: &Framebuffer| fb.pixel(CELL_W, 3 * CELL_H);
    input.truncate(1);
    system = system.update(&input);
    let fb = render(&system);
    assert_eq!(first_cell_of_row_2(&fb), Some(TEXT));
    assert!(row_has_ink(&fb, 2));
    // Nothing is drawn past the right margin.
    let right_margin = 320 - CELL_W;
    assert!((0..240).all(|y| (right_margin..320).all(|x| fb.pixel(x, y) == Some(BACKGROUND))));
}

#[test]
fn render_redraws_from_scratch() {
    let rom = fake_rom();
    for system in [boot_at(BootStage::Audio), shell()] {
        let mut first = Framebuffer::new();
        let mut second = Framebuffer::new();
        second.clear(200);
        second.set_pixel(300, 230, 3);

        system.render(&mut first, &rom, None);
        system.render(&mut second, &rom, None);
        assert_frame_eq!(first, second);
    }
}
