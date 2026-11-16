use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::layout::tree::{PaneId, SplitNode, SplitTree, TermRef};

pub type PanelId = u64;

/// A single isolated workspace panel containing its own `SplitTree` and focused pane.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PanelLayout {
    pub id: PanelId,
    pub tree: SplitTree,
    pub focused_pane: Option<PaneId>,
}

impl PanelLayout {
    pub fn new(id: PanelId, terminal: Option<TermRef>) -> (Self, PaneId) {
        let (tree, pane_id) = SplitTree::with_single(terminal);
        (
            Self {
                id,
                tree,
                focused_pane: Some(pane_id),
            },
            pane_id,
        )
    }

    pub fn from_tree(id: PanelId, tree: SplitTree) -> Self {
        let focused_pane = tree.pane_ids().into_iter().next();
        Self {
            id,
            tree,
            focused_pane,
        }
    }

    pub fn has_terminals(&self) -> bool {
        self.tree.has_terminals()
    }

    pub fn terminal_addresses(&self) -> Vec<String> {
        let mut out = Vec::new();
        if let Some(root) = &self.tree.root {
            collect_terminal_addresses(root, &mut out);
        }
        out
    }

    pub fn contains_terminal(&self, address: &str) -> bool {
        self.find_pane_by_terminal(address).is_some()
    }

    pub fn find_pane_by_terminal(&self, address: &str) -> Option<PaneId> {
        self.tree
            .root
            .as_ref()
            .and_then(|root| find_pane_by_terminal_in(root, address))
    }

    pub fn primary_terminal(&self) -> Option<&TermRef> {
        if let Some(focused) = self.focused_pane {
            if let Some(tr) = self.tree.find_terminal(focused) {
                return Some(tr);
            }
        }
        self.tree.root.as_ref().and_then(first_terminal_in)
    }

    pub fn ensure_valid_focus(&mut self) {
        let ids = self.tree.pane_ids();
        if let Some(focused) = self.focused_pane {
            if ids.contains(&focused) {
                return;
            }
        }
        self.focused_pane = ids.into_iter().next();
    }
}

fn collect_terminal_addresses(node: &SplitNode, out: &mut Vec<String>) {
    match node {
        SplitNode::Leaf {
            terminal: Some(tr), ..
        } => out.push(tr.address.clone()),
        SplitNode::Leaf { terminal: None, .. } => {}
        SplitNode::Split { first, second, .. } => {
            collect_terminal_addresses(first, out);
            collect_terminal_addresses(second, out);
        }
    }
}

fn find_pane_by_terminal_in(node: &SplitNode, address: &str) -> Option<PaneId> {
    match node {
        SplitNode::Leaf {
            id,
            terminal: Some(tr),
        } if tr.address == address => Some(*id),
        SplitNode::Leaf { .. } => None,
        SplitNode::Split { first, second, .. } => find_pane_by_terminal_in(first, address)
            .or_else(|| find_pane_by_terminal_in(second, address)),
    }
}

fn first_terminal_in(node: &SplitNode) -> Option<&TermRef> {
    match node {
        SplitNode::Leaf { terminal, .. } => terminal.as_ref(),
        SplitNode::Split { first, second, .. } => {
            first_terminal_in(first).or_else(|| first_terminal_in(second))
        }
    }
}

/// All isolated panels belonging to a single space.
///
/// Note: `panels` intentionally has no `#[serde(default)]` so untagged legacy
/// `SplitTree` deserialization (`{ "root": ..., "next_id": ... }`) remains disjoint.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpaceLayout {
    pub panels: Vec<PanelLayout>,
    pub active_panel: Option<PanelId>,
    pub next_panel_id: PanelId,
}

impl Default for SpaceLayout {
    fn default() -> Self {
        Self::new()
    }
}

impl SpaceLayout {
    pub fn new() -> Self {
        let (panel, _) = PanelLayout::new(1, None);
        Self {
            panels: vec![panel],
            active_panel: Some(1),
            next_panel_id: 2,
        }
    }

    pub fn with_single(terminal: Option<TermRef>) -> (Self, PanelId, PaneId) {
        let (panel, pane_id) = PanelLayout::new(1, terminal);
        (
            Self {
                panels: vec![panel],
                active_panel: Some(1),
                next_panel_id: 2,
            },
            1,
            pane_id,
        )
    }

