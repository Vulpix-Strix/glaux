//! Drawing the workspace and choosing the pointer's shape.

use super::sprites::SpriteSource;
use super::tab::{Layout, Tab, fit_name, text_width};
use super::{
    BAR, CURSOR_FRAME_TICKS, DOCS_PAD, HOVER_SHADE, IDLE_SHADE, OUTLINE, PLATE_PAD, Workspace,
    docs_source,
};
use crate::cartridge::LoadedCartridge;
use crate::rom::ICON_SIZE;
use crate::system::editing::session::CodeSession;
use crate::system::screen::{ACCENT, BACKGROUND, DIM, PALE, TEXT};
use crate::ui_text;
use glaux_input::Point;
use glaux_machine::font::BitmapFont;
use glaux_machine::framebuffer::{Framebuffer, WIDTH};
use glaux_machine::ink_image::INK_SLOTS;
use glaux_machine::system_rom::{Cursor, SystemCursor, SystemUi};
use glaux_video::draw::{clear, draw_ink_image, draw_text, draw_text_scaled, fill_rect};

/// Draws frame `frame` of `cursor` with its hotspot on `pointer`, if any.
/// Drawn over the finished frame, it covers everything below it.
pub fn draw_cursor(
    framebuffer: &mut Framebuffer,
    cursor: &Cursor,
    frame: usize,
    pointer: Option<Point>,
) {
    let (Some(at), Some(image)) = (pointer, cursor.frame(frame)) else {
        return;
    };
    let (hot_x, hot_y) = cursor.hotspot;
    draw_ink_image(
        framebuffer,
        image,
        i32::from(at.x) - i32::from(hot_x),
        i32::from(at.y) - i32::from(hot_y),
        &[OUTLINE, TEXT, TEXT],
    );
}

