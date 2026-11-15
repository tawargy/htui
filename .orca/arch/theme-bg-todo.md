# Task List: Inherit App Background for Panel Tab Bar & Footer Status Bar

---

## Phase 1: Theme Styles & Rendered Verification

## Task 1: Update `Theme::status_bar()`, `Theme::tab_bar_bg()`, `Theme::tab_new_terminal()`, and Theme Unit Tests

**Description:** Update `src/ui/theme.rs` so `Theme::status_bar()`, `Theme::tab_bar_bg()`, and `Theme::tab_new_terminal()` omit `.bg(Self::SUMI_INK0)` (`bg: None`), allowing the footer status bar, tab bar background, tab `"│"` separators, and `[+ New Terminal]` button to inherit the main app/terminal background just like `render_sidebar`, `render_panes`, and dialogs. Preserve `Theme::tab_active()` (`fg: SUMI_INK0, bg: CRYSTAL_BLUE, BOLD`) and `Theme::tab_inactive()` (`fg: OLD_WHITE, bg: SUMI_INK3`) so active and inactive tab pills maintain readable contrast. Update `kanagawa_wave_palette_styles` in `src/ui/theme.rs` (TDD: update/add assertions first, confirm failure, then apply the style edits).

**Acceptance criteria:**
- [x] `Theme::status_bar()` returns `Style::default().fg(Self::OLD_WHITE)` with `bg == None`.
- [x] `Theme::tab_bar_bg()` returns `Style::default().fg(Self::SUMI_INK4)` with `bg == None`.
- [x] `Theme::tab_new_terminal()` returns `Style::default().fg(Self::WAVE_AQUA2).add_modifier(Modifier::BOLD)` with `bg == None`.
- [x] `Theme::tab_active()` (`bg == Some(Theme::CRYSTAL_BLUE)`) and `Theme::tab_inactive()` (`bg == Some(Theme::SUMI_INK3)`, `fg == Some(Theme::OLD_WHITE)`) preserve pill contrast.
- [x] `kanagawa_wave_palette_styles` in `src/ui/theme.rs` verifies the updated `bg` and `fg` values and passes.

**Verification:**
- [x] Tests pass: `cargo test --lib ui::theme`
- [x] Build succeeds: `cargo check --all-targets`

**Dependencies:** None

**Files likely touched:**
- `src/ui/theme.rs`

**Estimated scope:** XS (1 file)

---

## Task 2: Verify Rendered Tab Bar & Footer Background Inheritance in `tests/ui_snapshots.rs`

**Description:** Add rendered buffer assertions in `tests/ui_snapshots.rs` (extending `tab_bar_render_click_and_cursor_alignment` or adding a dedicated snapshot test using `TuiApp::render` with active & inactive tabs) to verify that the rendered panel tab bar empty space, `"│"` separator, `"[+ New Terminal]"` button, and footer status bar cells all have `bg == Color::Reset` (matching the sidebar/pane background and not `Theme::SUMI_INK0`), while the active tab pill has `bg == Theme::CRYSTAL_BLUE` and inactive tab pills have `bg == Theme::SUMI_INK3`. Confirm `src/ui/tabs.rs` and `src/ui/statusbar.rs` require no extra overrides and pass formatting, clippy, and the full test suite.

**Acceptance criteria:**
- [x] Rendered `TuiApp::render` buffer asserts footer status bar cells (`y = area.height - 1`) have `cell.bg == Color::Reset` (and `!= Theme::SUMI_INK0`), matching sidebar/pane frame background cells.
- [x] Rendered `TuiApp::render` buffer asserts tab bar empty cells, `"│"` separator cell, and `"[+ New Terminal]"` cells (`y = 0` over `right_area`) have `cell.bg == Color::Reset` (and `!= Theme::SUMI_INK0`).
- [x] Rendered buffer asserts active tab pill cells use `Theme::CRYSTAL_BLUE` and inactive tab pill cells use `Theme::SUMI_INK3` with readable foreground colors (`Theme::SUMI_INK0` and `Theme::OLD_WHITE`).
- [x] Full test suite, formatter check, and clippy pass with zero warnings.

**Verification:**
- [x] Tests pass: `cargo test --test ui_snapshots && cargo test --all-targets`
- [x] Format & lint succeed: `cargo fmt --all -- --check && cargo clippy --all-targets -- -D warnings`

**Dependencies:** Task 1

**Files likely touched:**
- `tests/ui_snapshots.rs`

**Estimated scope:** Small (1 file)

---

## Checkpoint: Complete
- [x] All acceptance criteria met
- [x] `cargo test --all-targets`, `cargo fmt --all -- --check`, and `cargo clippy --all-targets -- -D warnings` pass cleanly
- [x] Ready for human review
