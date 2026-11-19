use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Direction, Layout, Rect};

use crate::config::Config;
use crate::hc::types::Terminal;
use crate::layout::panel::{PanelId, SpaceLayout};
use crate::layout::store::LayoutDoc;
use crate::layout::tree::{PaneId, SplitAxis, SplitTree, TermRef};
use crate::model::reducer::AppModelState;
use crate::model::tree::{TerminalEntry, UNASSIGNED_SPACE};
use crate::term::keys::encode_key;
use crate::term::mouse::encode_mouse;
use crate::term::screen::MouseProtocolMode;
use crate::term::session::{AttachSession, InputDisposition, SessionState};
use crate::ui::dialogs::{
    ActiveDialog, ConfirmAction, ConfirmDialog, DialogOutcome, HelpDialog, NewSpaceDialog,
    RenameSpaceDialog, SpawnDialog, SpawnTarget,
};
use crate::ui::keymap::{Command, PrefixRouter, RouteResult};
use crate::ui::mouse::{handle_mouse_with_passthrough, MouseAction, MouseDragState};
use crate::ui::panes::{render_panes, PaneLayoutGeometry};
use crate::ui::selection::{extract_selected_text, TextSelection};
use crate::ui::sidebar::{render_sidebar, SidebarItem, SidebarPanel, SidebarState};
use crate::ui::statusbar::render_statusbar;
use crate::ui::tabs::{render_tab_bar, TabHitbox};

pub const OUTER_RESIZE_SETTLE: Duration = Duration::from_millis(120);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppAction {
    None,
    Quit,
    AttachTerminal {
        pane_id: PaneId,
        terminal: String,
    },
    TakeOver(String),
    Spawn {
        command_or_template: String,
        task: String,
        space: String,
        target: SpawnTarget,
    },
    Kill(String),
    Remove(String),
    RenameSpace {
        old_space: String,
        new_space: String,
    },
    SaveLayout(String),
    RemoveLayout(String),
    ResizePane {
        terminal: String,
        cols: u16,
        rows: u16,
    },
    CopyToClipboard(String),
    SendInput {
        terminal: String,
        bytes: Vec<u8>,
    },
}

pub struct TuiApp {
    pub config: Config,
    pub model: AppModelState,
    pub layouts: LayoutDoc,
    pub active_space: String,
    pub focused_pane: Option<PaneId>,
    pub sessions: HashMap<String, AttachSession>,
    pub templates: Vec<String>,
    pub sidebar: SidebarState,
    pub sidebar_visible: bool,
    pub sidebar_focused: bool,
    pub router: PrefixRouter,
    pub dialog: Option<ActiveDialog>,
    pub drag: MouseDragState,
    pub selection: Option<TextSelection>,
    pub last_geom: PaneLayoutGeometry,
    pub last_tabs_geom: Vec<TabHitbox>,
    pub last_sidebar_areas: Option<(Rect, Rect)>,
    pub last_panes_area: Rect,
    pub toast: Option<(String, Instant)>,
    pub should_quit: bool,
    pub connected: bool,
    pub dirty: bool,
    outer_resize_settle_at: Option<Instant>,
}

impl TuiApp {
    pub fn new(config: Config, initial_space: Option<String>) -> Self {
        let prefix_timeout = Duration::from_millis(config.ui.prefix_timeout_ms);
        let active_space = initial_space.unwrap_or_else(|| UNASSIGNED_SPACE.to_string());
        let mut layouts = LayoutDoc::default();
        let (space_layout, _panel_id, p1) = SpaceLayout::with_single(None);
        layouts.spaces.insert(active_space.clone(), space_layout);
        let mut model = AppModelState::default();
        if active_space != UNASSIGNED_SPACE {
            model.declared_spaces.insert(active_space.clone());
        }

        Self {
            config,
            model,
            layouts,
            active_space,
            focused_pane: Some(p1),
            sessions: HashMap::new(),
            templates: Vec::new(),
            sidebar: SidebarState::default(),
            sidebar_visible: true,
            sidebar_focused: false,
            router: PrefixRouter::new(prefix_timeout),
            dialog: None,
            drag: MouseDragState::default(),
            selection: None,
            last_geom: PaneLayoutGeometry::default(),
            last_tabs_geom: Vec::new(),
            last_sidebar_areas: None,
            last_panes_area: Rect::default(),
            toast: None,
            should_quit: false,
            connected: true,
            dirty: true,
            outer_resize_settle_at: None,
        }
    }

    pub fn mark_outer_resize(&mut self, now: Instant) {
        self.outer_resize_settle_at = Some(now + OUTER_RESIZE_SETTLE);
    }

    pub fn clear_outer_resize_settle(&mut self) {
        self.outer_resize_settle_at = None;
    }

    pub fn has_pending_outer_resize(&self) -> bool {
        self.outer_resize_settle_at.is_some()
    }

    pub fn take_settled_outer_resize(&mut self, now: Instant) -> bool {
        if self.outer_resize_settle_at.is_some_and(|at| now >= at) {
            self.outer_resize_settle_at = None;
            true
        } else {
            false
        }
    }

    pub fn current_tree(&self) -> &SplitTree {
        self.layouts
            .spaces
            .get(&self.active_space)
            .and_then(SpaceLayout::active_panel)
            .map(|p| &p.tree)
            .unwrap_or_else(|| {
                static EMPTY: std::sync::OnceLock<SplitTree> = std::sync::OnceLock::new();
                EMPTY.get_or_init(SplitTree::new)
            })
    }

    pub fn current_tree_mut(&mut self) -> &mut SplitTree {
        let space = self.active_space.clone();
        &mut self
            .layouts
            .spaces
            .entry(space)
            .or_default()
            .active_panel_mut_or_default()
            .tree
    }

    pub fn active_panel_id(&self) -> Option<PanelId> {
        self.layouts
            .spaces
            .get(&self.active_space)
            .and_then(|s| s.active_panel().map(|p| p.id))
    }

    fn set_focused_pane(&mut self, pid: Option<PaneId>) {
        self.focused_pane = pid;
        if let Some(space) = self.layouts.spaces.get_mut(&self.active_space) {
            if let Some(panel) = space.active_panel_mut() {
                panel.focused_pane = pid;
            }
        }
    }

    pub fn attach_unattached_in_active_panel(&self, actions: &mut Vec<AppAction>) {
        let tree = self.current_tree();
        for pid in tree.pane_ids() {
            if let Some(tr) = tree.find_terminal(pid) {
                let is_attached = self.sessions.get(&tr.address).is_some_and(|s| {
                    s.state == SessionState::Active || s.state == SessionState::Observing
                });
                if !is_attached {
                    actions.push(AppAction::AttachTerminal {
                        pane_id: pid,
                        terminal: tr.address.clone(),
                    });
                }
            }
        }
    }

    pub fn switch_panel(&mut self, panel_id: PanelId) -> Vec<AppAction> {
        let mut actions = Vec::new();
        let cur_focus = self.focused_pane;
        let changed = {
            let Some(space) = self.layouts.spaces.get_mut(&self.active_space) else {
                return actions;
            };
            let prev_id = space.active_panel().map(|p| p.id);
            if let Some(prev_panel) = space.active_panel_mut() {
                if let Some(fp) = cur_focus {
                    if prev_panel.tree.pane_ids().contains(&fp) {
                        prev_panel.focused_pane = Some(fp);
                    }
                }
                prev_panel.ensure_valid_focus();
            }
            if !space.set_active_panel(panel_id) {
                return actions;
            }
            if let Some(next_panel) = space.active_panel_mut() {
                next_panel.ensure_valid_focus();
                self.focused_pane = next_panel.focused_pane;
            }
            prev_id != Some(panel_id)
        };
        if changed {
            self.selection = None;
        }
        self.drag = MouseDragState::default();
        self.sidebar_focused = false;
        self.attach_unattached_in_active_panel(&mut actions);
        self.dirty = true;
        actions
    }

    pub fn next_panel(&mut self) -> Vec<AppAction> {
        let Some(next_id) = self
            .layouts
            .spaces
            .get(&self.active_space)
            .and_then(SpaceLayout::next_panel_id)
        else {
            return Vec::new();
        };
        self.switch_panel(next_id)
    }

    pub fn prev_panel(&mut self) -> Vec<AppAction> {
        let Some(prev_id) = self
            .layouts
            .spaces
            .get(&self.active_space)
            .and_then(SpaceLayout::prev_panel_id)
        else {
            return Vec::new();
        };
        self.switch_panel(prev_id)
    }

    fn open_spawn_for_target(&mut self, target: SpawnTarget) {
        let sp = self.active_space.clone();
        let tpls = self.templates.clone();
        self.dialog = Some(ActiveDialog::Spawn(SpawnDialog::with_target(
            sp, tpls, target,
        )));
    }

