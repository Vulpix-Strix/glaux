//! The memory functions as scripts see them. The bus itself is tested in
//! `glaux-machine`, so these tests cover argument handling, errors, cost and
//! the phases the functions run in.

use glaux_machine::memory_map::FRAMEBUFFER;
use pretty_assertions::assert_eq;

use super::*;

const SPENT: Budget = Budget { interrupts: 100 };

fn started_with_budget(source: &str, budget: Budget) -> Runtime {
    match Runtime::start_on_scratch(
        "main.luau",
        source.as_bytes(),
        Files::default(),
        budget,
        font(),
    ) {
        Ok(runtime) => runtime,
        Err(failure) => panic!("expected the script to start, got {failure:?}"),
    }
}

/// Runs `body` as `tick()` and returns the runtime for inspecting memory.
fn ticked(body: &str) -> Runtime {
    let mut runtime = started(&format!("function tick() {body} end"));
    runtime.tick(NO_INPUT).unwrap();
    runtime
}

/// The message of the runtime error `body` raises in `tick()`, and the
/// runtime, whose memory must be unchanged.
fn tick_error(body: &str) -> (String, Runtime) {
    let mut runtime = started(&format!("function tick() {body} end"));
    match runtime.tick(NO_INPUT) {
        Err(RuntimeError {
            phase: Phase::Tick,
            error: ScriptError::Runtime(message),
        }) => (message, runtime),
        other => panic!("`{body}` should raise an error in tick, got {other:?}"),
    }
}

fn ram(runtime: &Runtime, range: std::ops::Range<usize>) -> Vec<u8> {
    runtime.scratch().work_ram[range].to_vec()
}

#[test]
fn whole_numbers_work_however_they_are_written() {
    let mut runtime = started(
        "function tick() poke(3.0, 7.0) poke16(0x10, 0x0102) log.info(peek(3), peek16(16)) end",
    );
    runtime.tick(NO_INPUT).unwrap();
    assert_eq!(texts(runtime.take_output()), ["7\t258"]);
}

#[test]
fn bad_arguments_are_rejected_and_write_nothing() {
    let bad = [
        "poke(1.5, 1)",
        "poke(-1, 1)",
        "poke(0/0, 1)",
        "poke(math.huge, 1)",
        "poke(-math.huge, 1)",
        "poke(0, 256)",
        "poke(0, -1)",
        "poke(0, 0.5)",
        "poke('0', 1)",
        "poke(nil, 1)",
        "poke(0)",
        "poke16(0, 65536)",
        "poke32(0, 4294967296)",
        "poke32(0, -1)",
        "peek({})",
        "peek16(0.1)",
        "peek32(true)",
        "memset(0, 256, 1)",
        "memset(0, 1, -1)",
        "memset(0, 1, 1.5)",
        "memset(0, 1, math.huge)",
        "memcpy(0, 0, 0/0)",
        "memcpy(0, '1', 1)",
    ];
    for body in bad {
        let (message, runtime) = tick_error(body);
        assert!(message.contains("bad argument #"), "`{body}`: {message}");
        assert_eq!(ram(&runtime, 0..8), [0; 8], "`{body}` wrote something");
    }
}

#[test]
fn the_error_names_the_function_and_the_argument() {
    let (message, _) = tick_error("poke16(0, 65536)");
    assert!(
        message.contains("bad argument #2 to 'poke16' (value 0-65535 expected, got 65536)"),
        "{message}"
    );
}

#[test]
fn addresses_and_ranges_outside_the_address_space_are_errors() {
    let outside = [
        ("peek(0x100000)", "peek"),
        ("peek16(0xFFFFF)", "peek16"),
        ("peek32(0xFFFFD)", "peek32"),
        ("poke(0x100000, 0)", "poke"),
        ("poke16(0xFFFFF, 0)", "poke16"),
        ("poke32(0xFFFFD, 0)", "poke32"),
        ("memset(0xFFFFF, 1, 2)", "memset"),
        ("memset(0x100000, 1, 0)", "memset"),
        ("memcpy(0xFFFF0, 0, 17)", "memcpy"),
        ("memcpy(0, 0xFFFF0, 17)", "memcpy"),
    ];
    for (body, function) in outside {
        let (message, runtime) = tick_error(body);
        assert!(
            message.contains(&format!("'{function}'")) && message.contains("20-bit address space"),
            "`{body}`: {message}"
        );
        assert_eq!(ram(&runtime, 0..8), [0; 8], "`{body}` wrote something");
    }
}

