use crate::ui_text;
use glaux_i18n::Arg;

/// Explains a rejected sheet without changing the importer's CLI diagnostics.
pub(super) fn fit(error: &crate::sprite_import::FitError) -> String {
    use crate::sprite_import::FitError;
    use glaux_machine::obj_palette::{OBJ_PALETTE_BANKS, OBJ_PALETTE_COLORS};
    use glaux_machine::sprite::{
        OBJ_SHEET_MAX_HEIGHT, OBJ_SHEET_MAX_WIDTH, OBJ_TILE_HEIGHT, OBJ_TILE_WIDTH,
    };
    let bank_colors = OBJ_PALETTE_COLORS - 1;
    let max_sheet_colors = OBJ_PALETTE_BANKS * bank_colors;
    match error {
        FitError::SemiTransparent { x, y, alpha, count } => ui_text::get_with(
            "error-fit-semi-transparent",
            &[
                ("count", Arg::Text(&count.to_string())),
                ("x", Arg::Text(&x.to_string())),
                ("y", Arg::Text(&y.to_string())),
                ("alpha", Arg::Text(&alpha.to_string())),
            ],
        ),
        FitError::InvalidDimensions { width, height } => ui_text::get_with(
            "error-fit-invalid-dimensions",
            &[
                ("width", Arg::Text(&width.to_string())),
                ("height", Arg::Text(&height.to_string())),
            ],
        ),
        FitError::TooLarge { width, height } => ui_text::get_with(
            "error-fit-too-large",
            &[
                ("width", Arg::Text(&width.to_string())),
                ("height", Arg::Text(&height.to_string())),
                (
                    "OBJ-SHEET-MAX-WIDTH",
                    Arg::Text(&OBJ_SHEET_MAX_WIDTH.to_string()),
                ),
                (
                    "OBJ-SHEET-MAX-HEIGHT",
                    Arg::Text(&OBJ_SHEET_MAX_HEIGHT.to_string()),
                ),
            ],
        ),
        FitError::TooManyColors { colors } => ui_text::get_with(
            "error-fit-too-many-colors",
            &[
                ("colors", Arg::Text(&colors.to_string())),
                ("max-sheet-colors", Arg::Text(&max_sheet_colors.to_string())),
                (
                    "OBJ-PALETTE-BANKS",
                    Arg::Text(&OBJ_PALETTE_BANKS.to_string()),
                ),
                ("bank-colors", Arg::Text(&bank_colors.to_string())),
            ],
        ),
        FitError::TileTooColorful {
            tile_x,
            tile_y,
            x,
            y,
            colors,
        } => ui_text::get_with(
            "error-fit-tile-too-colorful",
            &[
                ("tile-x", Arg::Text(&tile_x.to_string())),
                ("tile-y", Arg::Text(&tile_y.to_string())),
                ("x", Arg::Text(&x.to_string())),
                ("y", Arg::Text(&y.to_string())),
                ("end-x", Arg::Text(&(x + OBJ_TILE_WIDTH - 1).to_string())),
                ("end-y", Arg::Text(&(y + OBJ_TILE_HEIGHT - 1).to_string())),
                ("colors", Arg::Text(&colors.to_string())),
                ("bank-colors", Arg::Text(&bank_colors.to_string())),
            ],
        ),
        FitError::GroupTooColorful { name, colors } => ui_text::get_with(
            "error-fit-group-too-colorful",
            &[
                ("name", Arg::Text(name)),
                ("colors", Arg::Text(&colors.to_string())),
                ("bank-colors", Arg::Text(&bank_colors.to_string())),
            ],
        ),
        FitError::TooManyPalettes {
            unit,
            colors,
            bank_sizes,
        } => ui_text::get_with(
            "error-fit-too-many-palettes",
            &[
                (
                    "OBJ-PALETTE-BANKS",
                    Arg::Text(&OBJ_PALETTE_BANKS.to_string()),
                ),
                ("unit", Arg::Text(unit)),
                ("colors", Arg::Text(&colors.to_string())),
                (
                    "bank-sizes",
                    Arg::Text(
                        &(bank_sizes
                            .iter()
                            .map(usize::to_string)
                            .collect::<Vec<_>>()
                            .join(", ")),
                    ),
                ),
            ],
        ),
        FitError::EmptyGroupName => ui_text::get("error-fit-empty-group-name"),
        FitError::DuplicateGroup(name) => {
            ui_text::get_with("error-fit-duplicate-group", &[("name", Arg::Text(name))])
        }
        FitError::EmptyGroup(name) => {
            ui_text::get_with("error-fit-empty-group", &[("name", Arg::Text(name))])
        }
        FitError::GroupOutsideSheet(name) => ui_text::get_with(
            "error-fit-group-outside-sheet",
            &[("name", Arg::Text(name))],
        ),
        FitError::GroupsOverlap(name, other_name) => ui_text::get_with(
            "error-fit-groups-overlap",
            &[
                ("name", Arg::Text(name)),
                ("other-name", Arg::Text(other_name)),
            ],
        ),
        FitError::RoundTrip { x, y } => ui_text::get_with(
            "error-fit-round-trip",
            &[
                ("x", Arg::Text(&x.to_string())),
                ("y", Arg::Text(&y.to_string())),
            ],
        ),
        FitError::Encoding(error) => obj_encoding(error),
    }
}

