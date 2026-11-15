use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

use crate::layout::tree::{PaneId, SplitAxis};
use crate::ui::panes::{DividerHitbox, PaneLayoutGeometry};
use crate::ui::sidebar::SidebarPanel;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MouseAction {
    SelectSidebarRow {
        panel: SidebarPanel,
        idx: usize,
    },
    FocusPane(PaneId),
    TakeOverPane(PaneId),
    SpawnInPane(PaneId),
    StartSelect {
        pane_id: PaneId,
        start: (u16, u16),
    },
    UpdateSelect {
        pane_id: PaneId,
        end: (u16, u16),
    },
    EndSelect {
        pane_id: PaneId,
        start: (u16, u16),
        end: (u16, u16),
    },
    StartDragDivider(DividerHitbox),
    DragDivider {
        target_pane: PaneId,
        axis: SplitAxis,
        new_ratio: f32,
    },
    EndDrag,
    ScrollUp(PaneId),
    ScrollDown(PaneId),
    PaneMouse {
        pane_id: PaneId,
        rel_col: u16,
        rel_row: u16,
    },
    None,
}

#[derive(Debug, Clone, Default)]
pub struct MouseDragState {
    pub active_divider: Option<DividerHitbox>,
    pub active_selection: Option<(PaneId, Rect, (u16, u16))>,
    pub active_passthrough_pane: Option<(PaneId, Rect)>,
}

pub fn handle_mouse(
    event: MouseEvent,
    sidebar_areas: Option<(Rect, Rect)>,
    geom: &PaneLayoutGeometry,
    drag: &mut MouseDragState,
) -> MouseAction {
    handle_mouse_with_passthrough(event, sidebar_areas, geom, drag, |_| false)
}