    pub fn from_tree(tree: SplitTree) -> Self {
        let panel = PanelLayout::from_tree(1, tree);
        Self {
            panels: vec![panel],
            active_panel: Some(1),
            next_panel_id: 2,
        }
    }

    fn alloc_panel_id(&mut self) -> PanelId {
        let max_existing = self.panels.iter().map(|p| p.id).max().unwrap_or(0);
        let id = self.next_panel_id.max(max_existing.saturating_add(1));
        self.next_panel_id = id.saturating_add(1);
        id
    }

    pub fn add_panel(&mut self, terminal: Option<TermRef>) -> (PanelId, PaneId) {
        let panel_id = self.alloc_panel_id();
        let (panel, pane_id) = PanelLayout::new(panel_id, terminal);
        self.panels.push(panel);
        self.active_panel = Some(panel_id);
        (panel_id, pane_id)
    }

    pub fn remove_panel(&mut self, id: PanelId) -> bool {
        let Some(idx) = self.panels.iter().position(|p| p.id == id) else {
            return false;
        };
        self.panels.remove(idx);
        if self.active_panel == Some(id) {
            self.active_panel = if self.panels.is_empty() {
                None
            } else if idx < self.panels.len() {
                Some(self.panels[idx].id)
            } else {
                Some(self.panels[idx - 1].id)
            };
        } else {
            self.ensure_valid_active_panel();
        }
        true
    }

    pub fn set_active_panel(&mut self, id: PanelId) -> bool {
        if self.panels.iter().any(|p| p.id == id) {
            self.active_panel = Some(id);
            true
        } else {
            false
        }
    }

    pub fn active_panel(&self) -> Option<&PanelLayout> {
        self.active_panel
            .and_then(|id| self.panels.iter().find(|p| p.id == id))
            .or_else(|| self.panels.first())
    }

    pub fn active_panel_mut(&mut self) -> Option<&mut PanelLayout> {
        let idx = self
            .active_panel
            .and_then(|id| self.panels.iter().position(|p| p.id == id))
            .or_else(|| (!self.panels.is_empty()).then_some(0))?;
        self.active_panel = Some(self.panels[idx].id);
        self.panels.get_mut(idx)
    }

    pub fn active_panel_mut_or_default(&mut self) -> &mut PanelLayout {
        let idx = self
            .active_panel
            .and_then(|id| self.panels.iter().position(|p| p.id == id))
            .or_else(|| (!self.panels.is_empty()).then_some(0));
        let idx = match idx {
            Some(i) => i,
            None => {
                let panel_id = self.alloc_panel_id();
                let (panel, _) = PanelLayout::new(panel_id, None);
                self.panels.push(panel);
                0
            }
        };
        self.active_panel = Some(self.panels[idx].id);
        &mut self.panels[idx]
    }

    pub fn panel(&self, id: PanelId) -> Option<&PanelLayout> {
        self.panels.iter().find(|p| p.id == id)
    }

    pub fn panel_mut(&mut self, id: PanelId) -> Option<&mut PanelLayout> {
        self.panels.iter_mut().find(|p| p.id == id)
    }

    pub fn next_panel_id(&self) -> Option<PanelId> {
        if self.panels.is_empty() {
            return None;
        }
        let idx = self
            .active_panel
            .and_then(|id| self.panels.iter().position(|p| p.id == id));
        match idx {
            Some(i) => Some(self.panels[(i + 1) % self.panels.len()].id),
            None => self.panels.first().map(|p| p.id),
        }
    }

    pub fn prev_panel_id(&self) -> Option<PanelId> {
        if self.panels.is_empty() {
            return None;
        }
        let idx = self
            .active_panel
            .and_then(|id| self.panels.iter().position(|p| p.id == id));
        match idx {
            Some(i) => Some(self.panels[(i + self.panels.len() - 1) % self.panels.len()].id),
            None => self.panels.last().map(|p| p.id),
        }
    }

    pub fn find_panel_by_terminal(&self, address: &str) -> Option<PanelId> {
        self.panels
            .iter()
            .find(|p| p.contains_terminal(address))
            .map(|p| p.id)
    }

    pub fn find_pane_by_terminal(&self, address: &str) -> Option<(PanelId, PaneId)> {
        self.panels.iter().find_map(|p| {
            p.find_pane_by_terminal(address)
                .map(|pane_id| (p.id, pane_id))
        })
    }

    pub fn has_terminals(&self) -> bool {
        self.panels.iter().any(PanelLayout::has_terminals)
    }

