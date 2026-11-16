use std::collections::HashMap;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Widget};

use crate::layout::tree::{PaneId, SplitAxis, SplitNode, SplitTree};
use crate::term::session::{AttachSession, SessionState};
use crate::ui::selection::TextSelection;
use crate::ui::theme::Theme;

pub const TAKEOVER_LABEL: &str = " [Take over] ";
pub const SPAWN_PANE_LABEL: &str = " [+ New Terminal (Ctrl+B n)] ";
pub const UNATTACHED_PANE_LABEL: &str = " [connecting / unattached] ";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PaneHitbox {
    pub pane_id: PaneId,
    pub rect: Rect,
    pub takeover_btn: Option<Rect>,
    pub spawn_btn: Option<Rect>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DividerHitbox {
    pub target_pane: PaneId,
    pub axis: SplitAxis,
    pub container: Rect,
    pub line_pos: u16,
}

#[derive(Debug, Clone, Default)]
pub struct PaneLayoutGeometry {
    pub panes: Vec<PaneHitbox>,
    pub dividers: Vec<DividerHitbox>,
}

pub fn render_panes(
    area: Rect,
    buf: &mut Buffer,
    tree: &SplitTree,
    focused_pane: Option<PaneId>,
    sessions: &HashMap<String, AttachSession>,
    selection: Option<&TextSelection>,
) -> PaneLayoutGeometry {
    let mut geom = PaneLayoutGeometry::default();
    if let Some(root) = &tree.root {
        collect_dividers(root, area, &mut geom.dividers);
    }

    let rects = tree.compute_rects(area);
    for (pane_id, rect) in rects {
        let is_focused = focused_pane == Some(pane_id);
        let term_addr = tree.find_terminal(pane_id).map(|t| t.address.as_str());
        let session = term_addr.and_then(|a| sessions.get(a));

        let border_style = if is_focused {
            Theme::border_focused()
        } else {
            Theme::border_normal()
        };

        let mut title_spans = Vec::new();
        if term_addr.is_none() {
            title_spans.push(Span::raw(" (empty) "));
        } else if session.is_none() {
            title_spans.push(Span::raw(UNATTACHED_PANE_LABEL));
        }

        let mut takeover_rect = None;
        if let Some(sess) = session {
            match sess.state {
                SessionState::Active => {}
                SessionState::Observing => {
                    title_spans.push(Span::styled(" [observing] ", Theme::badge_observing()));
                    title_spans.push(Span::raw(" "));
                    let btn_w = TAKEOVER_LABEL.len() as u16;
                    if rect.width > btn_w + 4 {
                        let btn_x = rect.x + rect.width - btn_w - 1;
                        takeover_rect = Some(Rect::new(btn_x, rect.y, btn_w, 1));
                    }
                    title_spans.push(Span::styled(TAKEOVER_LABEL, Theme::takeover_button()));
                }
                SessionState::Ended => {
                    title_spans.push(Span::raw(" [ended] "));
                }
                SessionState::Connecting => {
                    title_spans.push(Span::raw(" [connecting] "));
                }
                SessionState::Error(_) => {
                    title_spans.push(Span::raw(" [error] "));
                }
            }
        }

        let block = Block::default()
            .title(Line::from(title_spans))
            .borders(Borders::ALL)
            .border_style(border_style);
        let inner = block.inner(rect);
        block.render(rect, buf);

        let mut spawn_rect = None;
        if let Some(sess) = session {
            sess.screen.render_to_buffer(inner, buf);
            if let Some(sel) = selection {
                if sel.pane_id == pane_id {
                    for dy in 0..inner.height {
                        for dx in 0..inner.width {
                            if sel.contains(dx, dy) {
                                if let Some(cell) = buf.cell_mut((inner.x + dx, inner.y + dy)) {
                                    cell.set_style(Theme::selection());
                                }
                            }
                        }
                    }
                }
            }
        } else if term_addr.is_none() {
            let lbl_w = (SPAWN_PANE_LABEL.len() as u16).min(inner.width);
            if inner.width >= 4 && inner.height >= 1 {
                let btn_x = inner.x + inner.width.saturating_sub(lbl_w) / 2;
                let btn_y = inner.y + inner.height / 2;
                let btn_area = Rect::new(btn_x, btn_y, lbl_w, 1);
                spawn_rect = Some(btn_area);
                Paragraph::new(Span::styled(SPAWN_PANE_LABEL, Theme::tab_new_terminal()))
                    .render(btn_area, buf);
            }
        } else if inner.width >= 4 && inner.height >= 1 {
            let lbl_w = (UNATTACHED_PANE_LABEL.len() as u16).min(inner.width);
            let lbl_x = inner.x + inner.width.saturating_sub(lbl_w) / 2;
            let lbl_y = inner.y + inner.height / 2;
            let lbl_area = Rect::new(lbl_x, lbl_y, lbl_w, 1);
            Paragraph::new(Span::styled(UNATTACHED_PANE_LABEL, Theme::border_normal()))
                .render(lbl_area, buf);
        }

        geom.panes.push(PaneHitbox {
            pane_id,
            rect,
            takeover_btn: takeover_rect,
            spawn_btn: spawn_rect,
        });
    }

    geom
}

fn collect_dividers(node: &SplitNode, area: Rect, out: &mut Vec<DividerHitbox>) {
    if let SplitNode::Split {
        axis,
        ratio,
        first,
        second,
    } = node
    {
        let r = ratio.clamp(0.1, 0.9);
        let first_pane = first_leaf_id(first);
        match axis {
            SplitAxis::Horizontal => {
                let w1 = ((area.width as f32) * r).round() as u16;
                let w1 = w1.clamp(1, area.width.saturating_sub(1).max(1));
                let w2 = area.width.saturating_sub(w1);
                if let Some(pid) = first_pane {
                    out.push(DividerHitbox {
                        target_pane: pid,
                        axis: SplitAxis::Horizontal,
                        container: area,
                        line_pos: area.x + w1,
                    });
                }
                collect_dividers(first, Rect::new(area.x, area.y, w1, area.height), out);
                collect_dividers(second, Rect::new(area.x + w1, area.y, w2, area.height), out);
            }
            SplitAxis::Vertical => {
                let h1 = ((area.height as f32) * r).round() as u16;
                let h1 = h1.clamp(1, area.height.saturating_sub(1).max(1));
                let h2 = area.height.saturating_sub(h1);
                if let Some(pid) = first_pane {
                    out.push(DividerHitbox {
                        target_pane: pid,
                        axis: SplitAxis::Vertical,
                        container: area,
                        line_pos: area.y + h1,
                    });
                }
                collect_dividers(first, Rect::new(area.x, area.y, area.width, h1), out);
                collect_dividers(second, Rect::new(area.x, area.y + h1, area.width, h2), out);
            }
        }
    }
}

fn first_leaf_id(node: &SplitNode) -> Option<PaneId> {
    match node {
        SplitNode::Leaf { id, .. } => Some(*id),
        SplitNode::Split { first, .. } => first_leaf_id(first),
    }
}
