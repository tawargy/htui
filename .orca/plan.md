# Implementation Plan: Unified Action Button Styling (New Space & New Terminal)

## Overview
Unify the visual styling of all "new space" and "new terminal" action buttons across the TUI to match inactive terminal tabs (`Theme::tab_inactive()`): `Theme::OLD_WHITE` foreground (`Rgb(200, 192, 147)`) on `Theme::SUMI_INK3` background (`Rgb(54, 54, 70)`), with no bold modifier. This affects three key UI elements:
1. Sidebar `[+ New Space]` (`SidebarItem::NewSpaceAction` in `src/ui/sidebar.rs`)
2. Tab bar `[+ New Terminal]` (`NEW_TERMINAL_TAB_LABEL` in `src/ui/tabs.rs`)
3. Empty-pane `[+ New Terminal (Ctrl+B n)]` (`SPAWN_PANE_LABEL` in `src/ui/panes.rs`)

---

## Target Style Contract

| Property | Value | Rationale |
|---|---|---|
| **Foreground (`fg`)** | `Theme::OLD_WHITE` (`Rgb(200, 192, 147)`) | Matches `Theme::tab_inactive()` text color |
| **Background (`bg`)** | `Theme::SUMI_INK3` (`Rgb(54, 54, 70)`) | Matches `Theme::tab_inactive()` pill background |
| **Modifier** | None (no `Modifier::BOLD`) | Consistent with `Theme::tab_inactive()`, avoiding inconsistent font weights |

---

## Scope & Architectural Decisions

### 1. Theme Module (`src/ui/theme.rs`)
- **`Theme::new_space_action()`**: Update definition to return `Style::default().fg(Self::OLD_WHITE).bg(Self::SUMI_INK3)` without `Modifier::BOLD`.
- **`Theme::tab_new_terminal()`**: Update definition to return `Style::default().fg(Self::OLD_WHITE).bg(Self::SUMI_INK3)` without `Modifier::BOLD`.
- **`Theme::badge_active()`**: Retain existing definition for backwards compatibility, but decouple `SPAWN_PANE_LABEL` in `src/ui/panes.rs` from it.
- **Unit test `kanagawa_wave_palette_styles`**: Update assertions to verify `tab_new_terminal()` and `new_space_action()` have `bg == Some(Theme::SUMI_INK3)`, `fg == Some(Theme::OLD_WHITE)`, and no `Modifier::BOLD`.

### 2. Tab Bar (`src/ui/tabs.rs` & `tests/ui_snapshots.rs`)
- `src/ui/tabs.rs` already references `Theme::tab_new_terminal()` for `NEW_TERMINAL_TAB_LABEL` (`" [+ New Terminal] "`). Updating `Theme::tab_new_terminal()` automatically styles the tab button.
- In `tests/ui_snapshots.rs` (lines 1503-1509 of `tab_bar_and_footer_inherit_app_background`), update assertions: previously asserted `cell.bg == Color::Reset` and `cell.fg == Theme::WAVE_AQUA2`; update to assert `cell.bg == Theme::SUMI_INK3` and `cell.fg == Theme::OLD_WHITE`.

### 3. Empty-Pane Spawn Button (`src/ui/panes.rs` & `tests/ui_snapshots.rs`)
- In `src/ui/panes.rs` line 124, switch `Paragraph::new(Span::styled(SPAWN_PANE_LABEL, Theme::badge_active()))` to `Theme::tab_new_terminal()`.
- Add a snapshot test in `tests/ui_snapshots.rs` asserting empty pane spawn button cell colors (`SUMI_INK3` background, `OLD_WHITE` foreground).

### 4. Sidebar New Space Action (`src/ui/sidebar.rs` & `tests/ui_snapshots.rs`)
- **Leading indent separation**: In `render_spaces_panel`, change `Line::from(Span::styled("  [+ New Space]", ...))` to `Line::from(vec![Span::raw("  "), Span::styled("[+ New Space]", action_style)])`. This ensures the 2-space indentation before the button does not get the `SUMI_INK3` pill background when unselected.
- **Selection override guard**: In Ratatui, an explicit `bg(SUMI_INK3)` on a `Span` overrides `ListItem::style(Theme::selected_row())` (`WAVE_BLUE2` background). When selected (`state.active_panel == SidebarPanel::Spaces && idx == state.selected`), set `action_style` to `Style::default()`. This allows `li = li.style(Theme::selected_row())` to cleanly highlight the entire row (indent, button, and trailing width) with `Theme::selected_row()` (`WAVE_BLUE2` bg, `FUJI_WHITE` fg, `BOLD`).
- **Mouse interaction safety**: `src/ui/mouse.rs` hit-testing computes `(row - inner.y) as usize` across the entire row width. Splitting the spans within the line has zero effect on mouse hit-testing.
- Add a test in `tests/ui_snapshots.rs` validating both unselected (indent unstyled, `[+ New Space]` in `OLD_WHITE`/`SUMI_INK3`) and selected (`Theme::selected_row()`) row rendering.

---

## Task List

### Phase 1: Theme Style Contract & Unit Tests
- [x] Task 1: Update `Theme::new_space_action()` and `Theme::tab_new_terminal()` in `src/ui/theme.rs` and update unit tests

### Checkpoint: Theme Foundation
- [x] `cargo test --lib ui::theme::tests::kanagawa_wave_palette_styles` passes

### Phase 2: Tab Bar & Empty Pane Button Styling
- [x] Task 2: Switch empty-pane spawn label in `src/ui/panes.rs` to `Theme::tab_new_terminal()`, update tab bar snapshot assertions and add empty-pane style snapshot test in `tests/ui_snapshots.rs`

### Checkpoint: Tab Bar & Panes
- [x] `cargo test --test ui_snapshots tab_bar` and empty pane tests pass

### Phase 3: Sidebar New Space Action & Selection Highlight
- [x] Task 3: Separate indent and guard selection override for `SidebarItem::NewSpaceAction` in `src/ui/sidebar.rs`, and add snapshot tests in `tests/ui_snapshots.rs`

### Checkpoint: Full Regression & Build
- [x] `cargo test` passes cleanly with 0 failures across all 33 unit tests and all integration test suites

---

## Risks and Mitigations

| Risk | Impact | Mitigation |
|---|---|---|
| Span background overriding selected row highlight in sidebar | High | When selected, pass `Style::default()` on the span so `ListItem::style(Theme::selected_row())` styles the entire row uniformly. Verified by dedicated snapshot test. |
| Inconsistent bold modifier across tabs and action buttons | Low | `Theme::tab_inactive()` has no `BOLD`. Removing `Modifier::BOLD` from both `tab_new_terminal()` and `new_space_action()` guarantees identical font weight. |
| Sidebar mouse hit-testing broken by span separation | Low | Confirmed in `src/ui/mouse.rs`: hit testing uses row index `row - inner.y`, independent of span layout. Existing mouse tests verify click coordinates. |
| Breaking downstream references to `Theme::badge_active()` | Low | Keep `Theme::badge_active()` defined in `src/ui/theme.rs`; only redirect `src/ui/panes.rs` to use `Theme::tab_new_terminal()`. |

---

## Open Questions
None. Requirements, color codes, and visual behaviors have been confirmed.
