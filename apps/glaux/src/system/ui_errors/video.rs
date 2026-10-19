use crate::ui_text;
use glaux_i18n::Arg;

pub(in crate::system) fn video_asset(error: &crate::system::video::VideoAssetError) -> String {
    use crate::system::video::VideoAssetError;
    match error {
        VideoAssetError::ObjTiles(detail) => ui_text::get_with(
            "error-video-objtiles",
            &[("detail", Arg::Text(&obj_tiles(detail)))],
        ),
        VideoAssetError::ObjAttributes(detail) => ui_text::get_with(
            "error-video-objattributes",
            &[("detail", Arg::Text(&obj_attributes(detail)))],
        ),
        VideoAssetError::ObjPalettes(detail) => ui_text::get_with(
            "error-video-objpalettes",
            &[("detail", Arg::Text(&obj_palettes(detail)))],
        ),
        VideoAssetError::Palette(detail) => ui_text::get_with(
            "error-video-palette",
            &[("detail", Arg::Text(&palette_bytes(detail)))],
        ),
    }
}

pub(in crate::system) fn video_load(error: &crate::system::video::VideoLoadError) -> String {
    use crate::system::video::VideoLoadError;
    match error {
        VideoLoadError::Cartridge(error) => super::cartridge(error),
        VideoLoadError::Asset(error) => video_asset(error),
    }
}

pub(in crate::system) fn obj_tiles(error: &glaux_machine::sprite::ObjTileBankError) -> String {
    use glaux_machine::sprite::ObjTileBankError;
    use glaux_machine::sprite::{OBJ_TILE_BANK_BYTES, OBJ_TILE_BYTES};
    match error {
        ObjTileBankError::Misaligned { length } => ui_text::get_with(
            "error-obj-tiles-misaligned",
            &[
                ("OBJ-TILE-BYTES", Arg::Text(&OBJ_TILE_BYTES.to_string())),
                ("length", Arg::Text(&length.to_string())),
            ],
        ),
        ObjTileBankError::TooLarge { length } => ui_text::get_with(
            "error-obj-tiles-too-large",
            &[
                ("length", Arg::Text(&length.to_string())),
                (
                    "OBJ-TILE-BANK-BYTES",
                    Arg::Text(&OBJ_TILE_BANK_BYTES.to_string()),
                ),
            ],
        ),
    }
}

pub(in crate::system) fn obj_attributes(
    error: &glaux_machine::sprite::ObjTileAttributesError,
) -> String {
    use glaux_machine::sprite::OBJ_TILE_ATTRIBUTES_BYTES;
    use glaux_machine::sprite::ObjTileAttributesError;
    match error {
        ObjTileAttributesError::TooLarge { length } => ui_text::get_with(
            "error-obj-attributes-too-large",
            &[
                ("length", Arg::Text(&length.to_string())),
                (
                    "OBJ-TILE-ATTRIBUTES-BYTES",
                    Arg::Text(&OBJ_TILE_ATTRIBUTES_BYTES.to_string()),
                ),
            ],
        ),
        ObjTileAttributesError::ReservedBits { tile, value } => ui_text::get_with(
            "error-obj-attributes-reserved-bits",
            &[
                ("tile", Arg::Text(&tile.to_string())),
                ("value", Arg::Text(&format!("{:#04x}", value))),
            ],
        ),
    }
}

pub(in crate::system) fn obj_palettes(
    error: &glaux_machine::obj_palette::ObjPalettesError,
) -> String {
    use glaux_machine::obj_palette::OBJ_PALETTES_BYTES;
    use glaux_machine::obj_palette::ObjPalettesError;
    match error {
        ObjPalettesError::Length { length } => ui_text::get_with(
            "error-obj-palettes-length",
            &[
                (
                    "OBJ-PALETTES-BYTES",
                    Arg::Text(&OBJ_PALETTES_BYTES.to_string()),
                ),
                ("length", Arg::Text(&length.to_string())),
            ],
        ),
        ObjPalettesError::ReservedEntry { bank, value } => ui_text::get_with(
            "error-obj-palettes-reserved-entry",
            &[
                ("bank", Arg::Text(&bank.to_string())),
                ("value", Arg::Text(&value.to_string())),
            ],
        ),
    }
}

pub(in crate::system) fn palette_bytes(
    error: &glaux_machine::palette::PaletteBytesError,
) -> String {
    use glaux_machine::palette::PALETTE_RGB888_BYTES;
    let glaux_machine::palette::PaletteBytesError { length } = error;
    ui_text::get_with(
        "error-palette-bytes",
        &[
            (
                "PALETTE-RGB888-BYTES",
                Arg::Text(&PALETTE_RGB888_BYTES.to_string()),
            ),
            ("length", Arg::Text(&length.to_string())),
        ],
    )
}

pub(in crate::system) fn preview_palette(
    error: &glaux_video::preview::ObjPreviewPaletteError,
) -> String {
    let glaux_video::preview::ObjPreviewPaletteError {
        needed,
        available,
        reserved,
    } = error;
    ui_text::get_with(
        "error-preview-palette",
        &[
            ("needed", Arg::Text(&needed.to_string())),
            ("available", Arg::Text(&available.to_string())),
            ("reserved", Arg::Text(&reserved.to_string())),
        ],
    )
}
