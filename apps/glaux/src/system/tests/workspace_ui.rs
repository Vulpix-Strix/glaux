use super::*;
use pretty_assertions::assert_eq;

#[test]
fn break_opens_the_workspace_and_returns_to_the_shell() {
    let fixture = Fixture::new("sys-ws-toggle");
    let system = press_break(loaded(&fixture));
    assert!(is_workspace(&system));
    assert_eq!(system.input_mode(), InputMode::Workspace);
    let system = press_break(system);
    assert_eq!(loaded_name(&system), Some("Pong"));
    assert_eq!(system.input_mode(), InputMode::Text);
}

#[test]
fn break_without_a_cartridge_explains_how_to_get_one() {
    let fixture = Fixture::new("sys-ws-empty");
    let system = press_break(shell_with_cartridge(&fixture, ""));
    assert_eq!(loaded_name(&system), None);
    assert_eq!(transcript(&system).last().unwrap(), NO_CARTRIDGE_HINT);
    assert!(NO_CARTRIDGE_HINT.len() <= text_columns(6));
    assert_eq!(system.input_mode(), InputMode::Text);
    // Input after the hint in the same tick still reaches the shell.
    let mut input = vec![InputEvent::Break];
    input.extend("pwd".chars().map(InputEvent::Text));
    input.push(InputEvent::Enter);
    let system = system.update(&input);
    assert_eq!(after(&system, NO_CARTRIDGE_HINT), ["> pwd", "/"]);
}

#[test]
fn a_round_trip_preserves_the_cartridge_and_the_shell() {
    let fixture = Fixture::new("sys-ws-preserve");
    std::fs::create_dir_all(fixture.root().join("games")).unwrap();
    let system = enter(enter(loaded(&fixture), "cd games"), "info");
    let system = system.update(&[InputEvent::Text('h')]);
    let mut expected = shell_of(&system).clone();
    // The shell ticks on the update that opens the workspace, then
    // stays untouched until it is back.
    expected.tick();

    let system = press_break(press_break(system));
    assert_eq!(shell_of(&system), &expected);
    assert_eq!(loaded_name(&system), Some("Pong"));

    // History, the draft line and the directory carry on as before.
    let system = system.update(&[InputEvent::Backspace, InputEvent::Up, InputEvent::Enter]);
    let runs = transcript(&system)
        .iter()
        .filter(|l| *l == "> info")
        .count();
    assert_eq!(runs, 2);
    let system = enter(system, "pwd");
    assert_eq!(transcript(&system).last().unwrap(), "/games");
}

#[test]
fn events_after_a_break_in_the_same_tick_are_dropped() {
    let fixture = Fixture::new("sys-ws-consume");
    let system = loaded(&fixture);
    let before = shell_of(&system).clone();
    let mut input = vec![
        InputEvent::Break,
        InputEvent::Text('x'),
        InputEvent::NextView,
    ];
    input.extend(click(tab_centre(Tab::Music)));
    let system = system.update(&input);
    assert!(is_workspace(&system));
    assert_eq!(workspace_tab(&system), Tab::Code);

    // Leaving drops what follows too: nothing is typed into the shell.
    let system = system.update(&[InputEvent::Break, InputEvent::Text('x'), InputEvent::Enter]);
    assert_eq!(shell_of(&system).transcript(), before.transcript());
    let system = enter(system, "pwd");
    assert_eq!(after(&system, "> load pong")[1..], ["> pwd", "/"]);
}

/// `system`'s frame with the pointer drawn over a copy, as the host
/// presents it.
fn presented(system: &System, pointer: Point) -> Framebuffer {
    let rom = fake_rom();
    let mut fb = Framebuffer::new();
    system.render(&mut fb, &rom, Some(pointer));
    system.draw_cursor(&mut fb, &rom, Some(pointer));
    fb
}

#[test]
fn the_cursor_is_drawn_only_in_the_workspace() {
    let fixture = Fixture::new("sys-ws-pointer");
    let at = Point::new(200, 5);
    let drawn = |system: &System| {
        let mut clean = Framebuffer::new();
        system.render(&mut clean, &fake_rom(), Some(at));
        presented(system, at) != clean
    };
    let mut system = System::new(
        info(),
        sounds(),
        video_defaults(),
        fixture.storage(),
        fake_rom().font(),
        fake_rom().palette().clone(),
    );
    assert!(!drawn(&system), "boot");
    while matches!(system.state(), SystemState::Boot { .. }) {
        system = system.update(&[]);
    }
    write_cartridge(&fixture.root().join("pong"), "Pong");
    let system = enter(system, "load pong");
    assert!(!drawn(&system), "shell");
    let system = press_break(system);
    assert!(drawn(&system), "workspace");
    let system = enter(press_break(system), "run");
    assert!(is_running(&system));
    assert!(!drawn(&system), "running");
    let system = press_break(system);
    assert!(!drawn(&system), "paused");
    let system = open_shell(system);
    assert!(!drawn(&system), "paused shell");
}

