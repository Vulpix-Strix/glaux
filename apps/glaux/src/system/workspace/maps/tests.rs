use pretty_assertions::assert_eq;

use super::form::{CreateForm, slug};
use super::*;
use crate::cartridge::map::{MapError, MapMeta};
use crate::system::workspace::{RenderContext, ViewInput};
use glaux_machine::font::FontMetrics;

fn id(text: &str) -> MapId {
    MapId::parse(text).unwrap()
}

fn editor() -> MapEditor {
    let layout = super::super::tab::Layout::new(FontMetrics::SYSTEM);
    MapEditor::new(layout.map_metrics())
}

fn summary(text: &str, name: &str) -> MapSummary {
    MapSummary {
        id: id(text),
        meta: Ok(MapMeta::new(name, 4, 3).unwrap()),
    }
}

fn broken(text: &str) -> MapSummary {
    MapSummary {
        id: id(text),
        meta: Err(MapError::MissingMeta(id(text))),
    }
}

fn document(text: &str) -> MapDocument {
    let meta = MapMeta::new("Open", 2, 1).unwrap().to_bytes();
    let files = crate::cartridge::map::MapFiles {
        meta,
        layers: [None, None, None],
    };
    MapDocument::from_files(id(text), &files).unwrap()
}

#[test]
fn ids_follow_the_name_as_lowercase_slugs() {
    assert_eq!(slug("Overworld"), "overworld");
    assert_eq!(slug("  Cave of  Wonders! "), "cave-of-wonders");
    assert_eq!(slug("--a--b--"), "a-b");
    assert_eq!(slug("ñandú"), "and");
    assert_eq!(slug("¡!"), "");
    assert_eq!(slug(&"x".repeat(50)).len(), 32);
    for name in ["Overworld", "Level 1", "A_b"] {
        assert!(
            slug(name).is_empty() || MapId::parse(&slug(name)).is_ok(),
            "{name}"
        );
    }
}

#[test]
fn a_new_map_form_validates_name_id_and_size_in_that_order() {
    let mut form = CreateForm::new(vec![id("taken")]);
    let area = Rect {
        x: 0,
        y: 12,
        width: 320,
        height: 228,
    };
    let layout = form.layout(area, 6, 12);
    let type_text = |form: &mut CreateForm, text: &str| {
        for ch in text.chars() {
            form.handle_input(InputEvent::Text(ch), &layout);
        }
    };

    assert!(form.validate().is_err(), "the name is blank");
    type_text(&mut form, "Taken");
    let message = form.validate().unwrap_err();
    assert!(message.contains("taken"), "{message}");

    // Tabbing into the id selects it, so End first keeps what is there.
    form.handle_input(InputEvent::Tab { reverse: false }, &layout);
    form.handle_input(InputEvent::End { select: false }, &layout);
    form.handle_input(InputEvent::Text('2'), &layout);
    let (map_id, meta) = form.validate().unwrap();
    assert_eq!(map_id, id("taken2"));
    assert_eq!(
        (meta.name.as_str(), meta.width, meta.height),
        ("Taken", 40, 30)
    );
}

#[test]
fn the_size_fields_take_digits_only_and_stay_in_range() {
    let mut form = CreateForm::new(vec![]);
    let layout = form.layout(editor().metrics.area(), 6, 12);
    for ch in "Map".chars() {
        form.handle_input(InputEvent::Text(ch), &layout);
    }
    form.handle_input(InputEvent::Tab { reverse: false }, &layout);
    form.handle_input(InputEvent::Tab { reverse: false }, &layout);
    // The width starts selected, so typing replaces it. Letters are ignored.
    for ch in "x300".chars() {
        form.handle_input(InputEvent::Text(ch), &layout);
    }
    let message = form.validate().unwrap_err();
    assert_eq!(
        message,
        "map.ron: size 300x30 is outside 1 to 256 in each direction"
    );
}

#[test]
fn the_remembered_map_opens_again_else_the_first_that_can_be_opened() {
    let mut editor = editor();
    assert_eq!(editor.to_open(), None, "nothing is listed yet");
    editor.listed(Ok(vec![broken("a"), summary("b", "B"), summary("c", "C")]));
    assert_eq!(editor.to_open(), Some(id("b")));

    editor.opened(&id("c"));
    assert_eq!(editor.to_open(), Some(id("c")));

    editor.listed(Ok(vec![summary("b", "B")]));
    assert_eq!(editor.to_open(), Some(id("b")), "c is gone");

    editor.listed(Ok(vec![broken("c"), summary("b", "B")]));
    assert_eq!(editor.to_open(), Some(id("b")), "c cannot be opened now");

    editor.deleted(&id("c"));
    editor.forget();
    assert_eq!(editor.to_open(), None);
}

#[test]
fn choosing_a_map_asks_first_when_the_open_one_has_unsaved_edits() {
    let mut editor = editor();
    editor.listed(Ok(vec![summary("a", "A"), summary("b", "B"), broken("c")]));
    let mut open = document("a");

    // The open map itself needs nothing.
    assert_eq!(editor.choose(0, Some(&open)), None);
    assert_eq!(
        editor.choose(1, Some(&open)),
        Some(MapOut::Request(MapRequest::Open(id("b"))))
    );

    open.set_name("Edited");
    assert_eq!(
        editor.choose(1, Some(&open)),
        Some(MapOut::ConfirmSwitch(id("b")))
    );
    // A map that cannot be opened says why and changes nothing.
    editor.list_open = true;
    let out = editor.choose(2, Some(&open));
    assert!(matches!(out, Some(MapOut::Notice(_))), "{out:?}");
    assert!(editor.list_open);
}