    pub fn terminal_addresses(&self) -> Vec<String> {
        self.panels
            .iter()
            .flat_map(PanelLayout::terminal_addresses)
            .collect()
    }

    pub fn is_single_empty_placeholder(&self) -> bool {
        self.panels.len() == 1
            && !self.panels[0].has_terminals()
            && self.panels[0].tree.pane_ids().len() == 1
    }

    pub fn ensure_valid_active_panel(&mut self) {
        if self
            .active_panel
            .map_or(true, |id| !self.panels.iter().any(|p| p.id == id))
        {
            self.active_panel = self.panels.first().map(|p| p.id);
        }
        for panel in &mut self.panels {
            panel.ensure_valid_focus();
        }
    }

    pub fn prune_removed(&mut self, present: &HashSet<String>) -> usize {
        let mut pruned = 0;
        for panel in &mut self.panels {
            pruned += panel.tree.prune_removed(present);
        }
        if pruned > 0 {
            self.collapse_empty_splits_or_prune();
        }
        pruned
    }

    pub fn prune_terminal(&mut self, address: &str) -> bool {
        let mut changed = false;
        for panel in &mut self.panels {
            let pids = panel.tree.pane_ids();
            for pid in pids {
                if panel
                    .tree
                    .find_terminal(pid)
                    .is_some_and(|t| t.address == address)
                {
                    panel.tree.set_terminal(pid, None);
                    changed = true;
                }
            }
        }
        if changed {
            self.collapse_empty_splits_or_prune();
        }
        changed
    }