#[test]
fn the_cursor_hotspot_sits_on_the_pointer_and_clips_at_the_edges() {
    let fixture = Fixture::new("sys-ws-hotspot");
    let system = press_break(loaded(&fixture));
    // Over the plain bar, where the outline ink stands out.
    let fb = presented(&system, Point::new(200, 5));
    assert_eq!(fb.pixel(200, 5), Some(TEXT), "hotspot");
    assert_eq!(fb.pixel(199, 4), Some(11), "outline up and left");
    assert_eq!(fb.pixel(199, 5), Some(12), "transparent");
    assert_eq!(fb.pixel(201, 6), Some(12), "nothing past the image");

    let fb = presented(&system, Point::new(0, 0));
    assert_eq!(fb.pixel(0, 0), Some(TEXT));
    let (w, h) = (WIDTH as u16, HEIGHT as u16);
    let fb = presented(&system, Point::new(w - 1, h - 1));
    assert_eq!(fb.pixel(i32::from(w) - 1, i32::from(h) - 1), Some(TEXT));
}

#[test]
fn the_cursor_never_enters_the_rendered_frame() {
    let fixture = Fixture::new("sys-ws-trails");
    let system = press_break(loaded(&fixture));
    let rom = fake_rom();
    // Both over the plain bar, where the outline ink stands out.
    let (a, b) = (Point::new(200, 5), Point::new(120, 8));

    let mut machine = Framebuffer::new();
    let mut shown = Framebuffer::new();
    for at in [a, b] {
        system.render(&mut machine, &rom, Some(at));
        shown.as_bytes_mut().copy_from_slice(machine.as_bytes());
        system.draw_cursor(&mut shown, &rom, Some(at));
    }
    let mut clean = Framebuffer::new();
    system.render(&mut clean, &rom, Some(b));
    assert_frame_eq!(machine, clean);

    let changed: Vec<(i32, i32)> = (0..HEIGHT as i32)
        .flat_map(|y| (0..WIDTH as i32).map(move |x| (x, y)))
        .filter(|&(x, y)| shown.pixel(x, y) != clean.pixel(x, y))
        .collect();
    assert_eq!(changed, [(119, 7), (120, 8)]);
}

#[test]
fn hovering_a_tab_highlights_it_and_names_it() {
    let fixture = Fixture::new("sys-ws-hover");
    let system = press_break(loaded(&fixture));
    let metrics = fake_rom().font().metrics();
    let interior = |fb: &Framebuffer, tab| {
        let rect = workspace::tests_support::tab_rect(tab, metrics);
        fb.pixel(rect.x + 6, rect.y + 6)
    };
    let idle = render(&system);
    let hover = presented(&system, tab_centre(Tab::Map));

    assert_eq!(interior(&idle, Tab::Code), Some(TEXT), "active");
    assert_eq!(interior(&idle, Tab::Map), Some(DIM), "idle");
    assert_eq!(interior(&hover, Tab::Map), Some(PALE), "hovered");
    assert_eq!(interior(&hover, Tab::Code), Some(TEXT), "still active");

    // The hint names the open tab ("Code") until a tab is hovered
    // ("Map"), with the fake font's glyphs drawn as solid blocks.
    let (x, y) = workspace::tests_support::hint_origin(metrics);
    let cell = metrics.cell_width() as i32;
    assert_eq!(idle.pixel(x, y), Some(DIM));
    assert_eq!(idle.pixel(x + 3 * cell, y), Some(DIM));
    assert_eq!(hover.pixel(x, y), Some(PALE));
    assert_eq!(hover.pixel(x + 3 * cell, y), Some(12));
}

#[test]
fn clicking_a_tab_selects_it() {
    let fixture = Fixture::new("sys-ws-click");
    let mut system = press_break(loaded(&fixture));
    for tab in [Tab::Music, Tab::Sprites, Tab::Sfx, Tab::Map, Tab::Code] {
        system = system.update(&click(tab_centre(tab)));
        assert_eq!(workspace_tab(&system), tab);
    }
    // Secondary presses and presses below the bar select nothing.
    let at = tab_centre(Tab::Music);
    system = system.update(&[InputEvent::PointerDown {
        button: PointerButton::Secondary,
        at,
    }]);
    system = system.update(&click(Point::new(at.x, 120)));
    assert_eq!(workspace_tab(&system), Tab::Code);
}

#[test]
fn ctrl_tab_cycles_tabs_but_plain_tab_does_not() {
    let fixture = Fixture::new("sys-ws-keys");
    let system = press_break(loaded(&fixture));
    let system = system.update(&[InputEvent::Tab, InputEvent::Tab]);
    assert_eq!(workspace_tab(&system), Tab::Code);
    let system = system.update(&[InputEvent::NextView, InputEvent::NextView]);
    assert_eq!(workspace_tab(&system), Tab::Map);
    let system = system.update(&[InputEvent::PreviousView; 3]);
    assert_eq!(workspace_tab(&system), Tab::Music);
    let system = system.update(&[InputEvent::NextView]);
    assert_eq!(workspace_tab(&system), Tab::Code);
}