pub fn handle_mouse_with_passthrough<F>(
    event: MouseEvent,
    sidebar_areas: Option<(Rect, Rect)>,
    geom: &PaneLayoutGeometry,
    drag: &mut MouseDragState,
    should_passthrough: F,
) -> MouseAction
where
    F: Fn(PaneId) -> bool,
{
    let col = event.column;
    let row = event.row;

    match event.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            // 1. Check split dividers (tolerance +/- 1 cell for VTE border alignment)
            for div in &geom.dividers {
                if hit_divider(div, col, row) {
                    drag.active_passthrough_pane = None;
                    drag.active_divider = Some(*div);
                    return MouseAction::StartDragDivider(*div);
                }
            }

            // 2. Check sidebar (Spaces & Agents panels)
            if let Some((spaces_area, agents_area)) = sidebar_areas {
                for (area, panel) in [
                    (spaces_area, SidebarPanel::Spaces),
                    (agents_area, SidebarPanel::Agents),
                ] {
                    if rect_contains(area, col, row) {
                        drag.active_passthrough_pane = None;
                        let inner = pane_inner(area);
                        if inner.width > 0 && inner.height > 0 && rect_contains(inner, col, row) {
                            return MouseAction::SelectSidebarRow {
                                panel,
                                idx: (row - inner.y) as usize,
                            };
                        }
                        return MouseAction::None;
                    }
                }
            }

            // 3. Check pane takeover / spawn buttons & pane focus
            for pane in &geom.panes {
                if let Some(btn) = pane.takeover_btn {
                    if rect_contains(btn, col, row) {
                        drag.active_passthrough_pane = None;
                        return MouseAction::TakeOverPane(pane.pane_id);
                    }
                }
                if let Some(btn) = pane.spawn_btn {
                    if rect_contains(btn, col, row) {
                        drag.active_passthrough_pane = None;
                        return MouseAction::SpawnInPane(pane.pane_id);
                    }
                }
                let inner = pane_inner(pane.rect);
                if inner.width > 0 && inner.height > 0 && rect_contains(inner, col, row) {
                    if should_passthrough(pane.pane_id) {
                        drag.active_passthrough_pane = Some((pane.pane_id, inner));
                        return MouseAction::PaneMouse {
                            pane_id: pane.pane_id,
                            rel_col: col - inner.x,
                            rel_row: row - inner.y,
                        };
                    }
                    drag.active_passthrough_pane = None;
                    let start = (col - inner.x, row - inner.y);
                    drag.active_selection = Some((pane.pane_id, inner, start));
                    return MouseAction::StartSelect {
                        pane_id: pane.pane_id,
                        start,
                    };
                }
                if rect_contains(pane.rect, col, row) {
                    drag.active_passthrough_pane = None;
                    return MouseAction::FocusPane(pane.pane_id);
                }
            }
            drag.active_passthrough_pane = None;
            MouseAction::None
        }
        MouseEventKind::Down(MouseButton::Middle | MouseButton::Right) => {
            if let Some((pane_id, inner, rel_col, rel_row)) =
                find_passthrough_pane(geom, col, row, &should_passthrough)
            {
                drag.active_passthrough_pane = Some((pane_id, inner));
                return MouseAction::PaneMouse {
                    pane_id,
                    rel_col,
                    rel_row,
                };
            }
            MouseAction::None
        }
        MouseEventKind::Drag(btn) => {
            if btn == MouseButton::Left {
                if let Some(div) = drag.active_divider {
                    let ratio = match div.axis {
                        SplitAxis::Horizontal => {
                            let rel = col.saturating_sub(div.container.x) as f32;
                            rel / (div.container.width.max(1) as f32)
                        }
                        SplitAxis::Vertical => {
                            let rel = row.saturating_sub(div.container.y) as f32;
                            rel / (div.container.height.max(1) as f32)
                        }
                    };
                    return MouseAction::DragDivider {
                        target_pane: div.target_pane,
                        axis: div.axis,
                        new_ratio: ratio.clamp(0.1, 0.9),
                    };
                }
                if let Some((pane_id, inner, _start)) = drag.active_selection {
                    let (rel_col, rel_row) = clamp_rel(inner, col, row);
                    return MouseAction::UpdateSelect {
                        pane_id,
                        end: (rel_col, rel_row),
                    };
                }
            }
            if let Some((pane_id, inner)) = drag.active_passthrough_pane {
                let (rel_col, rel_row) = clamp_rel(inner, col, row);
                return MouseAction::PaneMouse {
                    pane_id,
                    rel_col,
                    rel_row,
                };
            }
            if let Some((pane_id, _inner, rel_col, rel_row)) =
                find_passthrough_pane(geom, col, row, &should_passthrough)
            {
                return MouseAction::PaneMouse {
                    pane_id,
                    rel_col,
                    rel_row,
                };
            }
            MouseAction::None
        }
        MouseEventKind::Up(btn) => {
            if btn == MouseButton::Left {
                if drag.active_divider.take().is_some() {
                    return MouseAction::EndDrag;
                }
                if let Some((pane_id, inner, start)) = drag.active_selection.take() {
                    let (rel_col, rel_row) = clamp_rel(inner, col, row);
                    return MouseAction::EndSelect {
                        pane_id,
                        start,
                        end: (rel_col, rel_row),
                    };
                }
            }
            if let Some((pane_id, inner)) = drag.active_passthrough_pane.take() {
                let (rel_col, rel_row) = clamp_rel(inner, col, row);
                return MouseAction::PaneMouse {
                    pane_id,
                    rel_col,
                    rel_row,
                };
            }
            if let Some((pane_id, _inner, rel_col, rel_row)) =
                find_passthrough_pane(geom, col, row, &should_passthrough)
            {
                return MouseAction::PaneMouse {
                    pane_id,
                    rel_col,
                    rel_row,
                };
            }
            MouseAction::None
        }
        MouseEventKind::Moved | MouseEventKind::ScrollLeft | MouseEventKind::ScrollRight => {
            if let Some((pane_id, _inner, rel_col, rel_row)) =
                find_passthrough_pane(geom, col, row, &should_passthrough)
            {
                return MouseAction::PaneMouse {
                    pane_id,
                    rel_col,
                    rel_row,
                };
            }
            MouseAction::None
        }
        MouseEventKind::ScrollUp => {
            if let Some((pane_id, _inner, rel_col, rel_row)) =
                find_passthrough_pane(geom, col, row, &should_passthrough)
            {
                return MouseAction::PaneMouse {
                    pane_id,
                    rel_col,
                    rel_row,
                };
            }
            for pane in &geom.panes {
                if rect_contains(pane.rect, col, row) {
                    return MouseAction::ScrollUp(pane.pane_id);
                }
            }
            MouseAction::None
        }
        MouseEventKind::ScrollDown => {
            if let Some((pane_id, _inner, rel_col, rel_row)) =
                find_passthrough_pane(geom, col, row, &should_passthrough)
            {
                return MouseAction::PaneMouse {
                    pane_id,
                    rel_col,
                    rel_row,
                };
            }
            for pane in &geom.panes {
                if rect_contains(pane.rect, col, row) {
                    return MouseAction::ScrollDown(pane.pane_id);
                }
            }
            MouseAction::None
        }
    }
}

fn pane_inner(rect: Rect) -> Rect {
    Rect::new(
        rect.x + 1,
        rect.y + 1,
        rect.width.saturating_sub(2),
        rect.height.saturating_sub(2),
    )
}

fn clamp_rel(inner: Rect, col: u16, row: u16) -> (u16, u16) {
    let cx = col.clamp(inner.x, inner.x + inner.width.saturating_sub(1));
    let cy = row.clamp(inner.y, inner.y + inner.height.saturating_sub(1));
    (cx - inner.x, cy - inner.y)
}