fn obj_encoding(error: &glaux_machine::sprite::ObjTileEncodingError) -> String {
    use glaux_machine::sprite::ObjTileEncodingError;
    match error {
        ObjTileEncodingError::InvalidDimensions { width, height } => ui_text::get_with(
            "error-fit-invalid-dimensions",
            &[
                ("width", Arg::Text(&width.to_string())),
                ("height", Arg::Text(&height.to_string())),
            ],
        ),
        ObjTileEncodingError::SheetTooWide { width, maximum } => ui_text::get_with(
            "error-obj-encoding-sheet-too-wide",
            &[
                ("width", Arg::Text(&width.to_string())),
                ("maximum", Arg::Text(&maximum.to_string())),
            ],
        ),
        ObjTileEncodingError::SheetTooTall { height, maximum } => ui_text::get_with(
            "error-obj-encoding-sheet-too-tall",
            &[
                ("height", Arg::Text(&height.to_string())),
                ("maximum", Arg::Text(&maximum.to_string())),
            ],
        ),
        ObjTileEncodingError::PixelCount {
            width,
            height,
            expected,
            actual,
        } => ui_text::get_with(
            "error-obj-encoding-pixel-count",
            &[
                ("expected", Arg::Text(&expected.to_string())),
                ("width", Arg::Text(&width.to_string())),
                ("height", Arg::Text(&height.to_string())),
                ("actual", Arg::Text(&actual.to_string())),
            ],
        ),
        ObjTileEncodingError::PaletteIndex {
            index,
            x,
            y,
            tile_x,
            tile_y,
        } => ui_text::get_with(
            "error-obj-encoding-palette-index",
            &[
                ("index", Arg::Text(&index.to_string())),
                ("x", Arg::Text(&x.to_string())),
                ("y", Arg::Text(&y.to_string())),
                ("tile-x", Arg::Text(&tile_x.to_string())),
                ("tile-y", Arg::Text(&tile_y.to_string())),
            ],
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::fit;
    use crate::sprite_import::FitError;
    use pretty_assertions::assert_eq;
    #[test]
    fn tile_error_keeps_its_english_and_coordinates() {
        assert_eq!(
            fit(&FitError::TileTooColorful {
                tile_x: 2,
                tile_y: 1,
                x: 16,
                y: 8,
                colors: 16
            }),
            "tile (2, 1), pixels (16, 8) to (23, 15), uses 16 colors; an 8x8 sprite tile can use at most 15 plus transparency"
        );
    }
}