#[test]
fn the_selected_tab_survives_trips_to_the_shell() {
    let fixture = Fixture::new("sys-ws-keep-tab");
    let system = press_break(loaded(&fixture)).update(&click(tab_centre(Tab::Sfx)));
    let system = press_break(enter(press_break(system), "info"));
    assert!(is_workspace(&system));
    assert_eq!(workspace_tab(&system), Tab::Sfx);
}

#[test]
fn view_and_pointer_events_do_nothing_in_the_shell() {
    let fixture = Fixture::new("sys-ws-shell-isolation");
    let system = loaded(&fixture);
    let mut expected = shell_of(&system).clone();
    let mut input = vec![
        InputEvent::NextView,
        InputEvent::PointerMove(Some(Point::new(3, 3))),
        InputEvent::Wheel { delta: 2 },
    ];
    input.extend(click(tab_centre(Tab::Map)));
    let system = system.update(&input);
    expected.tick();
    assert_eq!(shell_of(&system), &expected);
    assert_eq!(workspace_tab(&system), Tab::Code);
}

#[test]
fn pointer_events_never_reach_a_cartridge() {
    let fixture = Fixture::new("sys-ws-cartridge-isolation");
    let at = Point::new(10, 10);
    let pointer: &[InputEvent] = &[
        InputEvent::PointerMove(Some(at)),
        InputEvent::PointerDown {
            button: PointerButton::Primary,
            at,
        },
        InputEvent::Wheel { delta: 1 },
        InputEvent::NextView,
        InputEvent::PointerUp {
            button: PointerButton::Primary,
            at,
        },
    ];
    let (system, lines) = logged_per_tick(started(&fixture, UP_STATE), &[pointer, &[]]);
    assert_eq!(lines, ["false   false", "false   false"]);
    assert!(is_running(&system));
    // Nor do they unpause or leave the pause screen.
    let system = press_break(system).update(pointer);
    assert!(is_paused_game(&system));
    assert_eq!(workspace_tab(&system), Tab::Code);
}

#[test]
fn the_workspace_is_silent_and_drawn_in_system_colors() {
    let fixture = Fixture::new("sys-ws-present");
    let rom = fake_rom();
    let system = started_with_palette(&fixture, "", &reversed_rom_palette());
    let mut system = press_break(stop_cartridge(system));
    assert!(is_workspace(&system));
    let palette = system.frame_palette(&rom);
    assert_eq!(palette.entries()[..32], rom.palette().entries()[..32]);
    let mut psg = Psg::new();
    for _ in 0..5 {
        system = system.update(&[]);
        system.audio(&mut psg);
        let mut samples = [0; SAMPLES_PER_TICK];
        psg.render(&mut samples);
        assert!(samples.iter().all(|&s| s == samples[0]));
    }
}

#[test]
fn the_workspace_draws_the_active_tab_and_redraws_from_scratch() {
    let fixture = Fixture::new("sys-ws-render");
    let system = press_break(loaded(&fixture)).update(&click(tab_centre(Tab::Map)));
    let fb = render(&system);
    let rect = workspace::tests_support::tab_rect(Tab::Map, fake_rom().font().metrics());
    assert_eq!(fb.pixel(rect.x + 1, rect.y), Some(ACCENT));
    assert_eq!(
        fb.pixel(rect.x + 1, rect.y + rect.height - 1),
        Some(BACKGROUND)
    );
    let other = workspace::tests_support::tab_rect(Tab::Code, fake_rom().font().metrics());
    assert_ne!(fb.pixel(other.x + 1, other.y), Some(ACCENT));

    let mut dirty = Framebuffer::new();
    dirty.clear(200);
    system.render(&mut dirty, &fake_rom(), None);
    assert_frame_eq!(dirty, fb);
}

#[test]
fn a_long_cartridge_name_never_draws_over_the_tabs() {
    let fixture = Fixture::new("sys-ws-long-name");
    let dir = fixture.root().join("long");
    write_cartridge(&dir, &"W".repeat(120));
    let system = enter(shell_with_cartridge(&fixture, ""), "load long");
    let long = render(&press_break(system));
    let short = render(&press_break(loaded(&fixture)));
    let metrics = fake_rom().font().metrics();
    let tabs = Tab::ALL.map(|tab| workspace::tests_support::tab_rect(tab, metrics));
    let last = tabs[tabs.len() - 1];
    let name_start = workspace::tests_support::name_start(metrics);
    assert!(name_start > last.x + last.width);
    for x in 0..name_start {
        for y in 0..last.height {
            assert_eq!(long.pixel(x, y), short.pixel(x, y), "({x}, {y})");
        }
    }
}