fn find_passthrough_pane<F>(
    geom: &PaneLayoutGeometry,
    col: u16,
    row: u16,
    should_passthrough: &F,
) -> Option<(PaneId, Rect, u16, u16)>
where
    F: Fn(PaneId) -> bool,
{
    for pane in &geom.panes {
        let inner = pane_inner(pane.rect);
        if inner.width > 0
            && inner.height > 0
            && rect_contains(inner, col, row)
            && should_passthrough(pane.pane_id)
        {
            return Some((pane.pane_id, inner, col - inner.x, row - inner.y));
        }
    }
    None
}

fn rect_contains(r: Rect, col: u16, row: u16) -> bool {
    col >= r.x && col < r.x + r.width && row >= r.y && row < r.y + r.height
}

fn hit_divider(div: &DividerHitbox, col: u16, row: u16) -> bool {
    if !rect_contains(div.container, col, row) {
        return false;
    }
    match div.axis {
        SplitAxis::Horizontal => col.abs_diff(div.line_pos) <= 1,
        SplitAxis::Vertical => row.abs_diff(div.line_pos) <= 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::panes::PaneHitbox;
    use crossterm::event::KeyModifiers;

    fn make_mouse(kind: MouseEventKind, column: u16, row: u16) -> MouseEvent {
        MouseEvent {
            kind,
            column,
            row,
            modifiers: KeyModifiers::NONE,
        }
    }

    fn two_pane_geom() -> PaneLayoutGeometry {
        PaneLayoutGeometry {
            panes: vec![
                PaneHitbox {
                    pane_id: 1,
                    rect: Rect::new(20, 0, 40, 12),
                    takeover_btn: Some(Rect::new(25, 0, 13, 1)),
                    spawn_btn: Some(Rect::new(25, 5, 20, 1)),
                },
                PaneHitbox {
                    pane_id: 2,
                    rect: Rect::new(60, 0, 40, 12),
                    takeover_btn: None,
                    spawn_btn: None,
                },
            ],
            dividers: vec![DividerHitbox {
                target_pane: 1,
                axis: SplitAxis::Horizontal,
                container: Rect::new(20, 0, 80, 12),
                line_pos: 60,
            }],
        }
    }

    #[test]
    fn dividers_sidebar_and_buttons_take_precedence_over_passthrough() {
        let geom = two_pane_geom();
        let sidebar = Some((Rect::new(0, 0, 20, 6), Rect::new(0, 6, 20, 6)));
        let mut drag = MouseDragState::default();

        // Divider (+/- 1 cell overlaps col 61, which is pane 2's left border/inner)
        assert!(matches!(
            handle_mouse_with_passthrough(
                make_mouse(MouseEventKind::Down(MouseButton::Left), 60, 5),
                sidebar,
                &geom,
                &mut drag,
                |_| true,
            ),
            MouseAction::StartDragDivider(_)
        ));
        drag.active_divider = None;

        // Sidebar Spaces
        assert_eq!(
            handle_mouse_with_passthrough(
                make_mouse(MouseEventKind::Down(MouseButton::Left), 5, 2),
                sidebar,
                &geom,
                &mut drag,
                |_| true,
            ),
            MouseAction::SelectSidebarRow {
                panel: SidebarPanel::Spaces,
                idx: 1,
            }
        );

        // Takeover button on pane 1
        assert_eq!(
            handle_mouse_with_passthrough(
                make_mouse(MouseEventKind::Down(MouseButton::Left), 26, 0),
                sidebar,
                &geom,
                &mut drag,
                |_| true,
            ),
            MouseAction::TakeOverPane(1)
        );

        // Spawn button on pane 1 (even inside inner area)
        assert_eq!(
            handle_mouse_with_passthrough(
                make_mouse(MouseEventKind::Down(MouseButton::Left), 26, 5),
                sidebar,
                &geom,
                &mut drag,
                |_| true,
            ),
            MouseAction::SpawnInPane(1)
        );
    }

    #[test]
    fn passthrough_down_drag_up_clamps_to_inner_and_clears_on_up() {
        let geom = two_pane_geom();
        let mut drag = MouseDragState::default();

        for btn in [MouseButton::Left, MouseButton::Middle, MouseButton::Right] {
            // Pane 2 inner is (61, 1, 38, 10) -> cols 61..=98, rows 1..=10
            let down = handle_mouse_with_passthrough(
                make_mouse(MouseEventKind::Down(btn), 62, 2),
                None,
                &geom,
                &mut drag,
                |pid| pid == 2,
            );
            assert_eq!(
                down,
                MouseAction::PaneMouse {
                    pane_id: 2,
                    rel_col: 1,
                    rel_row: 1,
                }
            );
            assert!(drag.active_passthrough_pane.is_some());

            // Drag outside pane 2 (into pane 1 at col 10, row 15) -> clamped to inner (0, 9)
            let dragged = handle_mouse_with_passthrough(
                make_mouse(MouseEventKind::Drag(btn), 10, 15),
                None,
                &geom,
                &mut drag,
                |pid| pid == 2,
            );
            assert_eq!(
                dragged,
                MouseAction::PaneMouse {
                    pane_id: 2,
                    rel_col: 0,
                    rel_row: 9,
                }
            );

            // Up outside pane 2 (col 120, row 0) -> clamped to inner (37, 0) and clears active_passthrough_pane
            let up = handle_mouse_with_passthrough(
                make_mouse(MouseEventKind::Up(btn), 120, 0),
                None,
                &geom,
                &mut drag,
                |pid| pid == 2,
            );
            assert_eq!(
                up,
                MouseAction::PaneMouse {
                    pane_id: 2,
                    rel_col: 37,
                    rel_row: 0,
                }
            );
            assert!(drag.active_passthrough_pane.is_none());
        }
    }

    #[test]
    fn passthrough_moved_and_scrolls_emit_pane_mouse_and_fallback_when_disabled() {
        let geom = two_pane_geom();
        let mut drag = MouseDragState::default();

        // Pane 2 has passthrough enabled, pane 1 does not
        let should = |pid| pid == 2;

        for kind in [
            MouseEventKind::Moved,
            MouseEventKind::ScrollUp,
            MouseEventKind::ScrollDown,
            MouseEventKind::ScrollLeft,
            MouseEventKind::ScrollRight,
        ] {
            assert_eq!(
                handle_mouse_with_passthrough(
                    make_mouse(kind, 63, 4),
                    None,
                    &geom,
                    &mut drag,
                    should,
                ),
                MouseAction::PaneMouse {
                    pane_id: 2,
                    rel_col: 2,
                    rel_row: 3,
                }
            );
        }

        // ScrollUp / ScrollDown on non-passthrough pane 1 falls back to ScrollUp / ScrollDown
        assert_eq!(
            handle_mouse_with_passthrough(
                make_mouse(MouseEventKind::ScrollUp, 25, 3),
                None,
                &geom,
                &mut drag,
                should,
            ),
            MouseAction::ScrollUp(1)
        );
        assert_eq!(
            handle_mouse_with_passthrough(
                make_mouse(MouseEventKind::ScrollDown, 25, 3),
                None,
                &geom,
                &mut drag,
                should,
            ),
            MouseAction::ScrollDown(1)
        );
        // Moved / ScrollLeft / ScrollRight on non-passthrough pane 1 -> None
        assert_eq!(
            handle_mouse_with_passthrough(
                make_mouse(MouseEventKind::Moved, 25, 3),
                None,
                &geom,
                &mut drag,
                should,
            ),
            MouseAction::None
        );
    }

    #[test]
    fn sidebar_border_clicks_return_none_and_inner_clicks_hit_exact_row() {
        let geom = two_pane_geom();
        let spaces_area = Rect::new(0, 0, 20, 6);
        let agents_area = Rect::new(0, 6, 20, 6);
        let sidebar = Some((spaces_area, agents_area));
        let mut drag = MouseDragState::default();

        // Top, bottom, left, and right borders of Spaces panel -> None
        for (col, row) in [
            (spaces_area.x + 2, spaces_area.y),
            (spaces_area.x + 2, spaces_area.y + spaces_area.height - 1),
            (spaces_area.x, spaces_area.y + 1),
            (spaces_area.x + spaces_area.width - 1, spaces_area.y + 1),
            (agents_area.x + 2, agents_area.y),
            (agents_area.x, agents_area.y + 1),
        ] {
            assert_eq!(
                handle_mouse(
                    make_mouse(MouseEventKind::Down(MouseButton::Left), col, row),
                    sidebar,
                    &geom,
                    &mut drag,
                ),
                MouseAction::None,
                "expected None on sidebar border ({col}, {row})"
            );
        }

        // Inner first row of Spaces panel -> idx 0 ([+ New Space])
        assert_eq!(
            handle_mouse(
                make_mouse(
                    MouseEventKind::Down(MouseButton::Left),
                    spaces_area.x + 1,
                    spaces_area.y + 1
                ),
                sidebar,
                &geom,
                &mut drag,
            ),
            MouseAction::SelectSidebarRow {
                panel: SidebarPanel::Spaces,
                idx: 0,
            }
        );

        // Inner first row of Agents panel -> idx 0
        assert_eq!(
            handle_mouse(
                make_mouse(
                    MouseEventKind::Down(MouseButton::Left),
                    agents_area.x + 1,
                    agents_area.y + 1
                ),
                sidebar,
                &geom,
                &mut drag,
            ),
            MouseAction::SelectSidebarRow {
                panel: SidebarPanel::Agents,
                idx: 0,
            }
        );
    }
}
