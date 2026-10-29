//! What the Code view adds while a debugged cartridge is stopped.
//!
//! The view only shows what the system fetched from the stopped runtime and
//! asks for the rest through [`DebugAction`]s. The scripts cannot be edited
//! meanwhile, but they can still be read, searched and navigated.

use glaux_input::{InputEvent, Point, PointerButton};
use glaux_script::debug::{Frame, Resume, Step, Stop, Variable};

use super::layout::{CodeLayout, DIAG_ROWS, DIAG_ROWS_STOPPED};
use super::symbols::{PanelLayout, WIDTH};
use super::{CodeEditor, CodeRequest};
use crate::system::workspace::Rect;

/// Width of the strip that resizes the panel.
const HANDLE: i32 = 3;

/// Narrowest the panel gets, which is also its width at first.
pub const MIN_WIDTH: i32 = WIDTH;

/// Most rows of the call stack the panel takes from the variables.
const MAX_STACK_ROWS: usize = 5;

/// What the Code view asks of a stopped cartridge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DebugAction {
    Resume(Resume),
    Stop,
    /// Show this frame of the call stack: its place in the script and its
    /// variables.
    SelectFrame(usize),
    /// Open or close the variable at this path, see
    /// [`glaux_script::debug::DebugRequest::Expand`]. The system answers an
    /// opening with [`DebugView::set_children`] when the entries are not
    /// known yet.
    Toggle(Vec<usize>),
}

/// A variable, with its entries once they have been fetched.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Node {
    variable: Variable,
    children: Option<Vec<Node>>,
    open: bool,
}

impl Node {
    fn new(variable: Variable) -> Self {
        Self {
            variable,
            children: None,
            open: false,
        }
    }
}

/// A variable as the panel lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    /// Nesting depth, 0 for the frame's own variables.
    pub depth: usize,
    /// Where the variable is, as [`DebugAction::Toggle`] names it.
    pub path: Vec<usize>,
    pub name: String,
    pub value: String,
    pub expandable: bool,
    pub open: bool,
}

/// What a stopped cartridge shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DebugView {
    frames: Vec<Frame>,
    selected: usize,
    roots: Vec<Node>,
    /// The selected frame has no variables to show.
    unavailable: bool,
    /// Index of the first variable row shown.
    scroll: usize,
}

impl DebugView {
    pub fn new(stop: Stop) -> Self {
        Self {
            roots: stop.variables.into_iter().map(Node::new).collect(),
            unavailable: false,
            frames: stop.frames,
            selected: 0,
            scroll: 0,
        }
    }

    pub fn frames(&self) -> &[Frame] {
        &self.frames
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    #[cfg(test)]
    pub fn unavailable(&self) -> bool {
        self.unavailable
    }

    pub fn scroll(&self) -> usize {
        self.scroll
    }

    /// The script and 1-based line the selected frame is at, if it is one.
    pub fn place(&self) -> Option<(&str, u32)> {
        let frame = self.frames.get(self.selected)?;
        Some((frame.path.as_deref()?, frame.line?))
    }

    /// Makes `frame` the selected one. Its variables are shown once
    /// [`set_variables`](Self::set_variables) or
    /// [`set_unavailable`](Self::set_unavailable) says what they are.
    pub fn select(&mut self, frame: usize) {
        if frame < self.frames.len() {
            self.selected = frame;
            self.roots.clear();
            self.unavailable = false;
            self.scroll = 0;
        }
    }

    pub fn set_variables(&mut self, variables: Vec<Variable>) {
        self.roots = variables.into_iter().map(Node::new).collect();
        self.unavailable = false;
    }

    pub fn set_unavailable(&mut self) {
        self.roots.clear();
        self.unavailable = true;
    }

    /// The variables as rows, with the entries of the open ones below them.
    pub fn rows(&self) -> Vec<Row> {
        fn walk(nodes: &[Node], depth: usize, path: &mut Vec<usize>, out: &mut Vec<Row>) {
            for (index, node) in nodes.iter().enumerate() {
                path.push(index);
                out.push(Row {
                    depth,
                    path: path.clone(),
                    name: node.variable.name.clone(),
                    value: node.variable.value.clone(),
                    expandable: node.variable.expandable,
                    open: node.open,
                });
                if let (true, Some(children)) = (node.open, &node.children) {
                    walk(children, depth + 1, path, out);
                }
                path.pop();
            }
        }
        let mut out = Vec::new();
        walk(&self.roots, 0, &mut Vec::new(), &mut out);
        out
    }

    fn node_mut(&mut self, path: &[usize]) -> Option<&mut Node> {
        let (&first, rest) = path.split_first()?;
        let mut node = self.roots.get_mut(first)?;
        for &index in rest {
            node = node.children.as_mut()?.get_mut(index)?;
        }
        Some(node)
    }

    /// Opens or closes the variable at `path`. Returns whether its entries
    /// are needed first, to be given to [`set_children`](Self::set_children).
    pub fn toggle(&mut self, path: &[usize]) -> bool {
        let Some(node) = self.node_mut(path) else {
            return false;
        };
        if !node.variable.expandable {
            return false;
        }
        if node.children.is_none() {
            return true;
        }
        node.open = !node.open;
        false
    }

    /// Gives the entries of the variable at `path` and opens it.
    pub fn set_children(&mut self, path: &[usize], children: Vec<Variable>) {
        if let Some(node) = self.node_mut(path) {
            node.children = Some(children.into_iter().map(Node::new).collect());
            node.open = true;
        }
    }

    fn scroll_by(&mut self, delta: isize, visible: usize) {
        let most = self.rows().len().saturating_sub(visible);
        self.scroll = self.scroll.saturating_add_signed(delta).min(most);
    }
}

/// Where the panel's parts sit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DebugLayout {
    pub area: Rect,
    /// The strip along the panel's left edge that is dragged to resize it.
    pub handle: Rect,
    pub locals_title: Rect,
    pub first_local: Rect,
    pub visible_locals: usize,
    pub stack_title: Rect,
    pub first_frame: Rect,
    pub visible_frames: usize,
}

