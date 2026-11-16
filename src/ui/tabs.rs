use std::collections::{HashMap, HashSet};

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use unicode_width::UnicodeWidthStr;

use crate::hc::types::Terminal;
use crate::layout::panel::{PanelId, SpaceLayout};
use crate::model::tree::TerminalEntry;
use crate::ui::theme::Theme;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TabHitbox {
    Panel { panel_id: PanelId, rect: Rect },
    UnattachedTerminal { address: String, rect: Rect },
    SplitHorizontal(Rect),
    SplitVertical(Rect),
    NewTerminal(Rect),
}

const SPLIT_H_LABEL: &str = " [+H] ";
const SPLIT_V_LABEL: &str = " [+V] ";
const NEW_TERMINAL_TAB_LABEL: &str = " [+ New Terminal] ";

pub fn render_tab_bar(
    area: Rect,
    buf: &mut Buffer,
    space_layout: Option<&SpaceLayout>,
    active_space: &str,
    terminals: &HashMap<String, Terminal>,
) -> Vec<TabHitbox> {
    let mut hitboxes = Vec::new();
    if area.width == 0 || area.height == 0 {
        return hitboxes;
    }

    let bg_style = Theme::tab_bar_bg();
    for x in area.x..area.x + area.width {
        if let Some(cell) = buf.cell_mut((x, area.y)) {
            cell.set_symbol(" ");
            cell.set_style(bg_style);
        }
    }

    let new_w = (NEW_TERMINAL_TAB_LABEL.width() as u16).min(area.width);
    let split_h_w = SPLIT_H_LABEL.width() as u16;
    let split_v_w = SPLIT_V_LABEL.width() as u16;
    let all_actions_w = split_h_w + 1 + split_v_w + 1 + new_w;
    let show_split_buttons = area.width >= all_actions_w;

    let new_x = (area.x + area.width).saturating_sub(new_w);
    let tabs_end = if show_split_buttons {
        (area.x + area.width).saturating_sub(all_actions_w)
    } else {
        new_x
    };

    let mut placed_addrs: HashSet<String> = HashSet::new();
    let mut x = area.x;

    if let Some(space) = space_layout {
        let active_id = space
            .active_panel
            .or_else(|| space.panels.first().map(|p| p.id));
        for panel in &space.panels {
            for addr in panel.terminal_addresses() {
                placed_addrs.insert(addr);
            }
            if x >= tabs_end {
                continue;
            }
            let pane_count = panel.tree.pane_ids().len();
            let extra = if pane_count > 1 {
                format!(" (+{})", pane_count - 1)
            } else {
                String::new()
            };
            let label = match panel
                .primary_terminal()
                .map(|tr| tr.address.as_str())
            {
                Some(addr) => match terminals.get(addr) {
                    Some(term) => {
                        let entry = TerminalEntry {
                            terminal: term.clone(),
                        };
                        format!(" {} {}{} ", entry.status_glyph(), entry.display_name(), extra)
                    }
                    None => format!(" {}{} ", addr, extra),
                },
                None => format!(" (empty){} ", extra),
            };

            let avail = tabs_end.saturating_sub(x);
            let w = (label.width() as u16).min(avail);
            if w == 0 {
                continue;
            }
            let style = if active_id == Some(panel.id) {
                Theme::tab_active()
            } else {
                Theme::tab_inactive()
            };
            buf.set_stringn(x, area.y, &label, w as usize, style);
            hitboxes.push(TabHitbox::Panel {
                panel_id: panel.id,
                rect: Rect::new(x, area.y, w, 1),
            });
            x = x.saturating_add(w);
            if x < tabs_end {
                buf.set_stringn(x, area.y, "│", 1, bg_style);
                x = x.saturating_add(1);
            }
        }
    }

    let mut unattached_terms: Vec<&Terminal> = terminals
        .values()
        .filter(|t| {
            let entry = TerminalEntry {
                terminal: (*t).clone(),
            };
            entry.space_name() == active_space && !placed_addrs.contains(&t.address)
        })
        .collect();
    unattached_terms.sort_by(|a, b| {
        a.created_ms
            .cmp(&b.created_ms)
            .then(a.address.cmp(&b.address))
    });

    for term in unattached_terms {
        if x >= tabs_end {
            break;
        }
        let entry = TerminalEntry {
            terminal: term.clone(),
        };
        let label = format!(" {} {} ", entry.status_glyph(), entry.display_name());
        let avail = tabs_end.saturating_sub(x);
        let w = (label.width() as u16).min(avail);
        if w == 0 {
            break;
        }
        buf.set_stringn(x, area.y, &label, w as usize, Theme::tab_inactive());
        hitboxes.push(TabHitbox::UnattachedTerminal {
            address: term.address.clone(),
            rect: Rect::new(x, area.y, w, 1),
        });
        x = x.saturating_add(w);
        if x < tabs_end {
            buf.set_stringn(x, area.y, "│", 1, bg_style);
            x = x.saturating_add(1);
        }
    }

    let btn_style = Theme::tab_new_terminal();
    if show_split_buttons {
        let split_h_x = tabs_end;
        let split_v_x = split_h_x + split_h_w + 1;
        buf.set_stringn(
            split_h_x,
            area.y,
            SPLIT_H_LABEL,
            split_h_w as usize,
            btn_style,
        );
        hitboxes.push(TabHitbox::SplitHorizontal(Rect::new(
            split_h_x, area.y, split_h_w, 1,
        )));
        buf.set_stringn(
            split_v_x,
            area.y,
            SPLIT_V_LABEL,
            split_v_w as usize,
            btn_style,
        );
        hitboxes.push(TabHitbox::SplitVertical(Rect::new(
            split_v_x, area.y, split_v_w, 1,
        )));
    }

    buf.set_stringn(
        new_x,
        area.y,
        NEW_TERMINAL_TAB_LABEL,
        new_w as usize,
        btn_style,
    );
    hitboxes.push(TabHitbox::NewTerminal(Rect::new(new_x, area.y, new_w, 1)));

    hitboxes
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::tree::{SplitAxis, TermRef};
    use ratatui::style::Color;

    fn make_term(addr: &str, cmd: &str, space: &str, created_ms: u64) -> Terminal {
        let mut t = Terminal {
            address: addr.into(),
            state: "live".into(),
            argv: vec![cmd.into()],
            created_ms,
            ..Default::default()
        };
        t.labels.insert("space".into(), space.into());
        t
    }

    #[test]
    fn renders_panel_pills_multipane_count_unattached_and_action_buttons() {
        let mut terminals = HashMap::new();
        terminals.insert("t1".into(), make_term("t1", "zsh", "api", 1));
        terminals.insert("t2".into(), make_term("t2", "claude", "api", 2));
        terminals.insert("t3".into(), make_term("t3", "bash", "api", 3));

        let (mut space, p1_id, p1_pane) = SpaceLayout::with_single(Some(TermRef {
            address: "t1".into(),
        }));
        space
            .panel_mut(p1_id)
            .unwrap()
            .tree
            .split(
                p1_pane,
                SplitAxis::Horizontal,
                Some(TermRef {
                    address: "t2".into(),
                }),
            )
            .unwrap();
        let (p2_id, _) = space.add_panel(None);
        space.set_active_panel(p1_id);

        let area = Rect::new(0, 0, 100, 1);
        let mut buf = Buffer::empty(area);
        let hits = render_tab_bar(area, &mut buf, Some(&space), "api", &terminals);
        let row: String = buf.content().iter().map(|c| c.symbol()).collect();

        assert!(row.contains("zsh (+1)"));
        assert!(row.contains("(empty)"));
        assert!(row.contains("bash"));
        assert!(row.contains("[+H]"));
        assert!(row.contains("[+V]"));
        assert!(row.contains("[+ New Terminal]"));

        assert!(hits
            .iter()
            .any(|h| matches!(h, TabHitbox::Panel { panel_id, .. } if *panel_id == p1_id)));
        assert!(hits
            .iter()
            .any(|h| matches!(h, TabHitbox::Panel { panel_id, .. } if *panel_id == p2_id)));
        assert!(hits.iter().any(
            |h| matches!(h, TabHitbox::UnattachedTerminal { address, .. } if address == "t3")
        ));

        let split_h = hits
            .iter()
            .find_map(|h| match h {
                TabHitbox::SplitHorizontal(r) => Some(*r),
                _ => None,
            })
            .unwrap();
        let split_v = hits
            .iter()
            .find_map(|h| match h {
                TabHitbox::SplitVertical(r) => Some(*r),
                _ => None,
            })
            .unwrap();
        let new_btn = hits
            .iter()
            .find_map(|h| match h {
                TabHitbox::NewTerminal(r) => Some(*r),
                _ => None,
            })
            .unwrap();
        assert_eq!(split_h.x + split_h.width + 1, split_v.x);
        assert_eq!(split_v.x + split_v.width + 1, new_btn.x);
        assert_eq!(buf.cell((split_v.x - 1, 0)).unwrap().bg, Color::Reset);
        assert_eq!(buf.cell((new_btn.x - 1, 0)).unwrap().bg, Color::Reset);
    }

    #[test]
    fn narrow_tab_bar_prioritizes_new_terminal_button() {
        let terminals = HashMap::new();
        let space = SpaceLayout::new();
        let area = Rect::new(0, 0, 24, 1);
        let mut buf = Buffer::empty(area);
        let hits = render_tab_bar(area, &mut buf, Some(&space), "api", &terminals);
        assert!(hits
            .iter()
            .any(|h| matches!(h, TabHitbox::NewTerminal(_))));
        assert!(!hits
            .iter()
            .any(|h| matches!(h, TabHitbox::SplitHorizontal(_))));
        assert!(!hits
            .iter()
            .any(|h| matches!(h, TabHitbox::SplitVertical(_))));
    }
}
