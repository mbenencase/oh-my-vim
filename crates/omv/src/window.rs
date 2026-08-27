//! The window tree: how the text area is divided between views of buffers.
//!
//! A window is a *view*, not a buffer — it remembers which buffer it shows and
//! where that view is scrolled and parked, so two windows on one file keep their
//! own place. `omv-core` never sees any of this; it is handed the focused
//! window's state and hands back [`Effect`](omv_core::Effect)s, exactly as
//! before splitting existed.

use omv_core::Direction;
use ratatui::layout::{Constraint, Layout as RatLayout, Rect};

/// Cells of separator drawn between two sibling windows.
pub const DIVIDER: u16 = 1;

pub type WindowId = u64;

/// How a split arranges its children.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    /// Side by side, what `:vsp` makes.
    Columns,
    /// Stacked, what `:hsp` makes.
    Rows,
}

#[derive(Debug, Clone)]
pub struct Window {
    pub id: WindowId,
    /// Index into `Editor::buffers`.
    pub buffer: usize,
    /// First visible line of this view.
    pub scroll: usize,
    /// Char index the cursor sits at *in this view*. The focused window's copy
    /// is stale while it has focus — `Buffer::cursor` is the live one — and is
    /// written back the moment focus moves elsewhere.
    pub cursor: usize,
    /// Rect the last frame gave this window. Directional focus is geometric, so
    /// it reads what was actually drawn rather than re-deriving the layout.
    pub area: Rect,
}

impl Window {
    fn new(id: WindowId, buffer: usize) -> Self {
        Window {
            id,
            buffer,
            scroll: 0,
            cursor: 0,
            area: Rect::ZERO,
        }
    }
}

#[derive(Debug)]
enum Node {
    Leaf(Window),
    Split { axis: Axis, children: Vec<Node> },
}

/// The tree, plus which leaf has focus.
#[derive(Debug)]
pub struct Windows {
    root: Node,
    focused: WindowId,
    next_id: WindowId,
}

impl Default for Windows {
    fn default() -> Self {
        Windows::new()
    }
}

impl Windows {
    pub fn new() -> Self {
        Windows {
            root: Node::Leaf(Window::new(0, 0)),
            focused: 0,
            next_id: 1,
        }
    }

    pub fn focused_id(&self) -> WindowId {
        self.focused
    }

    pub fn count(&self) -> usize {
        let mut n = 0;
        walk(&self.root, &mut |_| n += 1);
        n
    }

    /// Every window, in layout order (left to right, top to bottom).
    pub fn iter(&self) -> impl Iterator<Item = &Window> {
        let mut out = Vec::new();
        walk(&self.root, &mut |w| out.push(w));
        out.into_iter()
    }

    pub fn get(&self, id: WindowId) -> Option<&Window> {
        fn find(node: &Node, id: WindowId) -> Option<&Window> {
            match node {
                Node::Leaf(window) if window.id == id => Some(window),
                Node::Leaf(_) => None,
                Node::Split { children, .. } => children.iter().find_map(|c| find(c, id)),
            }
        }
        find(&self.root, id)
    }

    pub fn get_mut(&mut self, id: WindowId) -> Option<&mut Window> {
        fn find(node: &mut Node, id: WindowId) -> Option<&mut Window> {
            match node {
                Node::Leaf(window) if window.id == id => Some(window),
                Node::Leaf(_) => None,
                Node::Split { children, .. } => children.iter_mut().find_map(|c| find(c, id)),
            }
        }
        find(&mut self.root, id)
    }

    pub fn focused(&self) -> &Window {
        self.get(self.focused)
            .expect("the focused window is always in the tree")
    }

    pub fn focused_mut(&mut self) -> &mut Window {
        let id = self.focused;
        self.get_mut(id)
            .expect("the focused window is always in the tree")
    }

    pub fn focus(&mut self, id: WindowId) {
        if self.get(id).is_some() {
            self.focused = id;
        }
    }

    /// Split the focused window along `axis` and focus the new half, which
    /// starts as a copy of the view it was cut from — same buffer, same place.
    pub fn split(&mut self, axis: Axis) -> WindowId {
        let id = self.next_id;
        self.next_id += 1;
        let focused = self.focused;

        split_node(&mut self.root, focused, axis, id);
        // Splitting the same way twice should give three even windows, not a
        // window and a nested pair, so same-axis nesting is flattened away.
        flatten(&mut self.root);
        self.focused = id;
        id
    }

    /// Close a window. Fails (returning `false`) on the last one, the way vim
    /// refuses `:close` rather than quietly quitting.
    pub fn close(&mut self, id: WindowId) -> bool {
        if self.count() <= 1 || self.get(id).is_none() {
            return false;
        }
        // Remember a neighbour before the tree forgets where the window was.
        let successor = self.neighbour(id);
        remove_node(&mut self.root, id);
        collapse(&mut self.root);
        // Collapsing can lift a split into a parent that shares its axis.
        flatten(&mut self.root);
        if self.focused == id {
            self.focused = successor.unwrap_or_else(|| self.first_id());
        }
        true
    }