#[test]
fn a_block_that_does_not_fit_is_rejected_before_any_byte_is_written() {
    let mut runtime = started("function tick() memset(0, 1, 16) memcpy(0, 0xFFFF8, 16) end");
    assert!(runtime.tick(NO_INPUT).is_err());
    assert_eq!(ram(&runtime, 0..16), [1; 16]);
}

#[test]
fn the_last_address_is_valid_and_an_empty_block_changes_nothing() {
    let mut runtime = started(
        "function tick()
            poke(0, 5)
            memset(0, 9, 0)
            memcpy(0, 1, 0)
            memset(0xFFFFF, 0, 0)
            memset(0xFFFFF, 0, 1)
            log.info(peek(0), peek(0xFFFFF))
        end",
    );
    runtime.tick(NO_INPUT).unwrap();
    assert_eq!(texts(runtime.take_output()), ["5\t0"]);
}

#[test]
fn multi_byte_values_are_little_endian_and_cross_regions() {
    let runtime = ticked("poke32(0, 0x04030201)");
    assert_eq!(ram(&runtime, 0..4), [1, 2, 3, 4]);

    // The last byte of Work RAM and the first pixel of the framebuffer.
    let mut runtime = started(
        "function tick()
            poke32(0, 0x04030201)
            poke16(0xFFFF, 0xABCD)
            log.info(peek16(1), peek32(0), peek(0xFFFF), peek(0x10000), peek16(0xFFFF))
        end",
    );
    runtime.tick(NO_INPUT).unwrap();
    let expected = format!("{}\t{}\t{}\t{}\t{}", 0x0302, 0x04030201, 0xCD, 0xAB, 0xABCD);
    assert_eq!(texts(runtime.take_output()), [expected]);
}

#[test]
fn unmapped_addresses_read_zero_and_ignore_writes() {
    // 0x3C002 is a hole in the register page, 0x3D000 is past it.
    let mut runtime = started(
        "function tick()
            poke(0x3C002, 5) poke(0x3D000, 5)
            log.info(peek(0x3C002), peek(0x3D000))
        end",
    );
    runtime.tick(NO_INPUT).unwrap();
    assert_eq!(texts(runtime.take_output()), ["0\t0"]);
}

#[test]
fn devices_apply_their_own_write_masks() {
    // The display register keeps four bits, an OAM entry's sixth byte two,
    // and bit 15 of a tilemap entry is not stored.
    let mut runtime = started(
        "function tick()
            poke(0x3C000, 0xFF) poke(0x3B005, 0xFF) poke(0x34001, 0xFF)
            log.info(peek(0x3C000), peek(0x3B005), peek(0x34001) < 128)
        end",
    );
    runtime.tick(NO_INPUT).unwrap();
    assert_eq!(texts(runtime.take_output()), ["15\t3\ttrue"]);
}

#[test]
fn memcpy_behaves_like_memmove_in_both_directions() {
    let forward = ticked("for i = 0, 7 do poke(i, i + 1) end memcpy(2, 0, 6)");
    assert_eq!(ram(&forward, 0..8), [1, 2, 1, 2, 3, 4, 5, 6]);
    let backward = ticked("for i = 0, 7 do poke(i, i + 1) end memcpy(0, 2, 6)");
    assert_eq!(ram(&backward, 0..8), [3, 4, 5, 6, 7, 8, 7, 8]);
}

#[test]
fn memset_fills_the_block_and_nothing_else() {
    let runtime = ticked("memset(2, 0xAB, 3)");
    assert_eq!(ram(&runtime, 0..6), [0, 0, 0xAB, 0xAB, 0xAB, 0]);
}

#[test]
fn every_callback_and_the_entry_script_can_use_memory() {
    let mut runtime = started(
        "poke(0, 1)
        function boot() poke(1, 2) end
        function tick() poke(2, 3) end
        function draw() poke(3, 4) end",
    );
    assert_eq!(ram(&runtime, 0..2), [1, 2]);
    runtime.tick(NO_INPUT).unwrap();
    assert_eq!(ram(&runtime, 0..4), [1, 2, 3, 4]);
}

#[test]
fn a_failing_start_hands_the_hardware_back_with_what_it_wrote() {
    let mut home = Some(hardware());
    let failure = Runtime::start(
        "main.luau",
        b"poke(0, 7) function boot() poke(1, 8) error('boom') end",
        Files::default(),
        SMALL,
        font(),
        Default::default(),
        &mut home,
    );
    assert!(failure.is_err());
    let home = home.expect("the hardware is back");
    assert_eq!(home.work_ram[..2], [7, 8]);
}

