//! The documentation browser shown apart from the workspace, filling the screen.

use super::docs::{DocTarget, DocsRequest};
use super::{HostWork, Workspace, docs_source};
use crate::system::screen::BACKGROUND;
use glaux_docs::DocId;
use glaux_input::{InputEvent, Point};
use glaux_machine::framebuffer::Framebuffer;
use glaux_machine::system_rom::SystemCursor;
use glaux_video::draw::clear;

/// Where the browser starts when it is opened apart from the workspace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DocsStart {
    Home,
    Page(DocId),
    /// SEARCH DOCS with this query already typed.
    Search(String),
}

/// What the browser shown apart from the workspace wants after an event.
#[derive(Debug, PartialEq, Eq)]
pub enum DocsApart {
    Stay,
    Close,
    /// Work for the system: a link to open or the clipboard.
    Request(HostWork),
}

impl Workspace {
    /// Prepares the browser to fill the screen, outside the workspace, on
    /// the front page or on `start`. The browser keeps its tabs and history
    /// from earlier openings.
    pub fn show_docs_apart(&mut self, start: DocsStart) {
        self.docs.set_top(0);
        let source = docs_source();
        match start {
            DocsStart::Home => self.docs.show(source),
            DocsStart::Page(id) => {
                self.docs.show_target(source, DocTarget::page(id));
            }
            DocsStart::Search(query) => self.docs.show_search_for(source, &query),
        }
    }

    /// Hands `event` to the browser shown apart from the workspace and
    /// carries out what it asks. The workspace's own keys do nothing here.
    pub fn docs_apart_input(&mut self, event: InputEvent) -> DocsApart {
        match self.docs.handle_input(event, docs_source()) {
            Some(DocsRequest::Close) => DocsApart::Close,
            Some(request) => self
                .docs_request(request)
                .map_or(DocsApart::Stay, DocsApart::Request),
            None => DocsApart::Stay,
        }
    }

    /// Draws the browser shown apart from the workspace.
    pub fn render_docs_apart(&self, framebuffer: &mut Framebuffer, pointer: Option<Point>) {
        let notice = self
            .notice
            .as_ref()
            .map(|notice| (notice.text.as_str(), notice.color));
        clear(framebuffer, BACKGROUND);
        self.docs
            .render(framebuffer, &self.font, docs_source(), pointer, notice);
    }

    /// Inserts the clipboard's `text` into the open search of the browser
    /// shown apart from the workspace.
    pub fn docs_apart_paste(&mut self, text: Option<String>) {
        self.docs.paste(docs_source(), text);
    }

    /// The pointer over the browser shown apart from the workspace.
    pub fn docs_apart_cursor(&self, at: Point) -> SystemCursor {
        self.docs.cursor(at, docs_source())
    }
}
