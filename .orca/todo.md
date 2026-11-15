# Task List: Unified Action Button Styling (New Space & New Terminal)

## Phase 1: Theme Style Contract & Unit Tests

### Task 1: Update `Theme::new_space_action()` and `Theme::tab_new_terminal()` in `src/ui/theme.rs` and update unit tests
**Description:** Update `Theme::new_space_action()` and `Theme::tab_new_terminal()` to match `Theme::tab_inactive()` (`fg: OLD_WHITE`, `bg: SUMI_INK3`, no `BOLD` modifier). Retain `Theme::badge_active()`. Update `kanagawa_wave_palette_styles` unit test to verify new styles.

**Acceptance criteria:**
- [ ] `Theme::new_space_action()` returns `Style::default().fg(Self::OLD_WHITE).bg(Self::SUMI_INK3)` without `Modifier::BOLD`.
- [ ] `Theme::tab_new_terminal()` returns `Style::default().fg(Self::OLD_WHITE).bg(Self::SUMI_INK3)` without `Modifier::BOLD`.
- [ ] `Theme::badge_active()` remains intact in `Theme` for backwards compatibility.
- [ ] Unit test `kanagawa_wave_palette_styles` in `src/ui/theme.rs` asserts `Theme::tab_new_terminal().bg == Some(Theme::SUMI_INK3)`, `fg == Some(Theme::OLD_WHITE)`, and modifier does NOT contain `Modifier::BOLD`.
- [ ] Unit test `kanagawa_wave_palette_styles` asserts `Theme::new_space_action().bg == Some(Theme::SUMI_INK3)`, `fg == Some(Theme::OLD_WHITE)`, and modifier does NOT contain `Modifier::BOLD`.

**Verification:**
- [ ] Tests pass: `cargo test --lib ui::theme::tests::kanagawa_wave_palette_styles`
- [ ] Build succeeds: `cargo check`

**Dependencies:** None

**Files likely touched:**
- `src/ui/theme.rs`

**Estimated scope:** Small (1 file)

---

## Checkpoint: Theme Foundation
- [ ] All unit tests in `src/ui/theme.rs` pass (`cargo test --lib ui::theme::tests`)
- [ ] Build succeeds without warnings

---

## Phase 2: Tab Bar & Empty Pane Button Styling

### Task 2: Switch empty-pane spawn label in `src/ui/panes.rs` to `Theme::tab_new_terminal()`, update tab bar snapshot assertions and add empty-pane style snapshot test in `tests/ui_snapshots.rs`
**Description:** Update `src/ui/panes.rs` empty-pane spawn button to use `Theme::tab_new_terminal()`. Update existing tab bar assertions in `tests/ui_snapshots.rs` (lines 1503-1509) to assert `SUMI_INK3` background and `OLD_WHITE` foreground. Add test in `tests/ui_snapshots.rs` checking empty-pane spawn button cells match `SUMI_INK3` bg and `OLD_WHITE` fg.

**Acceptance criteria:**
- [ ] `src/ui/panes.rs` renders `SPAWN_PANE_LABEL` with `Theme::tab_new_terminal()` instead of `Theme::badge_active()`.
- [ ] `tests/ui_snapshots.rs` in test `tab_bar_and_footer_inherit_app_background` asserts all cells in `new_rect` have `bg == Theme::SUMI_INK3` and `fg == Theme::OLD_WHITE`.
- [ ] `tests/ui_snapshots.rs` includes a test verifying empty-pane spawn button cells have `bg == Theme::SUMI_INK3` and `fg == Theme::OLD_WHITE`.

**Verification:**
- [ ] Tests pass: `cargo test --test ui_snapshots tab_bar_and_footer_inherit_app_background`
- [ ] Tests pass: `cargo test --test ui_snapshots empty_pane`
- [ ] Build succeeds: `cargo check`

**Dependencies:** Task 1

**Files likely touched:**
- `src/ui/panes.rs`
- `tests/ui_snapshots.rs`

**Estimated scope:** Small (2 files)

---

## Checkpoint: Tab Bar & Panes
- [ ] `cargo test --test ui_snapshots tab_bar_and_footer_inherit_app_background` passes
- [ ] `cargo test --test ui_snapshots empty_pane` passes

---

## Phase 3: Sidebar New Space Action & Selection Highlight

### Task 3: Separate indent and guard selection override for `SidebarItem::NewSpaceAction` in `src/ui/sidebar.rs`, and add snapshot tests in `tests/ui_snapshots.rs`
**Description:** In `src/ui/sidebar.rs`, separate the leading `"  "` indent from `"[+ New Space]"` so the indent does not receive `SUMI_INK3` background. When selected (`state.active_panel == SidebarPanel::Spaces && idx == state.selected`), apply `Style::default()` to the button span so `ListItem::style(Theme::selected_row())` (`WAVE_BLUE2` bg, `FUJI_WHITE` fg, `BOLD`) completely highlights the row without `SUMI_INK3` overriding the background. Add snapshot tests in `tests/ui_snapshots.rs` verifying both unselected and selected styling of `[+ New Space]`.

**Acceptance criteria:**
- [ ] In `src/ui/sidebar.rs` `render_spaces_panel`, `SidebarItem::NewSpaceAction` renders `Line::from(vec![Span::raw("  "), Span::styled("[+ New Space]", action_style)])`.
- [ ] `action_style` is `Style::default()` when `is_selected` is true, and `Theme::new_space_action()` when `is_selected` is false.
- [ ] In `tests/ui_snapshots.rs`, add a test (or extend existing tests) verifying:
  - When `SidebarItem::NewSpaceAction` is unselected, the `"  "` indent cells have `Color::Reset` background, and the `"[+ New Space]"` cells have `bg == Theme::SUMI_INK3` and `fg == Theme::OLD_WHITE`.
  - When `SidebarItem::NewSpaceAction` is selected (`active_panel == SidebarPanel::Spaces` and `selected == 0`), row cells have `bg == Theme::WAVE_BLUE2`, `fg == Theme::FUJI_WHITE`, and `Modifier::BOLD`.
- [ ] All sidebar mouse navigation and clicking tests in `tests/ui_snapshots.rs` continue to pass.

**Verification:**
- [ ] Tests pass: `cargo test --test ui_snapshots sidebar`
- [ ] Tests pass: `cargo test --test ui_snapshots new_space`
- [ ] Full test suite passes: `cargo test`

**Dependencies:** Task 1, Task 2

**Files likely touched:**
- `src/ui/sidebar.rs`
- `tests/ui_snapshots.rs`

**Estimated scope:** Small (2 files)

---

## Final Checkpoint: Full Regression & Build
- [ ] Full test suite passes: `cargo test` (33 lib unit tests + all integration test binaries)
- [ ] No compilation warnings or clippy errors: `cargo clippy --all-targets`
