use std::collections::HashMap;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use unicode_width::UnicodeWidthStr;

use crate::hc::types::Terminal;
use crate::model::tree::TerminalEntry;
use crate::ui::theme::Theme;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TabHitbox {
    Terminal { address: String, rect: Rect },
    NewTerminal(Rect),
}

const NEW_TERMINAL_TAB_LABEL: &str = " [+ New Terminal] ";

pub fn render_tab_bar(
    area: Rect,
    buf: &mut Buffer,
    active_space: &str,
    focused_addr: Option<&str>,
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

    let mut space_terms: Vec<&Terminal> = terminals
        .values()
        .filter(|t| {
            let entry = TerminalEntry {
                terminal: (*t).clone(),
            };
            entry.space_name() == active_space
        })
        .collect();
    space_terms.sort_by(|a, b| {
        a.created_ms
            .cmp(&b.created_ms)
            .then(a.address.cmp(&b.address))
    });

    let btn_w = (NEW_TERMINAL_TAB_LABEL.width() as u16).min(area.width);
    let btn_x = (area.x + area.width).saturating_sub(btn_w);
    let tabs_end = btn_x;

    let mut x = area.x;
    for term in space_terms {
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
        let is_focused = focused_addr == Some(term.address.as_str());
        let style = if is_focused {
            Theme::tab_active()
        } else {
            Theme::tab_inactive()
        };
        buf.set_stringn(x, area.y, &label, w as usize, style);
        hitboxes.push(TabHitbox::Terminal {
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
    buf.set_stringn(
        btn_x,
        area.y,
        NEW_TERMINAL_TAB_LABEL,
        btn_w as usize,
        btn_style,
    );
    hitboxes.push(TabHitbox::NewTerminal(Rect::new(btn_x, area.y, btn_w, 1)));

    hitboxes
}