#[test]
fn starting_without_hardware_fails_before_running_the_script() {
    let mut home = None;
    let failure = match Runtime::start(
        "main.luau",
        b"log.info('ran')",
        Files::default(),
        SMALL,
        font(),
        Default::default(),
        &mut home,
    ) {
        Ok(_) => panic!("the start needs hardware"),
        Err(failure) => failure,
    };
    assert_eq!(failure.error.phase, Phase::Entry);
    assert!(failure.output.is_empty());
}

#[test]
fn framebuffer_writes_made_in_tick_are_gone_when_draw_starts() {
    let mut runtime = started(&format!(
        "poke(0x3C001, 3)
        function tick() poke({}, 5) end",
        FRAMEBUFFER.start
    ));
    runtime.tick(NO_INPUT).unwrap();
    assert_eq!(runtime.scratch().framebuffer.pixel(0, 0), Some(3));
}

#[test]
fn a_palette_written_in_draw_is_in_the_hardware_after_a_failing_draw() {
    let mut runtime = started("function draw() poke(0x3A000, 200) error('boom') end");
    assert!(runtime.tick(NO_INPUT).is_err());
    assert_eq!(runtime.scratch().palette.get(0).r, 200);
}

#[test]
fn memcpy_and_memset_are_charged_by_length() {
    let fits = ["memcpy(0, 100, 50)", "memset(0, 1, 150)"];
    for body in fits {
        let mut runtime = started_with_budget(&format!("function tick() {body} end"), SPENT);
        assert_eq!(runtime.tick(NO_INPUT), Ok(()), "{body}");
    }
    let too_much = ["memcpy(0, 100, 100)", "memset(0, 1, 199)"];
    for body in too_much {
        let mut runtime =
            started_with_budget(&format!("function tick() poke(0, 5) {body} end"), SPENT);
        assert_eq!(runtime.tick(NO_INPUT), Err(limit(Phase::Tick)), "{body}");
        assert_eq!(ram(&runtime, 0..1), [5], "{body} wrote after the limit");
    }
}

#[test]
fn every_access_has_a_cost_of_its_own() {
    // A loop iteration and the call to `peek` are one interrupt check each.
    // Twenty iterations fit the budget either way, and forty fit only if the
    // access itself is charged too.
    let mut runtime =
        started_with_budget("function tick() for i = 1, 20 do peek(0) end end", SPENT);
    assert_eq!(runtime.tick(NO_INPUT), Ok(()));
    let mut runtime =
        started_with_budget("function tick() for i = 1, 40 do peek(0) end end", SPENT);
    assert_eq!(runtime.tick(NO_INPUT), Err(limit(Phase::Tick)));
    for call in [
        "peek16(0)",
        "peek32(0)",
        "poke(0, 1)",
        "poke16(0, 1)",
        "poke32(0, 1)",
    ] {
        let source = format!("function tick() for i = 1, 40 do {call} end end");
        let mut runtime = started_with_budget(&source, SPENT);
        assert_eq!(runtime.tick(NO_INPUT), Err(limit(Phase::Tick)), "{call}");
    }
}

#[test]
fn a_rejected_call_costs_nothing() {
    // Charged by length, each of these would spend far more than the budget.
    let mut runtime = started_with_budget(
        "function tick() for i = 1, 5 do pcall(memset, 0xFFFFF, 0, 0xFFFFF) pcall(poke, 0, 256) end end",
        Budget { interrupts: 1_000 },
    );
    assert_eq!(runtime.tick(NO_INPUT), Ok(()));
}

#[test]
fn running_out_of_budget_cannot_be_caught_and_stays_out() {
    let mut runtime = started("function tick() pcall(memset, 0, 0, 0x10000) pcall(poke, 0, 9) end");
    // The script catches the error and ends without another interrupt, and
    // the call still fails. The poke after the limit writes nothing.
    assert_eq!(runtime.tick(NO_INPUT), Err(limit(Phase::Tick)));
    assert_eq!(ram(&runtime, 0..1), [0]);
}

#[test]
fn the_budget_starts_over_with_each_call() {
    let mut runtime = started_with_budget(
        "function boot() memset(0, 0, 150) end
        function tick() memset(0, 0, 150) end
        function draw() memset(0, 0, 150) end",
        SPENT,
    );
    for _ in 0..5 {
        assert_eq!(runtime.tick(NO_INPUT), Ok(()));
    }
}
