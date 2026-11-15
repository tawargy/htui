use std::collections::HashSet;

use ratatui::layout::Rect;
use serde::{Deserialize, Serialize};

pub type PaneId = u64;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TermRef {
    pub address: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SplitAxis {
    /// Side-by-side (left | right)
    Horizontal,
    /// Stacked (top / bottom)
    Vertical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavDir {
    Left,
    Right,
    Up,
    Down,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SplitNode {
    Leaf {
        id: PaneId,
        terminal: Option<TermRef>,
    },
    Split {
        axis: SplitAxis,
        ratio: f32,
        first: Box<SplitNode>,
        second: Box<SplitNode>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SplitTree {
    pub root: Option<SplitNode>,
    pub next_id: PaneId,
}

impl Default for SplitTree {
    fn default() -> Self {
        Self::new()
    }
}

impl SplitTree {
    pub fn new() -> Self {
        Self {
            root: None,
            next_id: 1,
        }
    }

    pub fn with_single(terminal: Option<TermRef>) -> (Self, PaneId) {
        let mut tree = Self::new();
        let id = tree.alloc_id();
        tree.root = Some(SplitNode::Leaf { id, terminal });
        (tree, id)
    }

    fn alloc_id(&mut self) -> PaneId {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    pub fn pane_ids(&self) -> Vec<PaneId> {
        let mut out = Vec::new();
        if let Some(root) = &self.root {
            collect_ids(root, &mut out);
        }
        out
    }

    pub fn find_terminal(&self, id: PaneId) -> Option<&TermRef> {
        self.root.as_ref().and_then(|n| find_term_in(n, id))
    }

    pub fn set_terminal(&mut self, id: PaneId, terminal: Option<TermRef>) -> bool {
        self.root
            .as_mut()
            .is_some_and(|n| set_term_in(n, id, terminal))
    }

    pub fn split(
        &mut self,
        target: PaneId,
        axis: SplitAxis,
        new_terminal: Option<TermRef>,
    ) -> Option<PaneId> {
        let new_id = self.alloc_id();
        let root = self.root.as_mut()?;
        if split_node(root, target, axis, new_id, new_terminal) {
            Some(new_id)
        } else {
            None
        }
    }

    pub fn close(&mut self, target: PaneId) -> bool {
        let Some(root) = self.root.take() else {
            return false;
        };
        let (new_root, removed) = close_node(root, target);
        self.root = new_root;
        removed
    }

    pub fn resize_pane(&mut self, target: PaneId, delta: f32) -> bool {
        self.root
            .as_mut()
            .is_some_and(|n| resize_node(n, target, delta))
    }

    pub fn set_ratio_at(&mut self, target: PaneId, ratio: f32) -> bool {
        let clamped = ratio.clamp(0.1, 0.9);
        self.root
            .as_mut()
            .is_some_and(|n| set_ratio_node(n, target, clamped))
    }

    pub fn compute_rects(&self, area: Rect) -> Vec<(PaneId, Rect)> {
        let mut out = Vec::new();
        if let Some(root) = &self.root {
            layout_node(root, area, &mut out);
        }
        out
    }

    pub fn focus_dir(&self, area: Rect, current: PaneId, dir: NavDir) -> Option<PaneId> {
        let rects = self.compute_rects(area);
        let (_, cur) = rects.iter().find(|(id, _)| *id == current)?;
        let cur_cx = cur.x as f32 + cur.width as f32 / 2.0;
        let cur_cy = cur.y as f32 + cur.height as f32 / 2.0;

        let mut best: Option<(PaneId, f32)> = None;
        for (id, r) in &rects {
            if *id == current {
                continue;
            }
            let cx = r.x as f32 + r.width as f32 / 2.0;
            let cy = r.y as f32 + r.height as f32 / 2.0;
            let dx = cx - cur_cx;
            let dy = cy - cur_cy;
            let valid = match dir {
                NavDir::Left => dx < -0.5,
                NavDir::Right => dx > 0.5,
                NavDir::Up => dy < -0.5,
                NavDir::Down => dy > 0.5,
            };
            if !valid {
                continue;
            }
            let dist = dx * dx + dy * dy;
            if best.map_or(true, |(_, b)| dist < b) {
                best = Some((*id, dist));
            }
        }
        best.map(|(id, _)| id)
    }

    pub fn prune_removed(&mut self, present: &HashSet<String>) -> usize {
        let Some(root) = self.root.as_mut() else {
            return 0;
        };
        prune_node(root, present)
    }

    pub fn has_terminals(&self) -> bool {
        self.root.as_ref().is_some_and(node_has_terminals)
    }

    pub fn collapse_empty_splits(&mut self) -> bool {
        if self.has_terminals() {
            return false;
        }
        let ids = self.pane_ids();
        if ids.len() <= 1 {
            return false;
        }
        self.root = Some(SplitNode::Leaf {
            id: ids[0],
            terminal: None,
        });
        true
    }
}

fn node_has_terminals(node: &SplitNode) -> bool {
    match node {
        SplitNode::Leaf { terminal, .. } => terminal.is_some(),
        SplitNode::Split { first, second, .. } => {
            node_has_terminals(first) || node_has_terminals(second)
        }
    }
}

fn collect_ids(node: &SplitNode, out: &mut Vec<PaneId>) {
    match node {
        SplitNode::Leaf { id, .. } => out.push(*id),
        SplitNode::Split { first, second, .. } => {
            collect_ids(first, out);
            collect_ids(second, out);
        }
    }
}

fn find_term_in(node: &SplitNode, target: PaneId) -> Option<&TermRef> {
    match node {
        SplitNode::Leaf { id, terminal } if *id == target => terminal.as_ref(),
        SplitNode::Leaf { .. } => None,
        SplitNode::Split { first, second, .. } => {
            find_term_in(first, target).or_else(|| find_term_in(second, target))
        }
    }
}

fn set_term_in(node: &mut SplitNode, target: PaneId, term: Option<TermRef>) -> bool {
    match node {
        SplitNode::Leaf { id, terminal } if *id == target => {
            *terminal = term;
            true
        }
        SplitNode::Leaf { .. } => false,
        SplitNode::Split { first, second, .. } => {
            set_term_in(first, target, term.clone()) || set_term_in(second, target, term)
        }
    }
}

fn contains_pane(node: &SplitNode, target: PaneId) -> bool {
    match node {
        SplitNode::Leaf { id, .. } => *id == target,
        SplitNode::Split { first, second, .. } => {
            contains_pane(first, target) || contains_pane(second, target)
        }
    }
}

fn split_node(
    node: &mut SplitNode,
    target: PaneId,
    axis: SplitAxis,
    new_id: PaneId,
    new_terminal: Option<TermRef>,
) -> bool {
    match node {
        SplitNode::Leaf { id, terminal } if *id == target => {
            let old = SplitNode::Leaf {
                id: *id,
                terminal: terminal.take(),
            };
            let new_leaf = SplitNode::Leaf {
                id: new_id,
                terminal: new_terminal,
            };
            *node = SplitNode::Split {
                axis,
                ratio: 0.5,
                first: Box::new(old),
                second: Box::new(new_leaf),
            };
            true
        }
        SplitNode::Leaf { .. } => false,
        SplitNode::Split { first, second, .. } => {
            split_node(first, target, axis, new_id, new_terminal.clone())
                || split_node(second, target, axis, new_id, new_terminal)
        }
    }
}

fn close_node(node: SplitNode, target: PaneId) -> (Option<SplitNode>, bool) {
    match node {
        SplitNode::Leaf { id, .. } if id == target => (None, true),
        leaf @ SplitNode::Leaf { .. } => (Some(leaf), false),
        SplitNode::Split {
            axis,
            ratio,
            first,
            second,
        } => {
            if contains_pane(&first, target) {
                let (new_first, removed) = close_node(*first, target);
                return match new_first {
                    Some(f) => (
                        Some(SplitNode::Split {
                            axis,
                            ratio,
                            first: Box::new(f),
                            second,
                        }),
                        removed,
                    ),
                    None => (Some(*second), removed),
                };
            }
            if contains_pane(&second, target) {
                let (new_second, removed) = close_node(*second, target);
                return match new_second {
                    Some(s) => (
                        Some(SplitNode::Split {
                            axis,
                            ratio,
                            first,
                            second: Box::new(s),
                        }),
                        removed,
                    ),
                    None => (Some(*first), removed),
                };
            }
            (
                Some(SplitNode::Split {
                    axis,
                    ratio,
                    first,
                    second,
                }),
                false,
            )
        }
    }
}

fn resize_node(node: &mut SplitNode, target: PaneId, delta: f32) -> bool {
    match node {
        SplitNode::Leaf { .. } => false,
        SplitNode::Split {
            ratio,
            first,
            second,
            ..
        } => {
            if resize_node(first, target, delta) || resize_node(second, target, delta) {
                return true;
            }
            if contains_pane(first, target) {
                *ratio = (*ratio + delta).clamp(0.1, 0.9);
                return true;
            }
            if contains_pane(second, target) {
                *ratio = (*ratio - delta).clamp(0.1, 0.9);
                return true;
            }
            false
        }
    }
}

fn set_ratio_node(node: &mut SplitNode, target: PaneId, new_ratio: f32) -> bool {
    match node {
        SplitNode::Leaf { .. } => false,
        SplitNode::Split {
            ratio,
            first,
            second,
            ..
        } => {
            if set_ratio_node(first, target, new_ratio) || set_ratio_node(second, target, new_ratio)
            {
                return true;
            }
            if contains_pane(first, target) || contains_pane(second, target) {
                *ratio = new_ratio.clamp(0.1, 0.9);
                return true;
            }
            false
        }
    }
}

fn layout_node(node: &SplitNode, area: Rect, out: &mut Vec<(PaneId, Rect)>) {
    match node {
        SplitNode::Leaf { id, .. } => out.push((*id, area)),
        SplitNode::Split {
            axis,
            ratio,
            first,
            second,
        } => {
            let r = ratio.clamp(0.1, 0.9);
            match axis {
                SplitAxis::Horizontal => {
                    let w1 = ((area.width as f32) * r).round() as u16;
                    let w1 = w1.clamp(1, area.width.saturating_sub(1).max(1));
                    let w2 = area.width.saturating_sub(w1);
                    let r1 = Rect::new(area.x, area.y, w1, area.height);
                    let r2 = Rect::new(area.x + w1, area.y, w2, area.height);
                    layout_node(first, r1, out);
                    layout_node(second, r2, out);
                }
                SplitAxis::Vertical => {
                    let h1 = ((area.height as f32) * r).round() as u16;
                    let h1 = h1.clamp(1, area.height.saturating_sub(1).max(1));
                    let h2 = area.height.saturating_sub(h1);
                    let r1 = Rect::new(area.x, area.y, area.width, h1);
                    let r2 = Rect::new(area.x, area.y + h1, area.width, h2);
                    layout_node(first, r1, out);
                    layout_node(second, r2, out);
                }
            }
        }
    }
}

fn prune_node(node: &mut SplitNode, present: &HashSet<String>) -> usize {
    match node {
        SplitNode::Leaf { terminal, .. } => {
            if let Some(tr) = terminal {
                if !present.contains(&tr.address) {
                    *terminal = None;
                    return 1;
                }
            }
            0
        }
        SplitNode::Split { first, second, .. } => {
            prune_node(first, present) + prune_node(second, present)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_close_resize_and_navigate() {
        let (mut tree, p1) = SplitTree::with_single(Some(TermRef {
            address: "t1".into(),
        }));
        let p2 = tree
            .split(
                p1,
                SplitAxis::Horizontal,
                Some(TermRef {
                    address: "t2".into(),
                }),
            )
            .unwrap();
        let p3 = tree
            .split(
                p2,
                SplitAxis::Vertical,
                Some(TermRef {
                    address: "t3".into(),
                }),
            )
            .unwrap();

        assert_eq!(tree.pane_ids(), vec![p1, p2, p3]);

        // Clamp ratio bounds [0.1, 0.9]
        tree.resize_pane(p1, 10.0);
        if let Some(SplitNode::Split { ratio, .. }) = &tree.root {
            assert!((*ratio - 0.9).abs() < 1e-6);
        }
        tree.resize_pane(p1, -10.0);
        if let Some(SplitNode::Split { ratio, .. }) = &tree.root {
            assert!((*ratio - 0.1).abs() < 1e-6);
        }

        // Geometric navigation
        let area = Rect::new(0, 0, 100, 50);
        assert_eq!(tree.focus_dir(area, p1, NavDir::Right), Some(p2));
        assert_eq!(tree.focus_dir(area, p2, NavDir::Down), Some(p3));
        assert_eq!(tree.focus_dir(area, p3, NavDir::Up), Some(p2));

        // Prune removed
        let mut present = HashSet::new();
        present.insert("t1".into());
        present.insert("t3".into());
        assert_eq!(tree.prune_removed(&present), 1);
        assert!(tree.find_terminal(p2).is_none());

        // Close p2 adopts p3
        assert!(tree.close(p2));
        assert_eq!(tree.pane_ids(), vec![p1, p3]);
    }

    #[test]
    fn has_terminals_and_collapse_empty_splits() {
        let empty = SplitTree::new();
        assert!(!empty.has_terminals());

        let (mut single_empty, p1) = SplitTree::with_single(None);
        assert!(!single_empty.has_terminals());
        assert!(!single_empty.collapse_empty_splits());
        assert_eq!(single_empty.pane_ids(), vec![p1]);

        let (mut multi_empty, m1) = SplitTree::with_single(None);
        let m2 = multi_empty.split(m1, SplitAxis::Horizontal, None).unwrap();
        let _m3 = multi_empty.split(m2, SplitAxis::Vertical, None).unwrap();
        assert!(!multi_empty.has_terminals());
        assert!(multi_empty.collapse_empty_splits());
        assert_eq!(multi_empty.pane_ids(), vec![m1]);
        assert!(!multi_empty.has_terminals());

        let (mut with_term, t1) = SplitTree::with_single(Some(TermRef {
            address: "t1".into(),
        }));
        let _t2 = with_term.split(t1, SplitAxis::Horizontal, None).unwrap();
        assert!(with_term.has_terminals());
        assert!(!with_term.collapse_empty_splits());
        assert_eq!(with_term.pane_ids().len(), 2);
    }
}