impl Workspace {
    /// Draws the tab bar and the selected view for `cartridge`, whose
    /// sprites the editor shows from `sprites` and whose scripts the Code
    /// view shows from `code`, with what is under `pointer` highlighted.
    /// The pointer itself is not drawn. See [`draw_cursor`].
    #[expect(
        clippy::too_many_arguments,
        reason = "each argument is a separate source the frame is drawn from"
    )]
    #[hotpath::measure(label = "workspace::render", impl_type = "Workspace")]
    pub fn render(
        &self,
        framebuffer: &mut Framebuffer,
        font: &BitmapFont,
        ui: &SystemUi,
        cartridge: &LoadedCartridge,
        sprites: Result<SpriteSource<'_>, &str>,
        code: Option<&CodeSession>,
        pointer: Option<Point>,
    ) {
        let layout = self.layout;
        clear(framebuffer, BACKGROUND);
        fill_rect(framebuffer, 0, 0, WIDTH as i32, layout.bar_height(), BAR);

        // Nothing under an open dialog reacts to the pointer, and only the
        // sprite editor's dialog does while one is open.
        let pointer = pointer.filter(|_| self.modal.is_none());
        let chrome_pointer = pointer
            .filter(|_| !self.sprites.has_dialog() && !self.code.has_overlay())
            .filter(|_| !self.docs.has_overlay() || !self.docs_open);
        let hovered = chrome_pointer.and_then(|at| layout.tab_at(at));
        let button = self.docs_button();
        let over_button = chrome_pointer.is_some_and(|at| button.contains(at));
        for (tab, rect) in layout.tabs() {
            let active = tab == self.tab;
            let inks: [u8; INK_SLOTS] = if active {
                // The active plate takes the body's color so it reads as
                // the front of the view below it.
                fill_rect(
                    framebuffer,
                    rect.x,
                    rect.y,
                    rect.width,
                    rect.height,
                    BACKGROUND,
                );
                [OUTLINE, ACCENT, TEXT]
            } else if hovered == Some(tab) {
                [OUTLINE, HOVER_SHADE, PALE]
            } else {
                [OUTLINE, IDLE_SHADE, DIM]
            };
            let icon_y = rect.y + (rect.height - ICON_SIZE as i32) / 2;
            draw_ink_image(
                framebuffer,
                ui.icon(tab.icon()),
                rect.x + PLATE_PAD,
                icon_y,
                &inks,
            );
            if active {
                fill_rect(framebuffer, rect.x, rect.y, rect.width, 1, ACCENT);
            }
        }

        // Icons alone don't say which editor is which: name the one under
        // the pointer, or else the open one.
        let (hint, color) = match hovered {
            Some(tab) => (self.tab_labels.get(tab), PALE),
            None if over_button => (self.tab_labels.docs_hint(), PALE),
            None => (self.tab_labels.get(self.tab), DIM),
        };
        draw_text(
            framebuffer,
            font,
            hint,
            layout.hint_x(),
            layout.text_y(),
            color,
        );

        self.docs_button_face(framebuffer, font, over_button);

        let name = &cartridge.manifest().cartridge.name;
        let cells = layout.name_cells(&self.tab_labels, font);
        if let Some(name) = fit_name(font, name, cells) {
            let x = layout.name_right(&self.tab_labels, font) - text_width(font, &name);
            draw_text(framebuffer, font, &name, x, layout.text_y(), PALE);
        }

        let notice = self
            .notice
            .as_ref()
            .map(|notice| (notice.text.as_str(), notice.color));
        if self.docs_open {
            self.docs
                .render(framebuffer, font, docs_source(), pointer, notice);
        } else {
            match self.tab {
                Tab::Sprites => {
                    self.sprites
                        .render(framebuffer, font, ui, sprites, pointer, notice)
                }
                Tab::Code => self
                    .code
                    .render(framebuffer, font, ui, code, pointer, notice),
                _ => self.render_placeholder(framebuffer, font),
            }
        }
        if let Some(modal) = &self.modal {
            let dialog_layout =
                modal
                    .dialog
                    .layout(font, layout.view_area(), layout.cell_w, layout.cell_h);
            modal.dialog.render(framebuffer, font, &dialog_layout);
        }
    }

    /// The Docs button: a plate like the active tab's while the browser is
    /// open, and lit while the pointer is over it.
    fn docs_button_face(&self, framebuffer: &mut Framebuffer, font: &BitmapFont, hovered: bool) {
        let button = self.docs_button();
        let ink = if self.docs_open {
            fill_rect(
                framebuffer,
                button.x,
                button.y,
                button.width,
                button.height,
                BACKGROUND,
            );
            fill_rect(framebuffer, button.x, button.y, button.width, 1, ACCENT);
            ACCENT
        } else if hovered {
            PALE
        } else {
            DIM
        };
        draw_text(
            framebuffer,
            font,
            self.tab_labels.docs(),
            button.x + DOCS_PAD,
            self.layout.text_y(),
            ink,
        );
    }

    /// The selected view's stand-in until its editor exists.
    fn render_placeholder(&self, framebuffer: &mut Framebuffer, font: &BitmapFont) {
        let Layout { cell_w, cell_h } = self.layout;
        let x = cell_w;
        let mut y = self.layout.bar_height() + cell_h;
        draw_text_scaled(
            framebuffer,
            font,
            self.tab_labels.get(self.tab),
            x,
            y,
            2,
            ACCENT,
        );
        y += 3 * cell_h;
        for (text, color) in [
            (self.tab.purpose(), TEXT),
            (ui_text::get("workspace-not-available-yet"), DIM),
            (String::new(), DIM),
            (ui_text::get("workspace-esc-shell-ctrl-tab-switch-tab"), DIM),
        ] {
            draw_text(framebuffer, font, &text, x, y, color);
            y += cell_h;
        }
    }

    /// The pointer to show at `at`, with the Code view showing `code`, and
    /// the frame of it to draw.
    ///
    /// This is the one place that decides it: an open dialog's buttons
    /// and the tab plates take a pointing hand, and everything else is up
    /// to the view, whose drags keep their pointer even over the tab bar.
    pub fn cursor(&self, at: Point, code: Option<&CodeSession>) -> (SystemCursor, usize) {
        let frame = (self.ticks / CURSOR_FRAME_TICKS) as usize;
        let hand = |clickable: bool| {
            if clickable {
                SystemCursor::Hand
            } else {
                SystemCursor::Arrow
            }
        };
        let cursor = if self.sprites.awaits_file() {
            SystemCursor::Busy
        } else if let Some(modal) = &self.modal {
            let layout = modal.dialog.layout(
                &self.font,
                self.layout.view_area(),
                self.layout.cell_w,
                self.layout.cell_h,
            );
            hand(layout.button_at(at).is_some())
        } else if self.tab == Tab::Code && self.code.is_dragging() {
            self.code.cursor(at, code)
        } else if !self.has_modal()
            && (self.layout.tab_at(at).is_some() || self.docs_button().contains(at))
        {
            SystemCursor::Hand
        } else if self.docs_open {
            self.docs.cursor(at, docs_source())
        } else {
            match self.tab {
                Tab::Sprites => self.sprites.cursor(at),
                Tab::Code => self.code.cursor(at, code),
                _ => SystemCursor::Arrow,
            }
        };
        (cursor, frame)
    }
}
