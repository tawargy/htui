# Task List: Show Active Empty Named Space in Sidebar & Harden New-Space Flow

> **Note:** Prior completed tasks are archived at [closepane-empty-spaces-todo.md](file:///home/tawargy/Documents/0-WorkSpace/1-SideProjects/htui/.orca/arch/closepane-empty-spaces-todo.md).

---

## Phase 1: Core Sidebar Active-Space Visibility

## Task 1: Make Active Named Space Visible in `SidebarState::rebuild` While Keeping `(unassigned)` and Inactive Empty Spaces Hidden

**Description:** Using TDD, first update `tests/ui_snapshots.rs::new_space_shortcut_and_spawn_unassigned` (lines 347-402) to assert `"backend"` (`count: 0`) IS immediately visible in `app.sidebar.visible_items` after `Ctrl+b c` + Enter while `UNASSIGNED_SPACE` remains absent, and update `tests/ui_snapshots.rs::sidebar_rebuild_hides_empty_and_finished_only_spaces` (lines 405-486) to cover: (a) `SidebarState::rebuild(&tree, None)` and `rebuild(&tree, Some(UNASSIGNED_SPACE))` hiding `"empty-declared"` and empty `UNASSIGNED_SPACE`, (b) `rebuild(&tree, Some("empty-declared"))` showing `"empty-declared (0)"` while keeping empty `UNASSIGNED_SPACE` hidden, and (c) an inactive finished-only space `"jobs"` (attached `t-fin`, `active_space != "jobs"`) hidden when `show_finished == false` and visible (`count: 1`) when `show_finished == true` (plus active `"jobs"` showing `count: 0` when `show_finished == false`). Observe the new assertions failing against current `SidebarState::rebuild`, then update `SidebarState::rebuild(&mut self, tree: &[MachineNode], active_space: Option<&str>)` in `src/ui/sidebar.rs` (lines 67-89) and `TuiApp::refresh_sidebar` in `src/ui/app.rs` (line 169). Also update the remaining direct `sb.rebuild(...)` calls in `tests/ui_snapshots.rs` (lines 62, 124).

**Acceptance criteria:**
- [x] `SidebarState::rebuild` (`src/ui/sidebar.rs:67-89`) emits `SidebarItem::Space` when `!sp.terminals.is_empty()` OR `(m.is_local && active_space == Some(sp.name.as_str()) && sp.name != UNASSIGNED_SPACE)`.
- [x] Empty `UNASSIGNED_SPACE` and inactive empty spaces (including inactive finished-only spaces when `show_finished == false`) never appear in `sidebar.visible_items`, even when `active_space == UNASSIGNED_SPACE`.
- [x] `tests/model.rs` (`ST-2`) and `tests/ui_snapshots.rs` (`sidebar_snapshot`, `sidebar_agents_panel_excludes_plain_shells`, `new_space_shortcut_and_spawn_unassigned`, `sidebar_rebuild_hides_empty_and_finished_only_spaces`) pass.

**Verification:**
- [x] Tests pass: `cargo test --test model` (2/2) and Task 1 target tests in `tests/ui_snapshots.rs` (33/34; remaining 1 is `reconcile_startup_layouts_cleans_stale_and_unassigned_while_preserving_create` updated in Task 3)
- [x] Build & lint succeed: `cargo check --all-targets`

**Dependencies:** None

**Files likely touched:**
- `src/ui/sidebar.rs`
- `src/ui/app.rs`
- `tests/ui_snapshots.rs`

**Estimated scope:** Small (3 files)

---

## Checkpoint: After Task 1
- [x] `cargo test --test model && cargo test --test ui_snapshots` pass (verified together with Task 3 completion)
- [x] `cargo check --all-targets` succeeds with zero warnings
- [x] Review progress before Phase 2

---

## Phase 2: `[+ New Space]` Click, `SubmitNewSpace`, and Lifecycle Hardening

## Task 2: Fix `[+ New Space]` Mouse Hit-Testing, `SubmitNewSpace` Validation/Focus/Selection, and Space-Switch Sidebar Refresh

**Description:** Using TDD, add tests in `src/ui/mouse.rs` (unit tests) and `tests/ui_snapshots.rs` covering:
1. Clicking `[+ New Space]` (`SidebarItem::NewSpaceAction` at inner row `spaces_area.y + 1`) via `app.render` + `app.handle_mouse_event` opens `NewSpaceDialog`, and submitting `"frontend"` makes `"frontend (0)"` visible in `sidebar.visible_items`, clears `app.sidebar_focused = false`, and selects `"frontend"` in `app.sidebar.selected`; whereas clicking the `spaces_area` border (`row == spaces_area.y` or `col == spaces_area.x`) returns `MouseAction::None` and does not open `NewSpaceDialog`.
2. Empty/whitespace input in `NewSpaceDialog` closes without creating a space; `DialogOutcome::SubmitNewSpace(UNASSIGNED_SPACE.into())` or `"   "` does NOT add to `declared_spaces` or emit `AppAction::SaveLayout(UNASSIGNED_SPACE)` and sets a toast (`"Invalid space name"`).
3. Submitting a duplicate/existing space name (e.g., `"api"` when `"api"` already has a split layout and terminal `t1`) switches `active_space` to `"api"` without overwriting `layouts.spaces["api"]` or resetting its terminal count in `sidebar.visible_items`, clears `sidebar_focused`, and selects `"api"`.
4. Switching away from an active space with 0 visible terminals (e.g., `"jobs"` with finished `t-fin` and `show_finished == false`) to another space via `SidebarItem::Space` Enter/click refreshes the sidebar immediately so `"jobs"` hides without breaking `sidebar_focused` navigation in `sidebar_new_space_at_index_zero_and_keyboard_nav`.

Observe failures first, then implement:
- Inner-rect checks via `pane_inner(spaces_area)` / `pane_inner(agents_area)` in `src/ui/mouse.rs:88-105`.
- Defensive trim + empty/`UNASSIGNED_SPACE` guard, `self.sidebar_focused = false`, and selecting the created/switched space row in `DialogOutcome::SubmitNewSpace` (`src/ui/app.rs:293-303`).
- Unconditional `self.refresh_sidebar()` after `self.switch_space(...)` in `SidebarItem::Space` and `SidebarItem::Terminal` handlers (`src/ui/app.rs:365-392, 544-568`).

**Acceptance criteria:**
- [x] Clicking inside `spaces_area`/`agents_area` borders hits the exact list row, while clicking the frame border returns `MouseAction::None` without opening `NewSpaceDialog`.
- [x] `DialogOutcome::SubmitNewSpace` rejects empty/`UNASSIGNED_SPACE` names cleanly, preserves existing layout/terminals on duplicate space names, clears `self.sidebar_focused = false`, and selects the active space in `self.sidebar.selected`.
- [x] Switching spaces via sidebar Enter/click always refreshes `sidebar.visible_items` while preserving `sidebar_focused` for `SidebarItem::Space` navigation.

**Verification:**
- [x] Tests pass: `cargo test --lib` (33/33) and Task 2 target tests in `tests/ui_snapshots.rs` (37/38; remaining 1 is `reconcile_startup_layouts_cleans_stale_and_unassigned_while_preserving_create` updated in Task 3)
- [x] Build succeeds: `cargo check --all-targets`

**Dependencies:** Task 1

**Files likely touched:**
- `src/ui/mouse.rs`
- `src/ui/app.rs`
- `tests/ui_snapshots.rs`

**Estimated scope:** Small (3 files)

---

## Task 3: Update & Verify `htui open --create` Startup, `switch_space`, `prune_empty_spaces`, `ClosePane`, and `Quit` Lifecycle

**Description:** Using TDD, update `tests/ui_snapshots.rs::reconcile_startup_layouts_cleans_stale_and_unassigned_while_preserving_create` (lines 2261-2268) so startup with `initial_space_name = Some("new-space")` (`htui open new-space --create`) asserts `SidebarItem::Space { name: "new-space", count: 0, .. }` IS visible in `app.sidebar.visible_items` while `"stale"` and empty `UNASSIGNED_SPACE` remain hidden. Also add an end-to-end lifecycle test in `tests/ui_snapshots.rs` verifying:
1. Creating `"draft"` (`Ctrl+b c`) makes `"draft (0)"` visible in `sidebar.visible_items` and survives background `refresh_sidebar()` / `prune_empty_spaces()` while active.
2. Closing `"draft"`'s last empty pane via `Command::ClosePane` (`Ctrl+b x` / `Ctrl+b q`) removes `"draft"` from `layouts.spaces`, `declared_spaces`, and `sidebar.visible_items`, emits `RemoveLayout("draft")`, and falls back to `UNASSIGNED_SPACE` (remaining hidden in sidebar) or next named space.
3. Re-creating `"draft"` and switching away via `switch_space` (or quitting via `Command::Quit`) removes `"draft"` from `layouts.spaces`, `declared_spaces`, and `sidebar.visible_items`.
4. Disconnected startup in `run_tui_loop` (`src/ui/event_loop.rs:114-117`) calls `app.refresh_sidebar()` so `[+ New Space]` and `initial_space` (`--create`) render on frame 1 even before `hc` connects.

**Acceptance criteria:**
- [x] `reconcile_startup_layouts_cleans_stale_and_unassigned_while_preserving_create` verifies `"new-space"` (`count: 0`) is visible in `sidebar.visible_items` on `--create` startup while `"stale"` and empty `UNASSIGNED_SPACE` are hidden and pruned.
- [x] `run_tui_loop` disconnected startup initializes `app.refresh_sidebar()`, and lifecycle tests verify `switch_space`, `prune_empty_spaces`, `Command::ClosePane`, and `Command::Quit` prune inactive/closed empty spaces while keeping empty `UNASSIGNED_SPACE` hidden.
- [x] Full formatting, clippy, and test suite pass cleanly with zero warnings.

**Verification:**
- [x] Format check: `cargo fmt --all -- --check`
- [x] Lint check: `cargo clippy --all-targets -- -D warnings`
- [x] Full test suite: `cargo test --all-targets`

**Dependencies:** Task 2

**Files likely touched:**
- `src/ui/event_loop.rs`
- `tests/ui_snapshots.rs`

**Estimated scope:** Small (2 files)

---

## Checkpoint: Complete
- [x] All acceptance criteria met
- [x] `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test --all-targets` pass cleanly
- [x] Ready for human review