    fn open_split_spawn_dialog(&mut self, axis: SplitAxis) {
        let space_name = self.active_space.clone();
        let entry = self.layouts.spaces.entry(space_name).or_default();
        let panel = entry.active_panel_mut_or_default();
        panel.ensure_valid_focus();
        let panel_id = panel.id;
        let pane_id = self
            .focused_pane
            .filter(|pid| panel.tree.pane_ids().contains(pid))
            .or(panel.focused_pane);
        if let Some(pane_id) = pane_id {
            self.set_focused_pane(Some(pane_id));
            self.open_spawn_for_target(SpawnTarget::SplitPane {
                panel_id,
                pane_id,
                axis,
            });
        } else {
            self.open_spawn_for_target(SpawnTarget::NewPanel);
        }
    }

    fn open_terminal_in_isolated_panel(&mut self, address: String, actions: &mut Vec<AppAction>) {
        let space_name = self.active_space.clone();
        let existing_panel = self
            .layouts
            .spaces
            .get(&space_name)
            .and_then(|s| s.find_pane_by_terminal(&address));
        if let Some((panel_id, pane_id)) = existing_panel {
            actions.extend(self.switch_panel(panel_id));
            self.set_focused_pane(Some(pane_id));
            return;
        }
        let space_layout = self.layouts.spaces.entry(space_name.clone()).or_default();
        let (_, pane_id) = space_layout.place_in_new_or_empty_panel(address.clone(), false);
        self.focused_pane = Some(pane_id);
        self.selection = None;
        self.sidebar_focused = false;
        actions.push(AppAction::AttachTerminal {
            pane_id,
            terminal: address,
        });
        actions.push(AppAction::SaveLayout(space_name));
    }

    fn open_sidebar_terminal(
        &mut self,
        address: String,
        space: String,
        actions: &mut Vec<AppAction>,
    ) {
        if self.active_space != space {
            if let Some(removed) = self.switch_space(space) {
                actions.push(AppAction::RemoveLayout(removed));
            }
        }
        self.open_terminal_in_isolated_panel(address, actions);
        self.refresh_sidebar();
        self.sidebar_focused = false;
    }

    pub fn set_toast(&mut self, msg: impl Into<String>, now: Instant) {
        self.toast = Some((msg.into(), now));
        self.dirty = true;
    }

    pub fn active_toast(&mut self, now: Instant) -> Option<&str> {
        if let Some((_, ts)) = &self.toast {
            if now.duration_since(*ts) > Duration::from_secs(5) {
                self.toast = None;
            }
        }
        self.toast.as_ref().map(|(s, _)| s.as_str())
    }

    pub fn refresh_sidebar(&mut self) {
        self.prune_empty_spaces();
        for sp in self.layouts.spaces.keys() {
            if sp != UNASSIGNED_SPACE {
                self.model.declared_spaces.insert(sp.clone());
            }
        }
        let tree = self
            .model
            .build_tree(self.config.ui.show_finished, self.config.ui.show_unassigned);
        self.sidebar.rebuild(&tree, Some(&self.active_space));
        self.dirty = true;
    }

    fn select_sidebar_space(&mut self, space: &str) {
        if let Some(idx) = self
            .sidebar
            .visible_items
            .iter()
            .position(|item| matches!(item, SidebarItem::Space { name, .. } if name == space))
        {
            self.sidebar.active_panel = SidebarPanel::Spaces;
            self.sidebar.selected = idx;
        }
    }

    fn apply_dialog_outcome(
        &mut self,
        outcome: DialogOutcome,
        now: Instant,
        actions: &mut Vec<AppAction>,
    ) {
        match outcome {
            DialogOutcome::None => {}
            DialogOutcome::Close => {
                self.dialog = None;
            }
            DialogOutcome::SubmitSpawn {
                command_or_template,
                task,
                space,
                target,
            } => {
                self.dialog = None;
                actions.push(AppAction::Spawn {
                    command_or_template,
                    task,
                    space,
                    target,
                });
            }
            DialogOutcome::SubmitConfirm(ConfirmAction::KillTerminal(addr)) => {
                self.dialog = None;
                actions.push(AppAction::Kill(addr));
            }
            DialogOutcome::SubmitConfirm(ConfirmAction::KillBeforeRemove(addr)) => {
                self.dialog = None;
                actions.push(AppAction::Kill(addr.clone()));
                actions.push(AppAction::Remove(addr));
            }
            DialogOutcome::SubmitConfirm(ConfirmAction::RemoveTerminal(addr)) => {
                self.dialog = None;
                actions.push(AppAction::Remove(addr));
            }
            DialogOutcome::SubmitNewSpace(name) => {
                self.dialog = None;
                let trimmed = name.trim().to_string();
                if trimmed.is_empty() || trimmed == UNASSIGNED_SPACE {
                    self.set_toast("Invalid space name", now);
                } else {
                    self.sidebar_focused = false;
                    self.model.declared_spaces.insert(trimmed.clone());
                    if let Some(removed) = self.switch_space_with_actions(trimmed.clone(), actions)
                    {
                        actions.push(AppAction::RemoveLayout(removed));
                    }
                    self.refresh_sidebar();
                    self.select_sidebar_space(&trimmed);
                    actions.push(AppAction::SaveLayout(trimmed));
                }
            }
            DialogOutcome::SubmitRename {
                old_space,
                new_space,
            } => {
                self.dialog = None;
                actions.push(AppAction::RenameSpace {
                    old_space,
                    new_space,
                });
            }
        }
    }