#[test]
fn saving_a_clean_map_only_says_there_is_nothing_to_save() {
    let mut editor = editor();
    let mut open = document("a");
    assert_eq!(
        editor.act(Action::Save, Some(&open)),
        Some(MapOut::Notice("Nothing to save.".into()))
    );
    open.set_name("Edited");
    assert_eq!(
        editor.act(Action::Save, Some(&open)),
        Some(MapOut::Request(MapRequest::Save))
    );
    assert_eq!(editor.act(Action::Save, None), None);
    assert_eq!(editor.act(Action::Delete, None), None);
    assert_eq!(
        editor.act(Action::Delete, Some(&open)),
        Some(MapOut::Request(MapRequest::PrepareDelete(id("a"))))
    );
}

#[test]
fn rename_asks_for_a_name_and_ignores_a_blank_one() {
    let mut editor = editor();
    let open = document("a");
    editor.act(Action::Rename, Some(&open));
    assert!(editor.has_overlay());
    assert!(editor.claims_break());

    // The current name starts selected, so typing replaces it.
    for ch in "  ".chars() {
        editor.handle_input(InputEvent::Text(ch), None);
    }
    assert_eq!(editor.handle_input(InputEvent::Enter, None), None);
    assert!(editor.has_overlay(), "a blank name keeps the window open");

    for ch in "New".chars() {
        editor.handle_input(InputEvent::Text(ch), None);
    }
    assert_eq!(
        editor.handle_input(InputEvent::Enter, None),
        Some(MapOut::Request(MapRequest::Rename { name: "New".into() }))
    );
    assert!(!editor.has_overlay());
}

fn inside(inner: Rect, outer: Rect) -> bool {
    inner.x >= outer.x
        && inner.y >= outer.y
        && inner.right() <= outer.right()
        && inner.bottom() <= outer.bottom()
}

#[test]
fn the_controls_fit_the_screen_without_overlapping_at_the_system_font() {
    let editor = editor();
    let layout = editor.layout();
    let screen = Rect {
        x: 0,
        y: 0,
        width: 320,
        height: 240,
    };

    let selector = layout.selector();
    assert!(selector.width >= 20 * 6, "room for a name and an id");
    let buttons: Vec<Rect> = Action::ALL.into_iter().map(|a| layout.button(a)).collect();
    for (i, a) in buttons.iter().enumerate() {
        assert!(inside(*a, layout.header()), "{a:?}");
        assert!(!a.overlaps(selector), "{a:?}");
        for b in &buttons[i + 1..] {
            assert!(!a.overlaps(*b), "{a:?} {b:?}");
        }
    }

    let rows = layout.visible_rows();
    assert!(rows >= 8, "{rows} rows");
    assert!(inside(layout.row(rows - 1), screen));
    assert!(layout.row(0).y >= selector.bottom());

    let create = layout.create_map(text_cells(&ui_text::get("maps-create-map")));
    assert!(inside(create, layout.body()));
}

#[test]
fn the_create_window_fits_the_view_at_the_system_font() {
    let editor = editor();
    let area = editor.metrics.area();
    let form = CreateForm::new(vec![]);
    let layout = form.layout(area, 6, 12);
    assert!(inside(layout.window, area));
    for field in layout.fields {
        assert!(inside(field, layout.window));
        assert!(field.width >= 12 * 6, "room for a name");
    }
    for button in [layout.create, layout.cancel] {
        assert!(inside(button, layout.window));
    }
    assert!(!layout.create.overlaps(layout.cancel));
    assert!(layout.message_y + 12 <= layout.create.y);
}

#[test]
fn the_status_bar_has_its_own_room_below_the_view() {
    let layout = editor().layout();
    let status = layout.status();

    assert!(layout.body().bottom() <= status.y);
    let rows = layout.visible_rows();
    assert!(layout.row(rows - 1).bottom() <= status.y);
}

#[test]
fn cart_rom_shows_in_the_status_bar_without_a_notice() {
    use crate::cartridge::rom::RomUsage;
    use glaux_machine::framebuffer::Framebuffer;

    let editor = editor();
    let rom = crate::rom::load().unwrap();
    let status = editor.layout().status();
    let usage = RomUsage {
        used: 100 * 1024,
        ..RomUsage::default()
    };
    let bar = |usage: Option<&RomUsage>| {
        let mut fb = Framebuffer::new();
        let info = crate::system::workspace::StatusInfo {
            rom: usage,
            ..Default::default()
        };
        editor.render(
            &mut RenderContext::new(&mut fb, rom.font(), rom.ui()),
            None,
            ViewInput {
                info,
                ..ViewInput::default()
            },
            false,
        );
        (status.y..status.bottom())
            .flat_map(|y| (status.x..status.right()).map(move |x| (x, y)))
            .map(|(x, y)| fb.pixel(x, y))
            .collect::<Vec<_>>()
    };

    assert_ne!(bar(Some(&usage)), bar(None));
}
