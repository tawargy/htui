use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Direction, Layout, Rect};

use crate::config::Config;
use crate::hc::types::Terminal;
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
    RenameSpaceDialog, SpawnDialog,
};
use crate::ui::keymap::{Command, PrefixRouter, RouteResult};
use crate::ui::mouse::{handle_mouse_with_passthrough, MouseAction, MouseDragState};
use crate::ui::panes::{render_panes, PaneLayoutGeometry};
use crate::ui::selection::{extract_selected_text, TextSelection};
use crate::ui::sidebar::{render_sidebar, SidebarItem, SidebarPanel, SidebarState};
use crate::ui::statusbar::render_statusbar;
use crate::ui::tabs::{render_tab_bar, TabHitbox};

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
}

impl TuiApp {
    pub fn new(config: Config, initial_space: Option<String>) -> Self {
        let prefix_timeout = Duration::from_millis(config.ui.prefix_timeout_ms);
        let active_space = initial_space.unwrap_or_else(|| UNASSIGNED_SPACE.to_string());
        let mut layouts = LayoutDoc::default();
        let (tree, p1) = SplitTree::with_single(None);
        layouts.spaces.insert(active_space.clone(), tree);
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
        }
    }

    pub fn current_tree(&self) -> &SplitTree {
        self.layouts
            .spaces
            .get(&self.active_space)
            .unwrap_or_else(|| {
                static EMPTY: std::sync::OnceLock<SplitTree> = std::sync::OnceLock::new();
                EMPTY.get_or_init(SplitTree::new)
            })
    }

    pub fn current_tree_mut(&mut self) -> &mut SplitTree {
        let space = self.active_space.clone();
        self.layouts.spaces.entry(space).or_insert_with(|| {
            let (t, _) = SplitTree::with_single(None);
            t
        })
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
                target: _,
            } => {
                self.dialog = None;
                actions.push(AppAction::Spawn {
                    command_or_template,
                    task,
                    space,
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
                    if let Some(removed) = self.switch_space(trimmed.clone()) {
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

    pub fn render(&mut self, area: Rect, buf: &mut Buffer, now: Instant) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(3), Constraint::Length(1)])
            .split(area);

        let main_area = chunks[0];
        let status_area = chunks[1];

        let right_area =
            if self.sidebar_visible && main_area.width > self.config.ui.sidebar_width + 10 {
                let cols = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([
                        Constraint::Length(self.config.ui.sidebar_width),
                        Constraint::Min(10),
                    ])
                    .split(main_area);
                self.last_sidebar_areas = Some(render_sidebar(
                    cols[0],
                    buf,
                    &self.sidebar,
                    self.sidebar_focused,
                ));
                cols[1]
            } else {
                self.last_sidebar_areas = None;
                main_area
            };

        let right_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(3)])
            .split(right_area);
        let tab_area = right_chunks[0];
        let panes_area = right_chunks[1];

        let focused_addr = self.focused_terminal_addr();
        self.last_tabs_geom = render_tab_bar(
            tab_area,
            buf,
            &self.active_space,
            focused_addr.as_deref(),
            &self.model.terminals,
        );

        self.last_panes_area = panes_area;
        let tree = self
            .layouts
            .spaces
            .get(&self.active_space)
            .cloned()
            .unwrap_or_default();
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
                                        if let Some(removed) = self.switch_space(name.clone()) {
                                            actions.push(AppAction::RemoveLayout(removed));
                                        }
                                        self.refresh_sidebar();
                                        self.select_sidebar_space(&name);
                                    }
                                    SidebarItem::Terminal { address, space, .. } => {
                                        if let Some(removed) = self.switch_space(space) {
                                            actions.push(AppAction::RemoveLayout(removed));
                                        }
                                        self.refresh_sidebar();
                                        self.sidebar_focused = false;
                                        if let Some(pid) = self.focused_pane {
                                            self.current_tree_mut().set_terminal(
                                                pid,
                                                Some(TermRef {
                                                    address: address.clone(),
                                                }),
                                            );
                                            actions.push(AppAction::AttachTerminal {
                                                pane_id: pid,
                                                terminal: address,
                                            });
                                            actions.push(AppAction::SaveLayout(
                                                self.active_space.clone(),
                                            ));
                                        }
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
                    TabHitbox::Terminal { rect, .. } => rect,
                    TabHitbox::NewTerminal(rect) => rect,
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
                        self.dialog = Some(ActiveDialog::Spawn(SpawnDialog::new(
                            self.active_space.clone(),
                            self.templates.clone(),
                        )));
                    }
                    TabHitbox::Terminal { address, .. } => {
                        let existing_pid =
                            self.current_tree().pane_ids().into_iter().find(|&pid| {
                                self.current_tree()
                                    .find_terminal(pid)
                                    .map(|t| t.address.as_str())
                                    == Some(address.as_str())
                            });
                        if let Some(pid) = existing_pid {
                            self.focused_pane = Some(pid);
                        } else if let Some(pid) = self
                            .focused_pane
                            .or_else(|| self.current_tree().pane_ids().into_iter().next())
                        {
                            self.focused_pane = Some(pid);
                            self.current_tree_mut().set_terminal(
                                pid,
                                Some(TermRef {
                                    address: address.clone(),
                                }),
                            );
                            actions.push(AppAction::AttachTerminal {
                                pane_id: pid,
                                terminal: address,
                            });
                            actions.push(AppAction::SaveLayout(self.active_space.clone()));
                        }
                    }
                }
                self.dirty = true;
                return actions;
            }
        }
        let shift_held = mouse.modifiers.contains(KeyModifiers::SHIFT);
        let tree = self.layouts.spaces.get(&self.active_space);
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
                            if let Some(removed) = self.switch_space(space) {
                                actions.push(AppAction::RemoveLayout(removed));
                            }
                            self.refresh_sidebar();
                            self.sidebar_focused = false;
                            if let Some(pid) = self.focused_pane {
                                self.current_tree_mut().set_terminal(
                                    pid,
                                    Some(TermRef {
                                        address: address.clone(),
                                    }),
                                );
                                actions.push(AppAction::AttachTerminal {
                                    pane_id: pid,
                                    terminal: address,
                                });
                            }
                        }
                        Some(SidebarItem::Space { name, .. }) => {
                            if let Some(removed) = self.switch_space(name.clone()) {
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
                self.focused_pane = Some(pid);
                self.sidebar_focused = false;
                self.selection = None;
                self.dirty = true;
            }
            MouseAction::StartSelect { pane_id, start } => {
                self.focused_pane = Some(pane_id);
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
                self.focused_pane = Some(pid);
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
                self.focused_pane = Some(pid);
                self.sidebar_focused = false;
                let sp = self.active_space.clone();
                let tpls = self.templates.clone();
                self.dialog = Some(ActiveDialog::Spawn(SpawnDialog::new(sp, tpls)));
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
                        }
                    }
                }
                self.dirty = true;
            }
            MouseAction::EndDrag => {
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
                    self.focused_pane = Some(pane_id);
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
                    .is_some_and(|t| !t.has_terminals())
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
        self.active_space = space.clone();
        let entry = self.layouts.spaces.entry(space).or_insert_with(|| {
            let (t, _) = SplitTree::with_single(None);
            t
        });
        self.focused_pane = entry.pane_ids().into_iter().next();
        self.dirty = true;
        removed_prev
    }

    fn apply_command(&mut self, cmd: Command, now: Instant, actions: &mut Vec<AppAction>) {
        match cmd {
            Command::SplitHorizontal | Command::SplitVertical => {
                let axis = if cmd == Command::SplitHorizontal {
                    SplitAxis::Horizontal
                } else {
                    SplitAxis::Vertical
                };
                if let Some(pid) = self.focused_pane {
                    if let Some(new_id) = self.current_tree_mut().split(pid, axis, None) {
                        self.focused_pane = Some(new_id);
                        actions.push(AppAction::SaveLayout(self.active_space.clone()));
                    }
                }
            }
            Command::ClosePane => {
                if let Some(pid) = self.focused_pane {
                    let closed_addr = self
                        .current_tree()
                        .find_terminal(pid)
                        .map(|t| t.address.clone());
                    let had_multi = self.current_tree().pane_ids().len() > 1;
                    if let Some(addr) = closed_addr.as_deref() {
                        self.current_tree_mut().set_terminal(pid, None);
                        self.kill_or_remove_terminal(addr, actions);
                    }
                    if had_multi {
                        self.current_tree_mut().close(pid);
                        self.focused_pane = self.current_tree().pane_ids().into_iter().next();
                    }
                    let cur_space = self.active_space.clone();
                    if cur_space != UNASSIGNED_SPACE {
                        if !had_multi {
                            self.close_space_terminals(&cur_space, actions);
                            self.remove_empty_space(&cur_space);
                            actions.push(AppAction::RemoveLayout(cur_space));
                        } else if !self.space_has_open_terminals(&cur_space) {
                            self.remove_empty_space(&cur_space);
                            actions.push(AppAction::RemoveLayout(cur_space));
                        } else {
                            actions.push(AppAction::SaveLayout(cur_space));
                        }
                    } else if !had_multi {
                        let extra_removed = self.close_space_terminals(UNASSIGNED_SPACE, actions);
                        if let Some(next_space) = self.next_named_space() {
                            self.layouts.spaces.remove(UNASSIGNED_SPACE);
                            actions.push(AppAction::RemoveLayout(UNASSIGNED_SPACE.to_string()));
                            self.switch_space(next_space);
                        } else if closed_addr.is_some() || extra_removed > 0 {
                            actions.push(AppAction::RemoveLayout(UNASSIGNED_SPACE.to_string()));
                        } else {
                            self.set_toast("Workspace is already empty", now);
                        }
                    } else if !self.current_tree().has_terminals() {
                        if self.current_tree_mut().collapse_empty_splits() {
                            self.focused_pane = self.current_tree().pane_ids().into_iter().next();
                        }
                        actions.push(AppAction::RemoveLayout(cur_space));
                    } else {
                        actions.push(AppAction::SaveLayout(cur_space));
                    }
                    self.refresh_sidebar();
                }
            }
            Command::FocusDir(dir) => {
                if let Some(pid) = self.focused_pane {
                    if let Some(next) =
                        self.current_tree()
                            .focus_dir(self.last_panes_area, pid, dir)
                    {
                        self.focused_pane = Some(next);
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
                let sp = self.active_space.clone();
                let tpls = self.templates.clone();
                self.dialog = Some(ActiveDialog::Spawn(SpawnDialog::new(sp, tpls)));
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
            Command::SplitSpawnHorizontal
            | Command::SplitSpawnVertical
            | Command::PrevPanel
            | Command::NextPanel => {}
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

    pub fn sync_pane_sizes(&mut self, now: Instant) {
        let targets: Vec<(PaneId, u16, u16)> = self
            .last_geom
            .panes
            .iter()
            .map(|p| {
                (
                    p.pane_id,
                    p.rect.width.saturating_sub(2).max(1),
                    p.rect.height.saturating_sub(2).max(1),
                )
            })
            .collect();
        for (pid, cols, rows) in targets {
            if let Some(addr) = self
                .current_tree()
                .find_terminal(pid)
                .map(|t| t.address.clone())
            {
                if let Some(sess) = self.sessions.get_mut(&addr) {
                    if sess.screen.size() != (cols, rows) {
                        let _ = sess.request_resize(cols, rows, now);
                    }
                }
            }
        }
    }

    pub fn ensure_session_stub(&mut self, addr: &str) {
        if !self.sessions.contains_key(addr) {
            let mut sess = AttachSession::new(addr.to_string(), 80, 24);
            sess.state = SessionState::Connecting;
            self.sessions.insert(addr.to_string(), sess);
        }
    }

    pub fn space_has_open_terminals(&self, space: &str) -> bool {
        let has_pane_term = self.layouts.spaces.get(space).is_some_and(|tree| {
            tree.pane_ids()
                .into_iter()
                .any(|pid| tree.find_terminal(pid).is_some())
        });
        if has_pane_term {
            return true;
        }
        self.model
            .terminals
            .values()
            .any(|t| t.is_live() && t.labels.get("space").map(String::as_str) == Some(space))
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

    /// Removes every space without terminal panels (including `UNASSIGNED_SPACE`)
    /// from `layouts.spaces` and `model.declared_spaces`, returning the removed keys.
    pub fn cleanup_empty_spaces_on_quit(&mut self) -> HashSet<String> {
        let mut removed: HashSet<String> = self
            .layouts
            .spaces
            .iter()
            .filter(|(_, tree)| !tree.has_terminals())
            .map(|(sp, _)| sp.clone())
            .collect();
        for sp in &self.model.declared_spaces {
            if !self
                .layouts
                .spaces
                .get(sp)
                .is_some_and(SplitTree::has_terminals)
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

        for (sp, tree) in &mut self.layouts.spaces {
            let mut pruned = tree.prune_removed(&present);
            for pid in tree.pane_ids() {
                if let Some(term_ref) = tree.find_terminal(pid) {
                    if let Some(term) = self.model.terminals.get(&term_ref.address) {
                        let term_space = term
                            .labels
                            .get("space")
                            .map(String::as_str)
                            .filter(|s| !s.is_empty())
                            .unwrap_or(UNASSIGNED_SPACE);
                        if term_space != sp.as_str() {
                            tree.set_terminal(pid, None);
                            pruned += 1;
                        }
                    }
                }
            }
            if !tree.has_terminals()
                && (pruned > 0 || sp != &initial_active)
                && tree.collapse_empty_splits()
                && sp == &initial_active
            {
                self.focused_pane = tree.pane_ids().into_iter().next();
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
                    .is_some_and(|t| !t.has_terminals());
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
        for (sp, tree) in &mut self.layouts.spaces {
            let pids = tree.pane_ids();
            let mut changed = false;
            for pid in pids {
                if tree.find_terminal(pid).map(|t| t.address.as_str()) == Some(addr) {
                    tree.set_terminal(pid, None);
                    affected.insert(sp.clone());
                    changed = true;
                }
            }
            if changed && tree.collapse_empty_splits() && sp == &active_space {
                self.focused_pane = tree.pane_ids().into_iter().next();
            }
        }
        let mut emptied: Vec<String> = affected
            .into_iter()
            .filter(|sp| {
                if sp == UNASSIGNED_SPACE {
                    self.layouts
                        .spaces
                        .get(UNASSIGNED_SPACE)
                        .is_some_and(|t| !t.has_terminals())
                } else {
                    let has_pane_term = self
                        .layouts
                        .spaces
                        .get(sp)
                        .is_some_and(SplitTree::has_terminals);
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
}
