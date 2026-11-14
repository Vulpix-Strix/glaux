//! Shared fixtures for the Map editor tests.

use glaux_input::Point;
use glaux_machine::bg::{BG_PALETTES_BYTES, BgPalettes, BgTileBank, BgVram};
use glaux_machine::font::{BitmapFont, FontMetrics};
use glaux_machine::framebuffer::Framebuffer;
use glaux_machine::obj_palette::ObjPalettes;
use glaux_machine::palette::Palette;
use glaux_machine::sprite::{ObjTileAttributes, ObjTileBank, ObjVram};
use glaux_machine::tilemap::{BgBank, TileEntry};
use glaux_video::preview::ObjPreviewPalette;
use pretty_assertions::assert_eq;

use super::MapEditor;
use super::frame::Hit;
use crate::cartridge::map::{MapFiles, MapId, MapMeta};
use crate::system::map_document::MapDocument;
use crate::system::workspace::RenderContext;
use crate::system::workspace::sprites::SpriteSource;

pub(super) struct Fixture {
    pub(super) obj: ObjVram,
    pub(super) bg: BgVram,
    pub(super) master: Palette,
    pub(super) preview: ObjPreviewPalette,
}

impl Fixture {
    /// Background tile 1 is solid local 3, tile 2 solid local 4 and tile 3
    /// holds local 5 in its top-left pixel only. BGP0 maps locals 3, 4 and 5
    /// to masters 100, 101 and 102, and BGP1 maps local 3 to master 103.
    pub(super) fn new() -> Self {
        let mut tiles = BgTileBank::new();
        for y in 0..8 {
            for x in 0..8 {
                tiles.set_pixel(1, x, y, 3);
                tiles.set_pixel(2, x, y, 4);
            }
        }
        tiles.set_pixel(3, 0, 0, 5);
        let mut palettes = BgPalettes::linear().as_bytes().to_vec();
        palettes[3] = 100;
        palettes[4] = 101;
        palettes[5] = 102;
        palettes[16 + 3] = 103;
        assert_eq!(palettes.len(), BG_PALETTES_BYTES);
        let bg = BgVram::new(tiles, BgPalettes::from_bytes(&palettes).unwrap());
        let master = Palette::default();
        let preview =
            ObjPreviewPalette::for_background(&Palette::default(), 32, &master, bg.palettes())
                .unwrap();
        Self {
            obj: ObjVram::new(
                ObjTileBank::new(),
                ObjTileAttributes::new(),
                ObjPalettes::linear(),
            ),
            bg,
            master,
            preview,
        }
    }

    pub(super) fn source(&self) -> SpriteSource<'_> {
        SpriteSource {
            obj: &self.obj,
            bg: &self.bg,
            master: &self.master,
            preview: &self.preview,
            modified: false,
        }
    }

    pub(super) fn slot(&self, master: u8) -> u8 {
        self.preview.slot(master).unwrap()
    }
}

pub(super) fn editor() -> MapEditor {
    let layout = super::super::tab::Layout::new(FontMetrics::SYSTEM);
    MapEditor::new(layout.map_metrics(), BitmapFont::new(FontMetrics::SYSTEM))
}

pub(super) fn document(width: usize, height: usize) -> MapDocument {
    let meta = MapMeta::new("Town", width, height).unwrap().to_bytes();
    let files = MapFiles {
        meta,
        layers: [None, None, None],
    };
    MapDocument::from_files(MapId::parse("town").unwrap(), &files).unwrap()
}

pub(super) fn entry(tile: u16) -> TileEntry {
    TileEntry::new(tile, false, false, false, BgBank::ZERO).unwrap()
}

pub(super) fn render(editor: &MapEditor, fixture: &Fixture, document: &MapDocument) -> Framebuffer {
    render_with(editor, Ok(fixture.source()), document, None)
}

pub(super) fn render_with(
    editor: &MapEditor,
    source: Result<SpriteSource<'_>, &str>,
    document: &MapDocument,
    pointer: Option<Point>,
) -> Framebuffer {
    let mut fb = Framebuffer::new();
    let rom = crate::rom::load().unwrap();
    let mut ctx = RenderContext::new(&mut fb, rom.font(), rom.ui());
    let frame = editor.frame();
    let hover = pointer.map_or(Hit::Nothing, |at| frame.hit(at, editor.edit.strip_top));
    editor.options_bar(&mut ctx, &frame, Some(document), true, hover);
    editor.render_editor(&mut ctx, source, document, pointer, hover);
    fb
}

pub(super) fn at(x: i32, y: i32) -> Point {
    Point::new(x as u16, y as u16)
}

pub(super) fn middle(rect: crate::system::workspace::Rect) -> Point {
    at(rect.x + rect.width / 2, rect.y + rect.height / 2)
}

pub(super) fn input(
    editor: &mut MapEditor,
    document: &mut MapDocument,
    event: glaux_input::InputEvent,
) -> Option<super::MapOut> {
    editor.handle_input(event, Some(document))
}

pub(super) fn pointer(
    editor: &mut MapEditor,
    document: &mut MapDocument,
    button: glaux_input::PointerButton,
    at: Point,
) -> Option<super::MapOut> {
    input(
        editor,
        document,
        glaux_input::InputEvent::PointerDown {
            button,
            at,
            select: false,
        },
    )
}

pub(super) fn press(editor: &mut MapEditor, document: &mut MapDocument, at: Point) {
    pointer(editor, document, glaux_input::PointerButton::Primary, at);
}

pub(super) fn release(editor: &mut MapEditor, document: &mut MapDocument, at: Point) -> Option<super::MapOut> {
    input(
        editor,
        document,
        glaux_input::InputEvent::PointerUp {
            button: glaux_input::PointerButton::Primary,
            at,
        },
    )
}

pub(super) fn key(editor: &mut MapEditor, document: &mut MapDocument, key: char) {
    input(editor, document, glaux_input::InputEvent::Text(key));
}