    fn compute_layout_areas(&self, area: Rect) -> (Option<Rect>, Rect, Rect, Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(3), Constraint::Length(1)])
            .split(area);

        let main_area = chunks[0];
        let status_area = chunks[1];

        let (sidebar_area, right_area) =
            if self.sidebar_visible && main_area.width > self.config.ui.sidebar_width + 10 {
                let cols = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([
                        Constraint::Length(self.config.ui.sidebar_width),
                        Constraint::Min(10),
                    ])
                    .split(main_area);
                (Some(cols[0]), cols[1])
            } else {
                (None, main_area)
            };

        let right_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(3)])
            .split(right_area);
        (sidebar_area, right_chunks[0], right_chunks[1], status_area)
    }

    pub fn compute_panes_area(&self, area: Rect) -> Rect {
        self.compute_layout_areas(area).2
    }

    pub fn render(&mut self, area: Rect, buf: &mut Buffer, now: Instant) {
        let (sidebar_area, tab_area, panes_area, status_area) = self.compute_layout_areas(area);
        self.last_sidebar_areas = sidebar_area
            .map(|sa| render_sidebar(sa, buf, &self.sidebar, self.sidebar_focused));

        let space_layout = self.layouts.spaces.get(&self.active_space);
        self.last_tabs_geom = render_tab_bar(
            tab_area,
            buf,
            space_layout,
            &self.active_space,
            &self.model.terminals,
        );

        self.last_panes_area = panes_area;
        let tree = self.current_tree().clone();
        self.last_geom = render_panes(
            panes_area,
            buf,
            &tree,
            self.focused_pane,
            &self.sessions,
            self.selection.as_ref(),
        );

        let core_name = if self.model.local_core.name.is_empty() {
            "local".to_string()
        } else {
            self.model.local_core.name.clone()
        };
        let prefix_armed = self.router.is_armed(now);
        let toast = self.active_toast(now).map(String::from);
        render_statusbar(
            status_area,
            buf,
            &core_name,
            &self.active_space,
            prefix_armed,
            toast.as_deref(),
        );

        if let Some(dialog) = &self.dialog {
            dialog.render(area, buf);
        }
        self.dirty = false;
    }

    pub fn handle_key(&mut self, key: KeyEvent, now: Instant) -> Vec<AppAction> {
        let mut actions = Vec::new();
        let modal_active = self.dialog.is_some();
        match self.router.route(key, modal_active, now) {
            RouteResult::ModalKey(k) => {
                if let Some(outcome) = self.dialog.as_mut().map(|dlg| dlg.handle_key(k)) {
                    self.apply_dialog_outcome(outcome, now, &mut actions);
                }
                self.dirty = true;
            }
            RouteResult::Command(cmd) => {
                self.apply_command(cmd, now, &mut actions);
                self.dirty = true;
            }
            RouteResult::PrefixArmed | RouteResult::PrefixCancelled => {
                self.dirty = true;
            }
            RouteResult::ForwardToPane(k) => {
                let empty_pane = self.focused_terminal_addr().is_none();
                if self.sidebar_focused
                    || (empty_pane && matches!(k.code, KeyCode::Up | KeyCode::Down))
                {
                    match k.code {
                        KeyCode::Up => {
                            self.sidebar.move_up();
                            self.sidebar_focused = true;
                        }
                        KeyCode::Char('k') if self.sidebar_focused => {
                            self.sidebar.move_up();
                        }
                        KeyCode::Down => {
                            self.sidebar.move_down();
                            self.sidebar_focused = true;
                        }
                        KeyCode::Char('j') if self.sidebar_focused => {
                            self.sidebar.move_down();
                        }
                        KeyCode::Left if self.sidebar_focused => {
                            self.sidebar.collapse_selected();
                            self.refresh_sidebar();
                        }
                        KeyCode::Right if self.sidebar_focused => {
                            self.sidebar.expand_selected();
                            self.refresh_sidebar();
                        }
                        KeyCode::Esc if self.sidebar_focused => {
                            self.sidebar_focused = false;
                        }
                        KeyCode::Tab if self.sidebar_focused => {
                            self.sidebar.active_panel = match self.sidebar.active_panel {
                                SidebarPanel::Spaces => SidebarPanel::Agents,
                                SidebarPanel::Agents => SidebarPanel::Spaces,
                            };
                        }
                        KeyCode::Enter if self.sidebar_focused => {
                            if let Some(item) = self.sidebar.selected_item().cloned() {
                                match item {
                                    SidebarItem::Space { name, .. } => {
                                        if let Some(removed) = self
                                            .switch_space_with_actions(name.clone(), &mut actions)
                                        {
                                            actions.push(AppAction::RemoveLayout(removed));
                                        }
                                        self.refresh_sidebar();
                                        self.select_sidebar_space(&name);
                                    }
                                    SidebarItem::Terminal { address, space, .. } => {
                                        self.open_sidebar_terminal(address, space, &mut actions);
                                    }
                                    SidebarItem::Machine { .. } => {
                                        self.sidebar.toggle_selected();
                                        self.refresh_sidebar();
                                    }
                                    SidebarItem::NewSpaceAction => {
                                        self.dialog =
                                            Some(ActiveDialog::NewSpace(NewSpaceDialog::default()));
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                    self.dirty = true;
                } else if self.selection.is_some()
                    && matches!(k.code, KeyCode::Char('C') | KeyCode::Char('c'))
                    && k.modifiers.contains(KeyModifiers::CONTROL)
                    && k.modifiers.contains(KeyModifiers::SHIFT)
                {
                    if let Some(text) = self.extract_selection_text() {
                        if !text.is_empty() {
                            actions.push(AppAction::CopyToClipboard(text));
                        }
                    }
                    self.dirty = true;
                } else if let Some(pid) = self.focused_pane {
                    if self.selection.take().is_some() {
                        self.dirty = true;
                    }
                    if let Some(addr) = self
                        .current_tree()
                        .find_terminal(pid)
                        .map(|t| t.address.clone())
                    {
                        if let Some(sess) = self.sessions.get_mut(&addr) {
                            let modes = sess.screen.modes();
                            if let Some(bytes) = encode_key(k, modes) {
                                if let Ok(InputDisposition::SuppressedObserving) =
                                    sess.send_input(&bytes)
                                {
                                    self.set_toast(
                                        "Observing mode: press Ctrl+B t or click [Take over]",
                                        now,
                                    );
                                }
                            }
                        }
                    }
                }
            }
            RouteResult::Ignored => {}
        }
        actions
    }

    pub fn handle_mouse_event(&mut self, mouse: MouseEvent, now: Instant) -> Vec<AppAction> {
        let mut actions = Vec::new();
        if self.dialog.is_some() {
            return actions;
        }
        if mouse.kind == MouseEventKind::Down(MouseButton::Left)
            && self.drag.active_divider.is_none()
            && self.drag.active_selection.is_none()
            && self.drag.active_passthrough_pane.is_none()
        {
            let hit = self.last_tabs_geom.iter().find(|h| {
                let r = match h {
                    TabHitbox::Panel { rect, .. }
                    | TabHitbox::UnattachedTerminal { rect, .. }
                    | TabHitbox::SplitHorizontal(rect)
                    | TabHitbox::SplitVertical(rect)
                    | TabHitbox::NewTerminal(rect) => rect,
                };
                mouse.column >= r.x
                    && mouse.column < r.x + r.width
                    && mouse.row >= r.y
                    && mouse.row < r.y + r.height
            });
            if let Some(hit) = hit.cloned() {
                self.sidebar_focused = false;
                match hit {
                    TabHitbox::NewTerminal(_) => {
                        self.open_spawn_for_target(SpawnTarget::NewPanel);
                    }
                    TabHitbox::SplitHorizontal(_) => {
                        self.open_split_spawn_dialog(SplitAxis::Horizontal);
                    }
                    TabHitbox::SplitVertical(_) => {
                        self.open_split_spawn_dialog(SplitAxis::Vertical);
                    }
                    TabHitbox::Panel { panel_id, .. } => {
                        actions.extend(self.switch_panel(panel_id));
                    }
                    TabHitbox::UnattachedTerminal { address, .. } => {
                        self.open_terminal_in_isolated_panel(address, &mut actions);
                    }
                }
                self.dirty = true;
                return actions;
            }
        }
        let shift_held = mouse.modifiers.contains(KeyModifiers::SHIFT);
        let tree = self
            .layouts
            .spaces
            .get(&self.active_space)
            .and_then(SpaceLayout::active_panel)
            .map(|p| &p.tree);
        let sessions = &self.sessions;
        let action = handle_mouse_with_passthrough(
            mouse,
            self.last_sidebar_areas,
            &self.last_geom,
            &mut self.drag,
            |pid| {
                if shift_held {
                    return false;
                }
                tree.and_then(|t| t.find_terminal(pid))
                    .and_then(|t| sessions.get(&t.address))
                    .is_some_and(|sess| {
                        sess.state == SessionState::Active
                            && sess.screen.modes().mouse_mode != MouseProtocolMode::None
                    })
            },
        );
        match action {
            MouseAction::SelectSidebarRow { panel, idx } => {
                let len = match panel {
                    SidebarPanel::Spaces => self.sidebar.visible_items.len(),
                    SidebarPanel::Agents => self.sidebar.agent_items.len(),
                };
                if idx < len {
                    self.sidebar.active_panel = panel;
                    match panel {
                        SidebarPanel::Spaces => self.sidebar.selected = idx,
                        SidebarPanel::Agents => self.sidebar.agent_selected = idx,
                    }
                    self.sidebar_focused = true;
                    match self.sidebar.selected_item().cloned() {
                        Some(SidebarItem::Terminal { address, space, .. }) => {
                            self.open_sidebar_terminal(address, space, &mut actions);
                        }
                        Some(SidebarItem::Space { name, .. }) => {
                            if let Some(removed) =
                                self.switch_space_with_actions(name.clone(), &mut actions)
                            {
                                actions.push(AppAction::RemoveLayout(removed));
                            }
                            self.refresh_sidebar();
                            self.select_sidebar_space(&name);
                        }
                        Some(SidebarItem::NewSpaceAction) => {
                            self.dialog = Some(ActiveDialog::NewSpace(NewSpaceDialog::default()));
                        }
                        _ => {}
                    }
                    self.dirty = true;
                }
            }
            MouseAction::FocusPane(pid) => {
                self.set_focused_pane(Some(pid));
                self.sidebar_focused = false;
                self.selection = None;
                self.dirty = true;
            }
            MouseAction::StartSelect { pane_id, start } => {
                self.set_focused_pane(Some(pane_id));
                self.sidebar_focused = false;
                self.selection = Some(TextSelection {
                    pane_id,
                    start,
                    end: start,
                });
                self.dirty = true;
            }
            MouseAction::UpdateSelect { pane_id, end } => {
                if let Some(sel) = &mut self.selection {
                    if sel.pane_id == pane_id {
                        sel.end = end;
                        self.dirty = true;
                    }
                }
            }
            MouseAction::EndSelect {
                pane_id,
                start,
                end,
            } => {
                if start == end {
                    self.selection = None;
                } else {
                    if let Some(sel) = &mut self.selection {
                        if sel.pane_id == pane_id {
                            sel.end = end;
                        }
                    }
                    if let Some(text) = self.extract_selection_text() {
                        if !text.is_empty() {
                            actions.push(AppAction::CopyToClipboard(text));
                        }
                    }
                }
                self.dirty = true;
            }
            MouseAction::TakeOverPane(pid) => {
                self.set_focused_pane(Some(pid));
                self.sidebar_focused = false;
                if let Some(addr) = self
                    .current_tree()
                    .find_terminal(pid)
                    .map(|t| t.address.clone())
                {
                    actions.push(AppAction::TakeOver(addr));
                }
                self.dirty = true;
            }
            MouseAction::SpawnInPane(pid) => {
                self.set_focused_pane(Some(pid));
                self.sidebar_focused = false;
                let space_name = self.active_space.clone();
                let panel_id = self
                    .layouts
                    .spaces
                    .entry(space_name)
                    .or_default()
                    .active_panel_mut_or_default()
                    .id;
                self.open_spawn_for_target(SpawnTarget::IntoPane {
                    panel_id,
                    pane_id: pid,
                });
                self.dirty = true;
            }
            MouseAction::DragDivider {
                target_pane,
                new_ratio,
                ..
            } => {
                self.current_tree_mut().set_ratio_at(target_pane, new_ratio);
                let area = self.last_panes_area;
                let rects = self.current_tree().compute_rects(area);
                for (pid, r) in rects {
                    if let Some(addr) = self
                        .current_tree()
                        .find_terminal(pid)
                        .map(|t| t.address.clone())
                    {
                        let cols = r.width.saturating_sub(2).max(1);
                        let rows = r.height.saturating_sub(2).max(1);
                        if let Some(sess) = self.sessions.get_mut(&addr) {
                            if let Ok(true) = sess.request_resize(cols, rows, now) {
                                actions.push(AppAction::ResizePane {
                                    terminal: addr,
                                    cols,
                                    rows,
                                });
                            }
                            let _ = sess.take_pending_redraw();
                        }
                    }
                }
                self.dirty = true;
            }
            MouseAction::EndDrag => {
                self.clear_outer_resize_settle();
                let mut flushed = false;
                for sess in self.sessions.values_mut() {
                    if sess.state == SessionState::Active {
                        if let Ok(true) = sess.flush_pending_resize_now(now) {
                            flushed = true;
                        }
                        if sess.take_pending_redraw() {
                            flushed = true;
                        }
                    }
                }
                if flushed {
                    self.dirty = true;
                }
                actions.push(AppAction::SaveLayout(self.active_space.clone()));
            }
            MouseAction::ScrollUp(pid) => {
                if let Some(addr) = self
                    .current_tree()
                    .find_terminal(pid)
                    .map(|t| t.address.clone())
                {
                    if let Some(sess) = self.sessions.get_mut(&addr) {
                        let cur = sess.screen.scrollback();
                        sess.screen.set_scrollback(cur + 3);
                        self.dirty = true;
                    }
                }
            }
            MouseAction::ScrollDown(pid) => {
                if let Some(addr) = self
                    .current_tree()
                    .find_terminal(pid)
                    .map(|t| t.address.clone())
                {
                    if let Some(sess) = self.sessions.get_mut(&addr) {
                        let cur = sess.screen.scrollback();
                        sess.screen.set_scrollback(cur.saturating_sub(3));
                        self.dirty = true;
                    }
                }
            }
            MouseAction::PaneMouse {
                pane_id,
                rel_col,
                rel_row,
            } => {
                if matches!(mouse.kind, MouseEventKind::Down(_)) {
                    self.set_focused_pane(Some(pane_id));
                    self.sidebar_focused = false;
                    self.selection = None;
                    self.dirty = true;
                }
                if let Some(addr) = self
                    .current_tree()
                    .find_terminal(pane_id)
                    .map(|t| t.address.clone())
                {
                    if let Some(sess) = self.sessions.get(&addr) {
                        if sess.state == SessionState::Active {
                            let modes = sess.screen.modes();
                            if let Some(bytes) = encode_mouse(mouse, rel_col, rel_row, modes) {
                                actions.push(AppAction::SendInput {
                                    terminal: addr,
                                    bytes,
                                });
                            }
                        }
                    }
                }
            }
            MouseAction::StartDragDivider(_) | MouseAction::None => {}
        }
        actions
    }

    pub fn switch_space(&mut self, space: String) -> Option<String> {
        let prev = self.active_space.clone();
        let mut removed_prev = None;
        if prev != space {
            if prev == UNASSIGNED_SPACE {
                if self
                    .layouts
                    .spaces
                    .get(&prev)
                    .is_some_and(|s| !s.has_terminals())
                {
                    self.layouts.spaces.remove(&prev);
                    removed_prev = Some(prev);
                }
            } else if !self.space_has_open_terminals(&prev) {
                self.layouts.spaces.remove(&prev);
                self.model.declared_spaces.remove(&prev);
                removed_prev = Some(prev);
            }
        }
        self.selection = None;
        self.drag = MouseDragState::default();
        self.active_space = space.clone();
        let entry = self.layouts.spaces.entry(space).or_default();
        entry.ensure_valid_active_panel();
        self.focused_pane = entry.active_panel().and_then(|p| {
            p.focused_pane
                .or_else(|| p.tree.pane_ids().into_iter().next())
        });
        self.dirty = true;
        removed_prev
    }

    pub fn switch_space_with_actions(
        &mut self,
        space: String,
        actions: &mut Vec<AppAction>,
    ) -> Option<String> {
        let removed = self.switch_space(space);
        self.attach_unattached_in_active_panel(actions);
        removed
    }

    fn apply_command(&mut self, cmd: Command, now: Instant, actions: &mut Vec<AppAction>) {
        match cmd {
            Command::SplitHorizontal | Command::SplitVertical => {
                self.clear_outer_resize_settle();
                let axis = if cmd == Command::SplitHorizontal {
                    SplitAxis::Horizontal
                } else {
                    SplitAxis::Vertical
                };
                if let Some(pid) = self.focused_pane {
                    if let Some(new_id) = self.current_tree_mut().split(pid, axis, None) {
                        self.set_focused_pane(Some(new_id));
                        actions.push(AppAction::SaveLayout(self.active_space.clone()));
                    }
                }
            }
            Command::ClosePane => {
                self.clear_outer_resize_settle();
                if let Some(pid) = self.focused_pane {
                    self.close_focused_pane(pid, now, actions);
                }
            }
            Command::FocusDir(dir) => {
                if let Some(pid) = self.focused_pane {
                    if let Some(next) =
                        self.current_tree()
                            .focus_dir(self.last_panes_area, pid, dir)
                    {
                        self.set_focused_pane(Some(next));
                        self.sidebar_focused = false;
                    }
                }
            }
            Command::TakeOver => {
                if let Some(pid) = self.focused_pane {
                    if let Some(addr) = self
                        .current_tree()
                        .find_terminal(pid)
                        .map(|t| t.address.clone())
                    {
                        actions.push(AppAction::TakeOver(addr));
                    }
                }
            }
            Command::OpenSpawnDialog => {
                let target = match (self.active_panel_id(), self.focused_pane) {
                    (Some(panel_id), Some(pane_id))
                        if self.current_tree().find_terminal(pane_id).is_none() =>
                    {
                        SpawnTarget::IntoPane { panel_id, pane_id }
                    }
                    _ => SpawnTarget::NewPanel,
                };
                self.open_spawn_for_target(target);
            }
            Command::OpenNewSpace => {
                self.dialog = Some(ActiveDialog::NewSpace(NewSpaceDialog::default()));
            }
            Command::ConfirmKill => {
                if let Some(addr) = self.focused_terminal_addr() {
                    self.dialog = Some(ActiveDialog::Confirm(ConfirmDialog::kill(addr)));
                }
            }
            Command::ConfirmRemove => {
                if let Some(addr) = self.focused_terminal_addr() {
                    let is_live = self
                        .model
                        .terminals
                        .get(&addr)
                        .is_some_and(Terminal::is_live);
                    let dlg = if is_live {
                        ConfirmDialog::kill_before_remove(addr)
                    } else {
                        ConfirmDialog::remove(addr)
                    };
                    self.dialog = Some(ActiveDialog::Confirm(dlg));
                }
            }
            Command::OpenRenameSpace => {
                if self.active_space != UNASSIGNED_SPACE {
                    self.dialog = Some(ActiveDialog::RenameSpace(RenameSpaceDialog::new(
                        self.active_space.clone(),
                    )));
                }
            }
            Command::ToggleSidebar => {
                self.clear_outer_resize_settle();
                self.sidebar_visible = !self.sidebar_visible;
                if !self.sidebar_visible {
                    self.sidebar_focused = false;
                }
            }
            Command::ToggleFocusSidebar => {
                if self.sidebar_visible {
                    self.sidebar_focused = !self.sidebar_focused;
                }
            }
            Command::OpenHelp => {
                self.dialog = Some(ActiveDialog::Help(HelpDialog));
            }
            Command::Quit => {
                self.cleanup_empty_spaces_on_quit();
                let tree = self
                    .model
                    .build_tree(self.config.ui.show_finished, self.config.ui.show_unassigned);
                self.sidebar.rebuild(&tree, Some(&self.active_space));
                self.should_quit = true;
                actions.push(AppAction::Quit);
            }
            Command::SplitSpawnHorizontal => {
                self.open_split_spawn_dialog(SplitAxis::Horizontal);
            }
            Command::SplitSpawnVertical => {
                self.open_split_spawn_dialog(SplitAxis::Vertical);
            }
            Command::PrevPanel => {
                actions.extend(self.prev_panel());
            }
            Command::NextPanel => {
                actions.extend(self.next_panel());
            }
        }
    }

    pub fn focused_terminal_addr(&self) -> Option<String> {
        let pid = self.focused_pane?;
        self.current_tree()
            .find_terminal(pid)
            .map(|t| t.address.clone())
    }

    pub fn focused_cursor_position(&self) -> Option<(u16, u16)> {
        if self.sidebar_focused || self.dialog.is_some() {
            return None;
        }
        let pid = self.focused_pane?;
        let hitbox = self.last_geom.panes.iter().find(|p| p.pane_id == pid)?;
        let addr = self
            .current_tree()
            .find_terminal(pid)
            .map(|t| t.address.as_str())?;
        let sess = self.sessions.get(addr)?;
        if sess.screen.scrollback() > 0 {
            return None;
        }
        let (col, row, visible) = sess.screen.cursor();
        if !visible {
            return None;
        }
        let inner_x = hitbox.rect.x + 1;
        let inner_y = hitbox.rect.y + 1;
        let inner_w = hitbox.rect.width.saturating_sub(2);
        let inner_h = hitbox.rect.height.saturating_sub(2);
        if col < inner_w && row < inner_h {
            Some((inner_x + col, inner_y + row))
        } else {
            None
        }
    }

    pub fn extract_selection_text(&self) -> Option<String> {
        let sel = self.selection?;
        let addr = self
            .current_tree()
            .find_terminal(sel.pane_id)
            .map(|t| t.address.as_str())?;
        let sess = self.sessions.get(addr)?;
        let (inner_w, inner_h) = self.pane_inner_size(sel.pane_id);
        Some(extract_selected_text(&*sess.screen, &sel, inner_w, inner_h))
    }

    pub fn pane_inner_size(&self, pane_id: PaneId) -> (u16, u16) {
        if self.last_panes_area.width > 0 && self.last_panes_area.height > 0 {
            if let Some((_, rect)) = self
                .current_tree()
                .compute_rects(self.last_panes_area)
                .into_iter()
                .find(|(pid, _)| *pid == pane_id)
            {
                return (
                    rect.width.saturating_sub(2).max(1),
                    rect.height.saturating_sub(2).max(1),
                );
            }
        }
        if let Some(p) = self.last_geom.panes.iter().find(|p| p.pane_id == pane_id) {
            return (
                p.rect.width.saturating_sub(2).max(1),
                p.rect.height.saturating_sub(2).max(1),
            );
        }
        if self.last_panes_area.width > 0 && self.last_panes_area.height > 0 {
            return (
                self.last_panes_area.width.saturating_sub(2).max(1),
                self.last_panes_area.height.saturating_sub(2).max(1),
            );
        }
        (80, 24)
    }

    pub fn predict_spawn_inner_size(&self, target: &SpawnTarget) -> Option<(u16, u16)> {
        if self.last_panes_area.width == 0 || self.last_panes_area.height == 0 {
            return None;
        }
        let inner = |r: Rect| {
            (
                r.width.saturating_sub(2).max(1),
                r.height.saturating_sub(2).max(1),
            )
        };
        match *target {
            SpawnTarget::IntoPane { panel_id, pane_id } => {
                if let Some(panel) = self
                    .layouts
                    .spaces
                    .get(&self.active_space)
                    .and_then(|s| s.panel(panel_id))
                {
                    if let Some((_, rect)) = panel
                        .tree
                        .compute_rects(self.last_panes_area)
                        .into_iter()
                        .find(|(pid, _)| *pid == pane_id)
                    {
                        return Some(inner(rect));
                    }
                }
                Some(inner(self.last_panes_area))
            }
            SpawnTarget::SplitPane {
                panel_id,
                pane_id,
                axis,
            } => {
                if let Some(panel) = self
                    .layouts
                    .spaces
                    .get(&self.active_space)
                    .and_then(|s| s.panel(panel_id))
                {
                    let mut tree = panel.tree.clone();
                    if let Some(new_pane_id) = tree.split(pane_id, axis, None) {
                        if let Some((_, rect)) = tree
                            .compute_rects(self.last_panes_area)
                            .into_iter()
                            .find(|(pid, _)| *pid == new_pane_id)
                        {
                            return Some(inner(rect));
                        }
                    }
                }
                Some(inner(self.last_panes_area))
            }
            SpawnTarget::NewPanel => Some(inner(self.last_panes_area)),
        }
    }

    pub fn sync_pane_sizes(&mut self, now: Instant) -> bool {
        let targets: Vec<(PaneId, u16, u16)> =
            if self.last_panes_area.width > 0 && self.last_panes_area.height > 0 {
                self.current_tree()
                    .compute_rects(self.last_panes_area)
                    .into_iter()
                    .map(|(pid, rect)| {
                        (
                            pid,
                            rect.width.saturating_sub(2).max(1),
                            rect.height.saturating_sub(2).max(1),
                        )
                    })
                    .collect()
            } else {
                self.last_geom
                    .panes
                    .iter()
                    .map(|p| {
                        (
                            p.pane_id,
                            p.rect.width.saturating_sub(2).max(1),
                            p.rect.height.saturating_sub(2).max(1),
                        )
                    })
                    .collect()
            };
        let mut resized = false;
        for (pid, cols, rows) in targets {
            if let Some(addr) = self
                .current_tree()
                .find_terminal(pid)
                .map(|t| t.address.clone())
            {
                if let Some(sess) = self.sessions.get_mut(&addr) {
                    if sess.state == SessionState::Active && sess.screen.size() != (cols, rows) {
                        if let Ok(true) = sess.request_resize(cols, rows, now) {
                            resized = true;
                        }
                        if sess.take_pending_redraw() {
                            self.dirty = true;
                        }
                    }
                }
            }
        }
        if resized {
            self.dirty = true;
        }
        resized
    }

    pub fn ensure_session_stub(&mut self, addr: &str) {
        if !self.sessions.contains_key(addr) {
            let mut sess = AttachSession::new(addr.to_string(), 80, 24);
            sess.state = SessionState::Connecting;
            self.sessions.insert(addr.to_string(), sess);
        }
    }

    pub fn space_has_open_terminals(&self, space: &str) -> bool {
        let has_pane_term = self
            .layouts
            .spaces
            .get(space)
            .is_some_and(SpaceLayout::has_terminals);
        if has_pane_term {
            return true;
        }
        self.model
            .terminals
            .values()
            .any(|t| t.is_live() && t.labels.get("space").map(String::as_str) == Some(space))
    }

    fn close_focused_pane(&mut self, pid: PaneId, now: Instant, actions: &mut Vec<AppAction>) {
        let cur_space = self.active_space.clone();
        let closed_addr = self
            .current_tree()
            .find_terminal(pid)
            .map(|t| t.address.clone());
        let had_multi_panes = self.current_tree().pane_ids().len() > 1;
        let panel_count_before = self
            .layouts
            .spaces
            .get(&cur_space)
            .map_or(1, |s| s.panels.len());
        let active_panel_id = self.active_panel_id();

        if let Some(addr) = closed_addr.as_deref() {
            self.current_tree_mut().set_terminal(pid, None);
            self.kill_or_remove_terminal(addr, actions);
        }

        if had_multi_panes {
            self.current_tree_mut().close(pid);
            let next_pid = self.current_tree().pane_ids().into_iter().next();
            self.set_focused_pane(next_pid);

            if panel_count_before > 1 {
                if let Some(panel_id) = active_panel_id {
                    let panel_empty = self
                        .layouts
                        .spaces
                        .get(&cur_space)
                        .and_then(|s| s.panel(panel_id))
                        .is_some_and(|p| !p.has_terminals());
                    if panel_empty {
                        self.remove_panel_and_attach_active(&cur_space, panel_id, actions);
                    }
                }
            }
            self.finalize_space_after_pane_close(&cur_space, actions);
        } else if panel_count_before > 1 {
            if let Some(panel_id) = active_panel_id {
                self.remove_panel_and_attach_active(&cur_space, panel_id, actions);
            }
            self.finalize_space_after_pane_close(&cur_space, actions);
        } else if cur_space != UNASSIGNED_SPACE {
            self.close_space_terminals(&cur_space, actions);
            actions.push(AppAction::RemoveLayout(cur_space.clone()));
            self.remove_empty_space_with_actions(&cur_space, actions);
        } else {
            let extra_removed = self.close_space_terminals(UNASSIGNED_SPACE, actions);
            if let Some(next_space) = self.next_named_space() {
                self.layouts.spaces.remove(UNASSIGNED_SPACE);
                actions.push(AppAction::RemoveLayout(UNASSIGNED_SPACE.to_string()));
                self.switch_space_with_actions(next_space, actions);
            } else if closed_addr.is_some() || extra_removed > 0 {
                actions.push(AppAction::RemoveLayout(UNASSIGNED_SPACE.to_string()));
            } else {
                self.set_toast("Workspace is already empty", now);
            }
        }
        self.refresh_sidebar();
    }

    fn remove_panel_and_attach_active(
        &mut self,
        space: &str,
        panel_id: PanelId,
        actions: &mut Vec<AppAction>,
    ) {
        if let Some(space_layout) = self.layouts.spaces.get_mut(space) {
            space_layout.remove_panel(panel_id);
        }
        self.sync_focused_pane_from_active_panel();
        self.selection = None;
        self.drag = MouseDragState::default();
        self.attach_unattached_in_active_panel(actions);
    }

    fn finalize_space_after_pane_close(&mut self, cur_space: &str, actions: &mut Vec<AppAction>) {
        if cur_space != UNASSIGNED_SPACE {
            if !self.space_has_open_terminals(cur_space) {
                actions.push(AppAction::RemoveLayout(cur_space.to_string()));
                self.remove_empty_space_with_actions(cur_space, actions);
            } else {
                actions.push(AppAction::SaveLayout(cur_space.to_string()));
            }
        } else if !self
            .layouts
            .spaces
            .get(UNASSIGNED_SPACE)
            .is_some_and(SpaceLayout::has_terminals)
        {
            self.finalize_empty_unassigned_space(actions);
        } else {
            actions.push(AppAction::SaveLayout(cur_space.to_string()));
        }
    }

    fn finalize_empty_unassigned_space(&mut self, actions: &mut Vec<AppAction>) {
        if let Some(next_space) = self.next_named_space() {
            self.layouts.spaces.remove(UNASSIGNED_SPACE);
            actions.push(AppAction::RemoveLayout(UNASSIGNED_SPACE.to_string()));
            self.switch_space_with_actions(next_space, actions);
        } else {
            if let Some(space_layout) = self.layouts.spaces.get_mut(UNASSIGNED_SPACE) {
                space_layout.collapse_empty_splits_or_prune();
            }
            self.sync_focused_pane_from_active_panel();
            actions.push(AppAction::RemoveLayout(UNASSIGNED_SPACE.to_string()));
        }
    }

    fn next_named_space(&self) -> Option<String> {
        self.layouts
            .spaces
            .keys()
            .filter(|k| k.as_str() != UNASSIGNED_SPACE)
            .min()
            .cloned()
    }

    fn kill_or_remove_terminal(&mut self, addr: &str, actions: &mut Vec<AppAction>) {
        let is_finished = self.model.terminals.get(addr).is_some_and(|t| !t.is_live());
        self.sessions.remove(addr);
        self.model.terminals.remove(addr);
        if is_finished {
            actions.push(AppAction::Remove(addr.to_string()));
        } else {
            actions.push(AppAction::Kill(addr.to_string()));
            actions.push(AppAction::Remove(addr.to_string()));
        }
    }

    fn close_space_terminals(&mut self, space: &str, actions: &mut Vec<AppAction>) -> usize {
        let mut addrs: Vec<String> = self
            .model
            .terminals
            .iter()
            .filter(|(_, t)| {
                TerminalEntry {
                    terminal: (*t).clone(),
                }
                .space_name()
                    == space
            })
            .map(|(addr, _)| addr.clone())
            .collect();
        addrs.sort();
        let count = addrs.len();
        for addr in addrs {
            self.kill_or_remove_terminal(&addr, actions);
        }
        count
    }

    pub fn remove_empty_space(&mut self, space: &str) {
        if space == UNASSIGNED_SPACE {
            return;
        }
        self.model.declared_spaces.remove(space);
        self.layouts.spaces.remove(space);
        if self.active_space == space {
            let next_space = self
                .next_named_space()
                .unwrap_or_else(|| UNASSIGNED_SPACE.to_string());
            self.switch_space(next_space);
        }
    }

    pub fn remove_empty_space_with_actions(&mut self, space: &str, actions: &mut Vec<AppAction>) {
        if space == UNASSIGNED_SPACE {
            return;
        }
        self.model.declared_spaces.remove(space);
        self.layouts.spaces.remove(space);
        if self.active_space == space {
            let next_space = self
                .next_named_space()
                .unwrap_or_else(|| UNASSIGNED_SPACE.to_string());
            self.switch_space_with_actions(next_space, actions);
        }
    }

    /// Removes every space without terminal panels (including `UNASSIGNED_SPACE`)
    /// from `layouts.spaces` and `model.declared_spaces`, returning the removed keys.
    pub fn cleanup_empty_spaces_on_quit(&mut self) -> HashSet<String> {
        let mut removed: HashSet<String> = self
            .layouts
            .spaces
            .iter()
            .filter(|(_, space)| !space.has_terminals())
            .map(|(sp, _)| sp.clone())
            .collect();
        for sp in &self.model.declared_spaces {
            if !self
                .layouts
                .spaces
                .get(sp)
                .is_some_and(SpaceLayout::has_terminals)
            {
                removed.insert(sp.clone());
            }
        }
        for sp in &removed {
            self.layouts.spaces.remove(sp);
            self.model.declared_spaces.remove(sp);
        }
        removed
    }

    /// Prunes removed or re-labeled terminals from all layouts, collapses empty splits,
    /// and removes emptied spaces (preserving `active_space` when `pruned == 0`).
    pub fn prune_empty_spaces(&mut self) -> Vec<String> {
        let present: HashSet<String> = self.model.terminals.keys().cloned().collect();
        let mut pruned_by_space: HashMap<String, usize> = HashMap::new();
        let initial_active = self.active_space.clone();

        for (sp, space_layout) in &mut self.layouts.spaces {
            let mut pruned = space_layout.prune_removed(&present);
            let mut label_pruned = false;
            for panel in &mut space_layout.panels {
                for pid in panel.tree.pane_ids() {
                    if let Some(term_ref) = panel.tree.find_terminal(pid) {
                        if let Some(term) = self.model.terminals.get(&term_ref.address) {
                            let term_space = term
                                .labels
                                .get("space")
                                .map(String::as_str)
                                .filter(|s| !s.is_empty())
                                .unwrap_or(UNASSIGNED_SPACE);
                            if term_space != sp.as_str() {
                                panel.tree.set_terminal(pid, None);
                                pruned += 1;
                                label_pruned = true;
                            }
                        }
                    }
                }
            }
            if label_pruned {
                space_layout.collapse_empty_splits_or_prune();
            }
            if !space_layout.has_terminals() && (pruned > 0 || sp != &initial_active) {
                space_layout.collapse_empty_splits_or_prune();
            }
            if pruned > 0 && sp == &initial_active {
                space_layout.ensure_valid_active_panel();
                self.focused_pane = space_layout.active_panel().and_then(|p| {
                    p.focused_pane
                        .or_else(|| p.tree.pane_ids().into_iter().next())
                });
            }
            if pruned > 0 {
                pruned_by_space.insert(sp.clone(), pruned);
            }
        }

        let mut candidate_spaces: HashSet<String> = self.layouts.spaces.keys().cloned().collect();
        candidate_spaces.extend(self.model.declared_spaces.iter().cloned());

        let mut emptied = Vec::new();
        let mut named_to_remove = Vec::new();
        for sp in candidate_spaces {
            let pruned = pruned_by_space.get(&sp).copied().unwrap_or(0);
            if sp == UNASSIGNED_SPACE {
                let is_empty = self
                    .layouts
                    .spaces
                    .get(UNASSIGNED_SPACE)
                    .is_some_and(|s| !s.has_terminals());
                if is_empty && (sp != initial_active || pruned > 0) {
                    if sp != initial_active {
                        self.layouts.spaces.remove(UNASSIGNED_SPACE);
                    }
                    emptied.push(sp);
                }
                continue;
            }
            if !self.space_has_open_terminals(&sp) && (sp != initial_active || pruned > 0) {
                named_to_remove.push(sp);
            }
        }

        for sp in &named_to_remove {
            self.model.declared_spaces.remove(sp);
            self.layouts.spaces.remove(sp);
            emptied.push(sp.clone());
        }
        if named_to_remove.iter().any(|sp| sp == &self.active_space) {
            let next_space = self
                .next_named_space()
                .unwrap_or_else(|| UNASSIGNED_SPACE.to_string());
            self.switch_space(next_space);
        }

        emptied.sort();
        emptied
    }

    fn sync_focused_pane_from_active_panel(&mut self) {
        self.focused_pane = self
            .layouts
            .spaces
            .get(&self.active_space)
            .and_then(SpaceLayout::active_panel)
            .and_then(|p| {
                p.focused_pane
                    .or_else(|| p.tree.pane_ids().into_iter().next())
            });
    }

    pub fn place_spawned_terminal(&mut self, addr: &str, target: SpawnTarget) {
        let space_name = self.active_space.clone();
        let space_layout = self.layouts.spaces.entry(space_name).or_default();
        let term_ref = TermRef {
            address: addr.to_string(),
        };

        match target {
            SpawnTarget::IntoPane { panel_id, pane_id } => {
                if space_layout.set_active_panel(panel_id) {
                    if let Some(panel) = space_layout.panel_mut(panel_id) {
                        if panel.tree.set_terminal(pane_id, Some(term_ref.clone())) {
                            panel.focused_pane = Some(pane_id);
                            self.focused_pane = Some(pane_id);
                            return;
                        }
                    }
                }
            }
            SpawnTarget::SplitPane {
                panel_id,
                pane_id,
                axis,
            } => {
                if space_layout.set_active_panel(panel_id) {
                    if let Some(panel) = space_layout.panel_mut(panel_id) {
                        if let Some(new_pane_id) =
                            panel.tree.split(pane_id, axis, Some(term_ref.clone()))
                        {
                            panel.focused_pane = Some(new_pane_id);
                            self.focused_pane = Some(new_pane_id);
                            return;
                        }
                    }
                }
            }
            SpawnTarget::NewPanel => {}
        }

        let (_panel_id, pane_id) =
            space_layout.place_in_new_or_empty_panel(addr.to_string(), false);
        self.focused_pane = Some(pane_id);
    }

    /// Deterministically places live terminals not yet present in any panel into
    /// individual panels in their space (sorted by `(created_ms, address)`), without
    /// stealing `active_panel` when the active panel is already occupied.
    pub fn reconcile_live_terminals(&mut self) -> Vec<String> {
        let mut placed: HashSet<String> = HashSet::new();
        for space_layout in self.layouts.spaces.values() {
            for addr in space_layout.terminal_addresses() {
                placed.insert(addr);
            }
        }

        let mut unassigned: Vec<(u64, String, String)> = self
            .model
            .terminals
            .values()
            .filter(|t| t.is_live() && !placed.contains(&t.address))
            .map(|t| {
                let space = TerminalEntry {
                    terminal: t.clone(),
                }
                .space_name()
                .to_string();
                (t.created_ms, t.address.clone(), space)
            })
            .collect();
        unassigned.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));

        let mut modified_spaces: HashSet<String> = HashSet::new();
        for (_, addr, space_name) in unassigned {
            if space_name != UNASSIGNED_SPACE {
                self.model.declared_spaces.insert(space_name.clone());
            }
            let space_layout = self.layouts.spaces.entry(space_name.clone()).or_default();
            space_layout.place_in_new_or_empty_panel(addr, true);
            if space_name == self.active_space {
                self.sync_focused_pane_from_active_panel();
            }
            modified_spaces.insert(space_name);
        }

        let mut result: Vec<String> = modified_spaces.into_iter().collect();
        result.sort();
        result
    }

    /// Removes `addr` from all space pane trees and returns any spaces
    /// that now have zero open terminals (including `UNASSIGNED_SPACE` when its tree has no terminals).
    pub fn prune_terminal_from_spaces(&mut self, addr: &str) -> Vec<String> {
        let mut affected = HashSet::new();
        if let Some(sp) = self
            .model
            .terminals
            .get(addr)
            .and_then(|t| t.labels.get("space"))
            .filter(|s| !s.is_empty())
        {
            affected.insert(sp.clone());
        }
        let active_space = self.active_space.clone();
        for (sp, space_layout) in &mut self.layouts.spaces {
            if space_layout.prune_terminal(addr) {
                affected.insert(sp.clone());
                if sp == &active_space {
                    self.focused_pane = space_layout.active_panel().and_then(|p| {
                        p.focused_pane
                            .or_else(|| p.tree.pane_ids().into_iter().next())
                    });
                }
            }
        }
        let mut emptied: Vec<String> = affected
            .into_iter()
            .filter(|sp| {
                if sp == UNASSIGNED_SPACE {
                    self.layouts
                        .spaces
                        .get(UNASSIGNED_SPACE)
                        .is_some_and(|s| !s.has_terminals())
                } else {
                    let has_pane_term = self
                        .layouts
                        .spaces
                        .get(sp)
                        .is_some_and(SpaceLayout::has_terminals);
                    let has_other_live = self.model.terminals.values().any(|t| {
                        t.address != addr
                            && t.is_live()
                            && t.labels.get("space").map(String::as_str) == Some(sp.as_str())
                    });
                    !has_pane_term && !has_other_live
                }
            })
            .collect();
        emptied.sort();
        emptied
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn submit_new_space_rejects_blank_and_unassigned_names() {
        let mut app = TuiApp::new(Config::default(), None);
        let now = Instant::now();

        for invalid in ["   ", "", UNASSIGNED_SPACE] {
            let mut actions = Vec::new();
            app.apply_dialog_outcome(
                DialogOutcome::SubmitNewSpace(invalid.into()),
                now,
                &mut actions,
            );
            assert!(actions.is_empty());
            assert!(!app.model.declared_spaces.contains(invalid.trim()));
            assert_eq!(app.active_space, UNASSIGNED_SPACE);
            assert_eq!(app.active_toast(now), Some("Invalid space name"));
        }
    }

    #[test]
    fn multipanel_close_pane_and_spawn_placement_and_reconcile() {
        let mut app = TuiApp::new(Config::default(), Some("api".into()));
        let mut t1 = Terminal {
            address: "t1".into(),
            state: "live".into(),
            argv: vec!["zsh".into()],
            created_ms: 10,
            ..Default::default()
        };
        t1.labels.insert("space".into(), "api".into());
        let mut t2 = Terminal {
            address: "t2".into(),
            state: "live".into(),
            argv: vec!["claude".into()],
            created_ms: 20,
            ..Default::default()
        };
        t2.labels.insert("space".into(), "api".into());
        let mut t3 = Terminal {
            address: "t3".into(),
            state: "live".into(),
            argv: vec!["bash".into()],
            created_ms: 5,
            ..Default::default()
        };
        t3.labels.insert("space".into(), "api".into());

        app.model.terminals.insert("t1".into(), t1);
        app.model.terminals.insert("t2".into(), t2);
        app.model.terminals.insert("t3".into(), t3);

        // Reconcile deterministically by (created_ms, address): t3 (5), t1 (10), t2 (20)
        let changed = app.reconcile_live_terminals();
        assert_eq!(changed, vec!["api".to_string()]);
        let api = &app.layouts.spaces["api"];
        assert_eq!(api.panels.len(), 3);
        assert_eq!(api.panels[0].terminal_addresses(), vec!["t3".to_string()]);
        assert_eq!(api.panels[1].terminal_addresses(), vec!["t1".to_string()]);
        assert_eq!(api.panels[2].terminal_addresses(), vec!["t2".to_string()]);
        // Active panel stayed on panel 1 (first filled panel, not stolen by t1 or t2)
        assert_eq!(api.active_panel, Some(api.panels[0].id));

        // SplitPane placement splits only panel 1
        let p1_id = api.panels[0].id;
        let p1_pane = api.panels[0].focused_pane.unwrap();
        let mut t4 = Terminal {
            address: "t4".into(),
            state: "live".into(),
            argv: vec!["sh".into()],
            created_ms: 30,
            ..Default::default()
        };
        t4.labels.insert("space".into(), "api".into());
        app.model.terminals.insert("t4".into(), t4);
        app.place_spawned_terminal(
            "t4",
            SpawnTarget::SplitPane {
                panel_id: p1_id,
                pane_id: p1_pane,
                axis: SplitAxis::Horizontal,
            },
        );
        assert_eq!(app.layouts.spaces["api"].panels[0].tree.pane_ids().len(), 2);
        assert_eq!(app.layouts.spaces["api"].panels[1].tree.pane_ids().len(), 1);

        // Close single-pane panel 2 -> removes only panel 2, keeps space and remaining panels
        let p2_id = app.layouts.spaces["api"].panels[1].id;
        app.switch_panel(p2_id);
        let now = Instant::now();
        let ctrl_b = KeyEvent::new(KeyCode::Char('b'), KeyModifiers::CONTROL);
        let key_x = KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE);
        app.handle_key(ctrl_b, now);
        let actions = app.handle_key(key_x, now + Duration::from_millis(20));
        assert!(actions.contains(&AppAction::Kill("t1".into())));
        assert!(actions.contains(&AppAction::SaveLayout("api".into())));
        assert!(!actions.contains(&AppAction::RemoveLayout("api".into())));
        assert_eq!(app.layouts.spaces["api"].panels.len(), 2);
        assert!(app.layouts.spaces.contains_key("api"));
    }

    #[test]
    fn switch_panel_and_switch_space_clear_drag_and_selection() {
        let mut app = TuiApp::new(Config::default(), Some("api".into()));
        let (p2_id, _) = app
            .layouts
            .spaces
            .get_mut("api")
            .unwrap()
            .add_panel(Some(TermRef {
                address: "t2".into(),
            }));
        app.layouts
            .spaces
            .get_mut("api")
            .unwrap()
            .set_active_panel(1);
        app.selection = Some(TextSelection {
            pane_id: 1,
            start: (0, 0),
            end: (5, 0),
        });
        app.drag.active_selection = Some((1, Rect::new(0, 0, 20, 10), (0, 0)));
        app.drag.active_passthrough_pane = Some((1, Rect::new(0, 0, 20, 10)));

        app.switch_panel(p2_id);
        assert!(app.selection.is_none());
        assert!(app.drag.active_selection.is_none());
        assert!(app.drag.active_passthrough_pane.is_none());

        app.selection = Some(TextSelection {
            pane_id: 1,
            start: (1, 1),
            end: (3, 1),
        });
        app.drag.active_selection = Some((1, Rect::new(0, 0, 20, 10), (1, 1)));
        app.switch_space("web".into());
        assert!(app.selection.is_none());
        assert!(app.drag.active_selection.is_none());
        assert!(app.drag.active_passthrough_pane.is_none());
    }

    #[test]
    fn pane_inner_size_uses_computed_rects_after_split_before_rerender() {
        let mut app = TuiApp::new(Config::default(), Some("api".into()));
        let area = Rect::new(0, 0, 120, 30);
        let mut buf = Buffer::empty(area);
        app.render(area, &mut buf, Instant::now());

        let p1 = app.focused_pane.unwrap();
        let (w_before, _) = app.pane_inner_size(p1);
        let p2 = app
            .current_tree_mut()
            .split(p1, SplitAxis::Horizontal, None)
            .unwrap();
        let (w_p1_after, _) = app.pane_inner_size(p1);
        let (w_p2_after, _) = app.pane_inner_size(p2);

        assert!(
            w_p1_after < w_before,
            "expected split p1 width {w_p1_after} < {w_before}"
        );
        assert!(
            w_p2_after < w_before,
            "expected split p2 width {w_p2_after} < {w_before}"
        );
    }

    #[test]
    fn close_pane_multi_pane_unassigned_empty_switches_to_named_space() {
        let mut app = TuiApp::new(Config::default(), None);
        let mut t_api = Terminal {
            address: "t-api".into(),
            state: "live".into(),
            argv: vec!["zsh".into()],
            ..Default::default()
        };
        t_api.labels.insert("space".into(), "api".into());
        app.model.terminals.insert("t-api".into(), t_api);
        app.model.declared_spaces.insert("api".into());
        let (api_space, _, api_pane) = SpaceLayout::with_single(Some(TermRef {
            address: "t-api".into(),
        }));
        app.layouts.spaces.insert("api".into(), api_space);

        // Unassigned has 1 terminal + 1 empty split pane
        let p1 = app.focused_pane.unwrap();
        app.current_tree_mut().set_terminal(
            p1,
            Some(TermRef {
                address: "t-u".into(),
            }),
        );
        app.model.terminals.insert(
            "t-u".into(),
            Terminal {
                address: "t-u".into(),
                state: "live".into(),
                argv: vec!["sh".into()],
                ..Default::default()
            },
        );
        app.current_tree_mut()
            .split(p1, SplitAxis::Horizontal, None)
            .unwrap();
        app.focused_pane = Some(p1);

        let now = Instant::now();
        let ctrl_b = KeyEvent::new(KeyCode::Char('b'), KeyModifiers::CONTROL);
        let key_x = KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE);
        app.handle_key(ctrl_b, now);
        let actions = app.handle_key(key_x, now + Duration::from_millis(20));

        assert_eq!(app.active_space, "api");
        assert!(!app.layouts.spaces.contains_key(UNASSIGNED_SPACE));
        assert!(actions.contains(&AppAction::RemoveLayout(UNASSIGNED_SPACE.into())));
        assert!(actions.contains(&AppAction::AttachTerminal {
            pane_id: api_pane,
            terminal: "t-api".into(),
        }));
    }

    #[test]
    fn outer_resize_settle_coalesces_trailing_edge_and_clears_on_layout_commands() {
        let mut app = TuiApp::new(Config::default(), Some("api".into()));
        assert!(!app.has_pending_outer_resize());

        let t0 = Instant::now();
        app.mark_outer_resize(t0);
        assert!(app.has_pending_outer_resize());
        assert!(!app.take_settled_outer_resize(t0 + Duration::from_millis(119)));
        assert!(app.has_pending_outer_resize());

        // Subsequent outer resize pushes deadline out (trailing-edge coalesce)
        app.mark_outer_resize(t0 + Duration::from_millis(50));
        assert!(!app.take_settled_outer_resize(t0 + OUTER_RESIZE_SETTLE));
        assert!(app.take_settled_outer_resize(t0 + Duration::from_millis(50) + OUTER_RESIZE_SETTLE));
        assert!(!app.has_pending_outer_resize());

        // Layout-mutating commands clear pending outer resize settle state immediately
        let ctrl_b = KeyEvent::new(KeyCode::Char('b'), KeyModifiers::CONTROL);
        for key_char in ['"', '%', 'b', 'x'] {
            app.mark_outer_resize(t0);
            assert!(app.has_pending_outer_resize());
            app.handle_key(ctrl_b, t0);
            app.handle_key(
                KeyEvent::new(KeyCode::Char(key_char), KeyModifiers::NONE),
                t0 + Duration::from_millis(10),
            );
            assert!(
                !app.has_pending_outer_resize(),
                "expected Ctrl-b {key_char} to clear outer_resize_settle_at"
            );
        }

        // EndDrag clears pending outer resize settle state
        let area = Rect::new(0, 0, 100, 30);
        let mut buf = Buffer::empty(area);
        let p1 = app.focused_pane.unwrap();
        app.current_tree_mut()
            .split(p1, SplitAxis::Horizontal, None);
        app.render(area, &mut buf, t0);
        let divider = app.last_geom.dividers[0];
        app.handle_mouse_event(
            MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: divider.line_pos,
                row: divider.container.y + 1,
                modifiers: KeyModifiers::NONE,
            },
            t0,
        );
        app.mark_outer_resize(t0);
        assert!(app.has_pending_outer_resize());
        app.handle_mouse_event(
            MouseEvent {
                kind: MouseEventKind::Up(MouseButton::Left),
                column: divider.line_pos,
                row: divider.container.y + 1,
                modifiers: KeyModifiers::NONE,
            },
            t0 + Duration::from_millis(5),
        );
        assert!(!app.has_pending_outer_resize());
    }

    #[test]
    fn compute_panes_area_matches_render_and_predict_spawn_inner_size_matches_placement() {
        let mut app = TuiApp::new(Config::default(), Some("api".into()));
        assert_eq!(app.predict_spawn_inner_size(&SpawnTarget::NewPanel), None);

        let area = Rect::new(0, 0, 166, 42);
        let expected_panes = app.compute_panes_area(area);
        let mut buf = Buffer::empty(area);
        app.render(area, &mut buf, Instant::now());
        assert_eq!(app.last_panes_area, expected_panes);

        // Sidebar hidden case also matches render
        app.sidebar_visible = false;
        let expected_no_sb = app.compute_panes_area(area);
        app.render(area, &mut buf, Instant::now());
        assert_eq!(app.last_panes_area, expected_no_sb);
        assert!(expected_no_sb.width > expected_panes.width);

        // Predict IntoPane matches pane_inner_size after placement
        let panel_id = app.active_panel_id().unwrap();
        let p1 = app.focused_pane.unwrap();
        let pred_into = app.predict_spawn_inner_size(&SpawnTarget::IntoPane {
            panel_id,
            pane_id: p1,
        });
        assert_eq!(pred_into, Some(app.pane_inner_size(p1)));

        // Predict SplitPane (Horizontal) matches post-placement pane_inner_size
        let split_target = SpawnTarget::SplitPane {
            panel_id,
            pane_id: p1,
            axis: SplitAxis::Horizontal,
        };
        let pred_split_h = app.predict_spawn_inner_size(&split_target);
        app.place_spawned_terminal("t-split-h", split_target);
        let new_pane_h = app.focused_pane.unwrap();
        assert_eq!(pred_split_h, Some(app.pane_inner_size(new_pane_h)));

        // Predict SplitPane (Vertical) matches post-placement pane_inner_size
        let split_v_target = SpawnTarget::SplitPane {
            panel_id,
            pane_id: new_pane_h,
            axis: SplitAxis::Vertical,
        };
        let pred_split_v = app.predict_spawn_inner_size(&split_v_target);
        app.place_spawned_terminal("t-split-v", split_v_target);
        let new_pane_v = app.focused_pane.unwrap();
        assert_eq!(pred_split_v, Some(app.pane_inner_size(new_pane_v)));

        // Predict NewPanel matches post-placement pane_inner_size
        let pred_new_panel = app.predict_spawn_inner_size(&SpawnTarget::NewPanel);
        app.place_spawned_terminal("t-new-panel", SpawnTarget::NewPanel);
        let new_panel_pane = app.focused_pane.unwrap();
        assert_eq!(pred_new_panel, Some(app.pane_inner_size(new_panel_pane)));
    }
}