    /// Drop every window but `id`.
    pub fn only(&mut self, id: WindowId) {
        let Some(window) = self.get(id).cloned() else {
            return;
        };
        self.root = Node::Leaf(window);
        self.focused = id;
    }

    fn first_id(&self) -> WindowId {
        let mut first = None;
        walk(&self.root, &mut |w| {
            if first.is_none() {
                first = Some(w.id)
            }
        });
        first.expect("the tree always holds at least one window")
    }

    /// The window that should take focus when `id` goes away: its nearest
    /// sibling in the same split, or failing that any other window.
    fn neighbour(&self, id: WindowId) -> Option<WindowId> {
        let mut answer = None;
        siblings(&self.root, id, &mut answer);
        answer.or_else(|| self.iter().map(|w| w.id).find(|other| *other != id))
    }

    /// The window `direction` of `from`, judged by where the last frame drew
    /// them: the closest window whose rect starts beyond `from`'s edge and whose
    /// span overlaps it.
    pub fn in_direction(&self, from: WindowId, direction: Direction) -> Option<WindowId> {
        let origin = self.get(from)?.area;
        let overlaps = |a: (u16, u16), b: (u16, u16)| a.0 < b.1 && b.0 < a.1;

        self.iter()
            .filter(|w| w.id != from && w.area.width > 0 && w.area.height > 0)
            .filter_map(|w| {
                let area = w.area;
                let distance = match direction {
                    Direction::Left => {
                        (overlaps((origin.y, origin.bottom()), (area.y, area.bottom()))
                            && area.right() <= origin.x)
                            .then(|| origin.x - area.right())
                    }
                    Direction::Right => {
                        (overlaps((origin.y, origin.bottom()), (area.y, area.bottom()))
                            && area.x >= origin.right())
                        .then(|| area.x - origin.right())
                    }
                    Direction::Up => (overlaps((origin.x, origin.right()), (area.x, area.right()))
                        && area.bottom() <= origin.y)
                        .then(|| origin.y - area.bottom()),
                    Direction::Down => {
                        (overlaps((origin.x, origin.right()), (area.x, area.right()))
                            && area.y >= origin.bottom())
                        .then(|| area.y - origin.bottom())
                    }
                }?;
                Some((distance, w.id))
            })
            .min()
            .map(|(_, id)| id)
    }

    /// Hand every window the rect it owns, and report the separators to paint
    /// between them. Called once per frame, before anything is drawn.
    pub fn layout(&mut self, area: Rect) -> Vec<Divider> {
        let mut dividers = Vec::new();
        assign(&mut self.root, area, &mut dividers);
        dividers
    }
}

/// A one-cell separator between two sibling windows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Divider {
    pub area: Rect,
    /// `Columns` means the windows are side by side, so the rule is vertical.
    pub axis: Axis,
}

fn assign(node: &mut Node, area: Rect, dividers: &mut Vec<Divider>) {
    match node {
        Node::Leaf(window) => window.area = area,
        Node::Split { axis, children } => {
            // n children and n-1 dividers, the children sharing what is left.
            let mut constraints = Vec::with_capacity(children.len() * 2 - 1);
            for i in 0..children.len() {
                if i > 0 {
                    constraints.push(Constraint::Length(DIVIDER));
                }
                constraints.push(Constraint::Fill(1));
            }
            let slots = match axis {
                Axis::Columns => RatLayout::horizontal(constraints).split(area),
                Axis::Rows => RatLayout::vertical(constraints).split(area),
            };
            for (i, child) in children.iter_mut().enumerate() {
                if i > 0 {
                    dividers.push(Divider {
                        area: slots[i * 2 - 1],
                        axis: *axis,
                    });
                }
                assign(child, slots[i * 2], dividers);
            }
        }
    }
}

fn walk<'a>(node: &'a Node, visit: &mut impl FnMut(&'a Window)) {
    match node {
        Node::Leaf(window) => visit(window),
        Node::Split { children, .. } => children.iter().for_each(|c| walk(c, visit)),
    }
}

fn split_node(node: &mut Node, target: WindowId, axis: Axis, new_id: WindowId) -> bool {
    match node {
        Node::Leaf(window) if window.id == target => {
            let mut fresh = window.clone();
            fresh.id = new_id;
            let old = std::mem::replace(
                node,
                Node::Split {
                    axis,
                    children: Vec::with_capacity(2),
                },
            );
            let Node::Split { children, .. } = node else {
                unreachable!("just built a split")
            };
            children.push(old);
            children.push(Node::Leaf(fresh));
            true
        }
        Node::Leaf(_) => false,
        Node::Split { children, .. } => children
            .iter_mut()
            .any(|c| split_node(c, target, axis, new_id)),
    }
}