    pub fn collapse_empty_splits_or_prune(&mut self) -> bool {
        let mut changed = false;
        if self.panels.len() > 1 {
            let empty_ids: Vec<PanelId> = if self.has_terminals() {
                self.panels
                    .iter()
                    .filter(|p| !p.has_terminals())
                    .map(|p| p.id)
                    .collect()
            } else {
                let keep_id = self
                    .active_panel
                    .filter(|id| self.panels.iter().any(|p| p.id == *id))
                    .or_else(|| self.panels.first().map(|p| p.id));
                self.panels
                    .iter()
                    .filter(|p| Some(p.id) != keep_id)
                    .map(|p| p.id)
                    .collect()
            };
            for id in empty_ids {
                if self.remove_panel(id) {
                    changed = true;
                }
            }
        }
        if self.panels.len() == 1 {
            if let Some(panel) = self.panels.first_mut() {
                if panel.tree.collapse_empty_splits() {
                    panel.ensure_valid_focus();
                    changed = true;
                }
            }
        }
        self.ensure_valid_active_panel();
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::tree::SplitAxis;

    fn term(addr: &str) -> TermRef {
        TermRef {
            address: addr.to_string(),
        }
    }

    #[test]
    fn space_layout_new_and_from_tree_migration() {
        let space = SpaceLayout::new();
        assert_eq!(space.panels.len(), 1);
        assert_eq!(space.active_panel, Some(1));
        assert_eq!(space.next_panel_id, 2);
        assert!(!space.has_terminals());
        assert!(space.is_single_empty_placeholder());
        let p = space.active_panel().unwrap();
        assert_eq!(p.id, 1);
        assert_eq!(p.focused_pane, Some(1));

        let (mut legacy_tree, p1) = SplitTree::with_single(Some(term("t1")));
        let p2 = legacy_tree
            .split(p1, SplitAxis::Horizontal, Some(term("t2")))
            .unwrap();
        let migrated = SpaceLayout::from_tree(legacy_tree.clone());
        assert_eq!(migrated.panels.len(), 1);
        assert_eq!(migrated.active_panel, Some(1));
        assert_eq!(migrated.next_panel_id, 2);
        assert!(migrated.has_terminals());
        assert!(!migrated.is_single_empty_placeholder());
        assert_eq!(migrated.panels[0].tree, legacy_tree);
        assert_eq!(migrated.panels[0].focused_pane, Some(p1));
        assert_eq!(
            migrated.panels[0].terminal_addresses(),
            vec!["t1".to_string(), "t2".to_string()]
        );
        assert_eq!(migrated.find_pane_by_terminal("t2"), Some((1, p2)));
    }

    #[test]
    fn panel_isolation_and_independent_splits() {
        let (mut space, p1_id, p1_pane1) = SpaceLayout::with_single(Some(term("t1")));
        let (p2_id, p2_pane1) = space.add_panel(Some(term("t2")));
        assert_eq!(space.panels.len(), 2);
        assert_eq!(space.active_panel, Some(p2_id));

        // Split inside panel 2 only
        let p2_pane2 = space
            .panel_mut(p2_id)
            .unwrap()
            .tree
            .split(p2_pane1, SplitAxis::Horizontal, Some(term("t3")))
            .unwrap();
        space.panel_mut(p2_id).unwrap().focused_pane = Some(p2_pane2);

        // Panel 1 must remain completely untouched
        let p1 = space.panel(p1_id).unwrap();
        assert_eq!(p1.tree.pane_ids(), vec![p1_pane1]);
        assert_eq!(p1.focused_pane, Some(p1_pane1));
        assert_eq!(p1.terminal_addresses(), vec!["t1".to_string()]);

        // Panel 2 has both panes and its own focus
        let p2 = space.panel(p2_id).unwrap();
        assert_eq!(p2.tree.pane_ids(), vec![p2_pane1, p2_pane2]);
        assert_eq!(p2.focused_pane, Some(p2_pane2));
        assert_eq!(
            p2.terminal_addresses(),
            vec!["t2".to_string(), "t3".to_string()]
        );
        assert_eq!(space.find_panel_by_terminal("t1"), Some(p1_id));
        assert_eq!(space.find_panel_by_terminal("t3"), Some(p2_id));
    }

    #[test]
    fn panel_cycling_and_removal_selects_adjacent_panel() {
        let (mut space, id1, _) = SpaceLayout::with_single(Some(term("t1")));
        let (id2, _) = space.add_panel(Some(term("t2")));
        let (id3, _) = space.add_panel(Some(term("t3")));

        // Active is id3
        assert_eq!(space.active_panel, Some(id3));
        assert_eq!(space.next_panel_id(), Some(id1));
        assert_eq!(space.prev_panel_id(), Some(id2));

        assert!(space.set_active_panel(id2));
        assert_eq!(space.next_panel_id(), Some(id3));
        assert_eq!(space.prev_panel_id(), Some(id1));

        // Remove middle active panel (id2) -> selects right neighbor (id3)
        assert!(space.remove_panel(id2));
        assert_eq!(space.panels.len(), 2);
        assert_eq!(space.active_panel, Some(id3));

        // Remove rightmost active panel (id3) -> selects left neighbor (id1)
        assert!(space.remove_panel(id3));
        assert_eq!(space.panels.len(), 1);
        assert_eq!(space.active_panel, Some(id1));

        // Removing non-existent panel returns false
        assert!(!space.remove_panel(999));
    }

    #[test]
    fn pruning_removes_emptied_panels_or_collapses_last_panel() {
        let (mut space, id1, pane1) = SpaceLayout::with_single(Some(term("t1")));
        let (id2, pane2) = space.add_panel(Some(term("t2")));
        let _pane2b = space
            .panel_mut(id2)
            .unwrap()
            .tree
            .split(pane2, SplitAxis::Vertical, None)
            .unwrap();

        // Prune t2 -> panel 2 becomes empty and is removed because panel 1 remains
        assert!(space.prune_terminal("t2"));
        assert_eq!(space.panels.len(), 1);
        assert_eq!(space.active_panel, Some(id1));
        assert_eq!(space.panels[0].id, id1);
        assert!(space.has_terminals());

        // Split panel 1 with an empty pane, then prune t1 -> sole panel collapses empty splits
        let _pane1b = space
            .panel_mut(id1)
            .unwrap()
            .tree
            .split(pane1, SplitAxis::Horizontal, None)
            .unwrap();
        let present: HashSet<String> = HashSet::new();
        assert_eq!(space.prune_removed(&present), 1);
        assert_eq!(space.panels.len(), 1);
        assert!(!space.has_terminals());
        assert_eq!(space.panels[0].tree.pane_ids(), vec![pane1]);
        assert_eq!(space.panels[0].focused_pane, Some(pane1));
    }

    #[test]
    fn ensure_valid_focus_recovers_after_pane_close() {
        let (mut panel, p1) = PanelLayout::new(1, Some(term("t1")));
        let p2 = panel
            .tree
            .split(p1, SplitAxis::Horizontal, Some(term("t2")))
            .unwrap();
        panel.focused_pane = Some(p2);
        assert!(panel.tree.close(p2));
        panel.ensure_valid_focus();
        assert_eq!(panel.focused_pane, Some(p1));
    }
}