impl DebugLayout {
    /// The panel `width` wide at the right edge of `layout`'s text area,
    /// with room for `frames` call stack rows at its foot.
    pub fn new(layout: &CodeLayout, frames: usize, width: i32) -> Self {
        let panel = PanelLayout::with_width(layout, width);
        let cell_h = layout.metrics.cell_h.max(1);
        let visible_frames = frames.clamp(1, MAX_STACK_ROWS);
        let stack_height = (visible_frames as i32 + 1) * cell_h + 3;
        let stack_title = Rect {
            y: panel.area.bottom() - stack_height,
            height: cell_h + 2,
            ..panel.area
        };
        let first_frame = Rect {
            y: stack_title.bottom() + 1,
            height: cell_h,
            ..panel.area
        };
        let visible_locals =
            usize::try_from((stack_title.y - panel.first_row.y) / cell_h).unwrap_or(0);
        Self {
            area: panel.area,
            handle: Rect {
                width: HANDLE,
                ..panel.area
            },
            locals_title: panel.title,
            first_local: panel.first_row,
            visible_locals,
            stack_title,
            first_frame,
            visible_frames,
        }
    }

    pub fn local_row(&self, n: usize) -> Rect {
        Rect {
            y: self.first_local.y + n as i32 * self.first_local.height,
            ..self.first_local
        }
    }

    pub fn frame_row(&self, n: usize) -> Rect {
        Rect {
            y: self.first_frame.y + n as i32 * self.first_frame.height,
            ..self.first_frame
        }
    }

    fn local_at(&self, at: Point) -> Option<usize> {
        (0..self.visible_locals).find(|&n| self.local_row(n).contains(at))
    }

    fn frame_at(&self, at: Point) -> Option<usize> {
        (0..self.visible_frames).find(|&n| self.frame_row(n).contains(at))
    }
}

impl CodeEditor {
    pub fn show_stop(&mut self, stop: Stop) {
        self.symbols = None;
        self.find = None;
        self.completion = None;
        self.signature = None;
        self.close_hover();
        self.debug = Some(DebugView::new(stop));
        self.layout = self.layout.resized_diag(DIAG_ROWS_STOPPED);
    }

    pub fn end_stop(&mut self) {
        self.resizing = false;
        self.debug = None;
        self.layout = self.layout.resized_diag(DIAG_ROWS);
    }

    pub fn debug_view(&self) -> Option<&DebugView> {
        self.debug.as_ref()
    }

    pub fn debug_view_mut(&mut self) -> Option<&mut DebugView> {
        self.debug.as_mut()
    }

    pub fn debug_layout(&self) -> Option<DebugLayout> {
        let view = self.debug.as_ref()?;
        Some(DebugLayout::new(
            &self.layout,
            view.frames.len(),
            self.debug_width,
        ))
    }

    /// Widest the panel gets: half the text area.
    fn max_debug_width(&self) -> i32 {
        (self.layout.text.width / 2).max(MIN_WIDTH)
    }

    /// Presses on the plates and the panel of a stopped cartridge, and the
    /// wheel over the panel. Returns what they ask for, `None` when the
    /// event is not theirs.
    pub(super) fn debug_pointer(&mut self, event: InputEvent) -> Option<Option<CodeRequest>> {
        let layout = self.debug_layout()?;
        match event {
            InputEvent::PointerDown {
                button: PointerButton::Primary,
                at,
                ..
            } if layout.handle.contains(at) => {
                self.resizing = true;
                Some(None)
            }
            InputEvent::PointerMove(at) if self.resizing => {
                self.pointer = at;
                if let Some(at) = at {
                    let right = self.layout.text.right();
                    self.debug_width =
                        (right - i32::from(at.x)).clamp(MIN_WIDTH, self.max_debug_width());
                }
                Some(None)
            }
            InputEvent::PointerUp {
                button: PointerButton::Primary,
                ..
            } if self.resizing => {
                self.resizing = false;
                Some(None)
            }
            InputEvent::PointerCancel => {
                self.resizing = false;
                None
            }
            InputEvent::PointerDown {
                button: PointerButton::Primary,
                at,
                ..
            } => {
                let [go, over, into, out, stop] = self.layout.step_buttons();
                let action = if go.contains(at) {
                    DebugAction::Resume(Resume::Continue)
                } else if over.contains(at) {
                    DebugAction::Resume(Resume::Step(Step::Over))
                } else if into.contains(at) {
                    DebugAction::Resume(Resume::Step(Step::Into))
                } else if out.contains(at) {
                    DebugAction::Resume(Resume::Step(Step::Out))
                } else if stop.contains(at) {
                    DebugAction::Stop
                } else if let Some(n) = layout.frame_at(at) {
                    DebugAction::SelectFrame(n)
                } else if let Some(n) = layout.local_at(at) {
                    let view = self.debug.as_ref()?;
                    let row = view.rows().into_iter().nth(view.scroll + n)?;
                    DebugAction::Toggle(row.path)
                } else if layout.area.contains(at) {
                    return Some(None);
                } else {
                    return None;
                };
                Some(Some(CodeRequest::Debug(action)))
            }
            InputEvent::Wheel { delta }
                if self.pointer.is_some_and(|at| layout.area.contains(at)) =>
            {
                let visible = layout.visible_locals;
                if let Some(view) = self.debug.as_mut() {
                    view.scroll_by(-isize::from(delta), visible);
                }
                Some(None)
            }
            _ => None,
        }
    }
}