/// Splice a split's children into it when they share its axis, so `Columns` of
/// [`a`, `Columns` of [`b`, `c`]] becomes `Columns` of [`a`, `b`, `c`].
fn flatten(node: &mut Node) {
    let Node::Split { axis, children } = node else {
        return;
    };
    children.iter_mut().for_each(flatten);
    let axis = *axis;
    let merged = children
        .drain(..)
        .flat_map(|child| match child {
            Node::Split {
                axis: inner,
                children,
            } if inner == axis => children,
            other => vec![other],
        })
        .collect();
    *children = merged;
}

fn remove_node(node: &mut Node, target: WindowId) {
    if let Node::Split { children, .. } = node {
        children.retain(|c| !matches!(c, Node::Leaf(w) if w.id == target));
        children.iter_mut().for_each(|c| remove_node(c, target));
    }
}

/// Replace any split left holding a single child with that child.
fn collapse(node: &mut Node) {
    if let Node::Split { children, .. } = node {
        children.iter_mut().for_each(collapse);
        if children.len() == 1 {
            *node = children.remove(0);
            // The lifted child may itself now share its new parent's axis.
            collapse(node);
        }
    }
}

/// Id of a leaf sharing a split with `target`, preferring the one after it.
fn siblings(node: &Node, target: WindowId, out: &mut Option<WindowId>) {
    if let Node::Split { children, .. } = node {
        if let Some(at) = children
            .iter()
            .position(|c| matches!(c, Node::Leaf(w) if w.id == target))
        {
            let pick = children
                .get(at + 1)
                .or_else(|| at.checked_sub(1).and_then(|i| children.get(i)));
            if let Some(pick) = pick {
                let mut first = None;
                walk(pick, &mut |w| {
                    if first.is_none() {
                        first = Some(w.id)
                    }
                });
                *out = first;
                return;
            }
        }
        children.iter().for_each(|c| siblings(c, target, out));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(windows: &Windows) -> Vec<WindowId> {
        windows.iter().map(|w| w.id).collect()
    }

    #[test]
    fn a_split_copies_the_view_it_was_cut_from() {
        let mut windows = Windows::new();
        windows.focused_mut().buffer = 3;
        windows.focused_mut().scroll = 12;
        windows.focused_mut().cursor = 40;
        let new = windows.split(Axis::Columns);

        assert_eq!(windows.count(), 2);
        assert_eq!(windows.focused_id(), new, "the new window takes focus");
        let fresh = windows.get(new).unwrap();
        assert_eq!(
            (fresh.buffer, fresh.scroll, fresh.cursor),
            (3, 12, 40),
            "a split shows the same place in the same buffer"
        );
    }

    #[test]
    fn splitting_the_same_way_twice_gives_three_even_windows() {
        let mut windows = Windows::new();
        windows.split(Axis::Columns);
        windows.split(Axis::Columns);
        assert_eq!(windows.count(), 3);

        windows.layout(Rect::new(0, 0, 32, 10));
        let widths: Vec<u16> = windows.iter().map(|w| w.area.width).collect();
        assert_eq!(widths, vec![10, 10, 10], "30 cells of text, 2 of divider");
    }

    #[test]
    fn mixed_axes_nest_rather_than_flatten() {
        let mut windows = Windows::new();
        windows.split(Axis::Columns);
        windows.split(Axis::Rows);
        windows.layout(Rect::new(0, 0, 40, 20));

        let areas: Vec<Rect> = windows.iter().map(|w| w.area).collect();
        assert_eq!(areas.len(), 3);
        assert_eq!(areas[0].height, 20, "the left window keeps the full height");
        assert!(
            areas[1].height < 20 && areas[2].height < 20,
            "the right half is the one that got stacked: {areas:?}"
        );
    }

    #[test]
    fn closing_collapses_the_split_it_leaves_behind() {
        let mut windows = Windows::new();
        let second = windows.split(Axis::Columns);
        assert!(windows.close(second));
        assert_eq!(windows.count(), 1);
        assert_eq!(
            windows.focused_id(),
            0,
            "focus falls back to the surviving window"
        );

        assert!(
            !windows.close(0),
            "the last window cannot be closed — there would be nothing to edit"
        );
    }

    #[test]
    fn only_keeps_the_focused_window() {
        let mut windows = Windows::new();
        windows.split(Axis::Columns);
        let keep = windows.split(Axis::Rows);
        windows.only(keep);
        assert_eq!(ids(&windows), vec![keep]);
        assert_eq!(windows.focused_id(), keep);
    }

    #[test]
    fn focus_moves_to_the_window_actually_drawn_that_way() {
        let mut windows = Windows::new();
        let right = windows.split(Axis::Columns);
        let below = windows.split(Axis::Rows); // splits the right-hand window
        windows.layout(Rect::new(0, 0, 40, 20));

        assert_eq!(
            windows.in_direction(0, Direction::Right),
            Some(right),
            "the left window's neighbour is the top-right one"
        );
        assert_eq!(windows.in_direction(right, Direction::Down), Some(below));
        assert_eq!(windows.in_direction(below, Direction::Up), Some(right));
        assert_eq!(windows.in_direction(right, Direction::Left), Some(0));
        assert_eq!(
            windows.in_direction(0, Direction::Up),
            None,
            "there is nothing above the left window"
        );
    }
}
