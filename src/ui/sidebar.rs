use std::collections::HashSet;

use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Widget};

use crate::hc::types::StatusKind;
use crate::model::tree::{MachineNode, TerminalEntry, UNASSIGNED_SPACE};
use crate::ui::theme::Theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SidebarPanel {
    #[default]
    Spaces,
    Agents,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SidebarItem {
    Machine {
        name: String,
        state: String,
    },
    Space {
        machine: String,
        name: String,
        count: usize,
    },
    Terminal {
        machine: String,
        space: String,
        address: String,
        label: String,
        glyph: &'static str,
        live: bool,
    },
    NewSpaceAction,
}

#[derive(Debug, Clone, Default)]
pub struct SidebarState {
    pub active_panel: SidebarPanel,
    pub selected: usize,
    pub agent_selected: usize,
    pub collapsed_machines: HashSet<String>,
    pub collapsed_spaces: HashSet<String>,
    pub visible_items: Vec<SidebarItem>,
    pub agent_items: Vec<SidebarItem>,
}

fn agent_priority(entry: &TerminalEntry) -> u8 {
    if !entry.terminal.is_live() {
        return 5;
    }
    match entry.terminal.status.as_ref().map(|s| s.kind_enum()) {
        Some(StatusKind::Blocked) | Some(StatusKind::Error) => 0,
        Some(StatusKind::Working) => 1,
        Some(StatusKind::Done) => 2,
        Some(StatusKind::Idle) => 3,
        Some(StatusKind::Other(_)) | None => 4,
    }
}

impl SidebarState {
    pub fn rebuild(&mut self, tree: &[MachineNode], active_space: Option<&str>) {
        self.visible_items.clear();
        self.agent_items.clear();
        self.visible_items.push(SidebarItem::NewSpaceAction);

        let mut agents: Vec<(u8, String, SidebarItem)> = Vec::new();
        for m in tree {
            self.visible_items.push(SidebarItem::Machine {
                name: m.name.clone(),
                state: m.state.clone(),
            });
            if self.collapsed_machines.contains(&m.name) {
                continue;
            }
            for sp in m.spaces.values() {
                let is_active_named = m.is_local
                    && active_space == Some(sp.name.as_str())
                    && sp.name != UNASSIGNED_SPACE;
                if sp.terminals.is_empty() && !is_active_named {
                    continue;
                }
                self.visible_items.push(SidebarItem::Space {
                    machine: m.name.clone(),
                    name: sp.name.clone(),
                    count: sp.terminals.len(),
                });
                for entry in &sp.terminals {
                    if !entry.is_agent() {
                        continue;
                    }
                    let prio = agent_priority(entry);
                    let addr = entry.terminal.address.clone();
                    agents.push((
                        prio,
                        addr.clone(),
                        SidebarItem::Terminal {
                            machine: m.name.clone(),
                            space: sp.name.clone(),
                            address: addr,
                            label: entry.display_name(),
                            glyph: entry.status_glyph(),
                            live: entry.terminal.is_live(),
                        },
                    ));
                }
            }
        }
        agents.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
        self.agent_items = agents.into_iter().map(|(_, _, item)| item).collect();

        if !self.visible_items.is_empty() && self.selected >= self.visible_items.len() {
            self.selected = self.visible_items.len() - 1;
        }
        if !self.agent_items.is_empty() && self.agent_selected >= self.agent_items.len() {
            self.agent_selected = self.agent_items.len() - 1;
        }
    }

    pub fn move_up(&mut self) {
        match self.active_panel {
            SidebarPanel::Spaces => {
                if self.selected > 0 {
                    self.selected -= 1;
                }
            }
            SidebarPanel::Agents => {
                if self.agent_selected > 0 {
                    self.agent_selected -= 1;
                } else if !self.visible_items.is_empty() {
                    self.active_panel = SidebarPanel::Spaces;
                    self.selected = self.visible_items.len() - 1;
                }
            }
        }
    }

    pub fn move_down(&mut self) {
        match self.active_panel {
            SidebarPanel::Spaces => {
                if self.selected + 1 < self.visible_items.len() {
                    self.selected += 1;
                } else if !self.agent_items.is_empty() {
                    self.active_panel = SidebarPanel::Agents;
                    self.agent_selected = 0;
                }
            }
            SidebarPanel::Agents => {
                if self.agent_selected + 1 < self.agent_items.len() {
                    self.agent_selected += 1;
                }
            }
        }
    }

    pub fn collapse_selected(&mut self) {
        if self.active_panel != SidebarPanel::Spaces {
            return;
        }
        if let Some(item) = self.visible_items.get(self.selected) {
            match item {
                SidebarItem::Machine { name, .. } => {
                    self.collapsed_machines.insert(name.clone());
                }
                SidebarItem::Space { machine, name, .. } => {
                    self.collapsed_spaces.insert(format!("{machine}/{name}"));
                }
                _ => {}
            }
        }
    }

    pub fn expand_selected(&mut self) {
        if self.active_panel != SidebarPanel::Spaces {
            return;
        }
        if let Some(item) = self.visible_items.get(self.selected) {
            match item {
                SidebarItem::Machine { name, .. } => {
                    self.collapsed_machines.remove(name);
                }
                SidebarItem::Space { machine, name, .. } => {
                    self.collapsed_spaces.remove(&format!("{machine}/{name}"));
                }
                _ => {}
            }
        }
    }

    pub fn toggle_selected(&mut self) -> Option<SidebarItem> {
        let item = self.selected_item()?.clone();
        if self.active_panel == SidebarPanel::Spaces {
            match &item {
                SidebarItem::Machine { name, .. } => {
                    if !self.collapsed_machines.remove(name) {
                        self.collapsed_machines.insert(name.clone());
                    }
                }
                SidebarItem::Space { machine, name, .. } => {
                    let key = format!("{machine}/{name}");
                    if !self.collapsed_spaces.remove(&key) {
                        self.collapsed_spaces.insert(key);
                    }
                }
                SidebarItem::Terminal { .. } | SidebarItem::NewSpaceAction => {}
            }
        }
        Some(item)
    }

    pub fn selected_item(&self) -> Option<&SidebarItem> {
        match self.active_panel {
            SidebarPanel::Spaces => self.visible_items.get(self.selected),
            SidebarPanel::Agents => self.agent_items.get(self.agent_selected),
        }
    }
}

pub fn render_sidebar(
    area: Rect,
    buf: &mut Buffer,
    state: &SidebarState,
    focused: bool,
) -> (Rect, Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
        .split(area);
    let spaces_area = chunks[0];
    let agents_area = chunks[1];

    render_spaces_panel(spaces_area, buf, state, focused);
    render_agents_panel(agents_area, buf, state, focused);
    (spaces_area, agents_area)
}

fn render_spaces_panel(area: Rect, buf: &mut Buffer, state: &SidebarState, focused: bool) {
    let border_style = if focused && state.active_panel == SidebarPanel::Spaces {
        Theme::border_focused()
    } else {
        Theme::border_normal()
    };
    let block = Block::default()
        .title(" Spaces ")
        .borders(Borders::ALL)
        .border_style(border_style);
    let inner = block.inner(area);
    block.render(area, buf);

    let items: Vec<ListItem> = state
        .visible_items
        .iter()
        .enumerate()
        .map(|(idx, item)| {
            let line = match item {
                SidebarItem::Machine { name, state: st } => {
                    let icon = if state.collapsed_machines.contains(name) {
                        "▸"
                    } else {
                        "▾"
                    };
                    let st_style = if st == "connected" {
                        Theme::machine_online()
                    } else {
                        Theme::machine_offline()
                    };
                    Line::from(vec![
                        Span::raw(format!("{icon} {name} ")),
                        Span::styled(format!("({st})"), st_style),
                    ])
                }
                SidebarItem::Space { name, count, .. } => Line::from(vec![
                    Span::styled(format!("  • {name} "), Theme::space_label()),
                    Span::styled(format!("({count})"), Theme::terminal_finished()),
                ]),
                SidebarItem::NewSpaceAction => {
                    Line::from(Span::styled("  [+ New Space]", Theme::new_space_action()))
                }
                SidebarItem::Terminal { .. } => Line::from(""),
            };
            let mut li = ListItem::new(line);
            if state.active_panel == SidebarPanel::Spaces && idx == state.selected {
                li = li.style(Theme::selected_row());
            }
            li
        })
        .collect();

    let list = List::new(items);
    Widget::render(list, inner, buf);
}

fn render_agents_panel(area: Rect, buf: &mut Buffer, state: &SidebarState, focused: bool) {
    let border_style = if focused && state.active_panel == SidebarPanel::Agents {
        Theme::border_focused()
    } else {
        Theme::border_normal()
    };
    let block = Block::default()
        .title(" Agents ")
        .borders(Borders::ALL)
        .border_style(border_style);
    let inner = block.inner(area);
    block.render(area, buf);

    let items: Vec<ListItem> = state
        .agent_items
        .iter()
        .enumerate()
        .map(|(idx, item)| {
            let line = match item {
                SidebarItem::Terminal {
                    space,
                    label,
                    glyph,
                    live,
                    ..
                } => {
                    let style = if *live {
                        Theme::terminal_live()
                    } else {
                        Theme::terminal_finished()
                    };
                    Line::from(vec![
                        Span::styled(format!(" {glyph} {label} "), style),
                        Span::styled(format!("[{space}]"), Theme::terminal_finished()),
                    ])
                }
                _ => Line::from(Span::styled("", Style::default())),
            };
            let mut li = ListItem::new(line);
            if state.active_panel == SidebarPanel::Agents && idx == state.agent_selected {
                li = li.style(Theme::selected_row());
            }
            li
        })
        .collect();

    let list = List::new(items);
    Widget::render(list, inner, buf);
}
