# Task List: `Command::ClosePane` Closes Spaces, Kills/Removes Unattached Terminals, and Hides Empty Spaces from Sidebar

---

## Phase 1: Sidebar Empty-Space Hiding

## Task 1: Hide Empty Spaces in `SidebarState::rebuild` and Verify Sidebar / Model Invariants

**Description:** Using TDD, first update `tests/ui_snapshots.rs::new_space_shortcut_and_spawn_unassigned` and add unit tests for `SidebarState::rebuild` (including an empty `SpaceNode` from `declared_spaces` and a finished-only space when `show_finished == false`) to assert that zero-terminal spaces are hidden from `sidebar.visible_items`, while `app.active_space`, `app.layouts.spaces`, and `ST-2` in `tests/model.rs` remain valid. Observe the new assertions failing against the current `SidebarState::rebuild`, then update `SidebarState::rebuild` in `src/ui/sidebar.rs` to skip any `SpaceNode` where `sp.terminals.is_empty()`. Also verify via a unit/async test on `reconcile_startup_layouts` + `refresh_sidebar` that startup with `initial_space_name = Some("new-space")` and stale empty spaces produces zero `count: 0` `SidebarItem::Space` entries.

**Acceptance criteria:**
- [x] `SidebarState::rebuild` in `src/ui/sidebar.rs` never emits `SidebarItem::Space` when `sp.terminals.is_empty()`, including when `show_finished == false` hides a space's only terminal or when an active newly created space has 0 terminals.
- [x] `AppModelState::build_tree` (`src/model/reducer.rs`) continues to satisfy `ST-2` in `tests/model.rs` untouched.
- [x] `tests/ui_snapshots.rs::new_space_shortcut_and_spawn_unassigned` verifies `"backend"` is `active_space` and in `layouts.spaces` while hidden from `sidebar.visible_items` until a terminal is added in `"backend"`, and `reconcile_startup_layouts` leaves no empty spaces in the sidebar.

**Verification:**
- [x] Tests pass: `cargo test --test model && cargo test --test ui_snapshots`
- [x] Build succeeds: `cargo check --all-targets`

**Dependencies:** None

**Files likely touched:**
- `src/ui/sidebar.rs`
- `tests/ui_snapshots.rs`

**Estimated scope:** Small (2 files)

---

## Checkpoint: After Task 1
- [x] `cargo test --test model` and `cargo test --test ui_snapshots` pass
- [x] `cargo check --all-targets` succeeds with zero warnings
- [x] Review progress before Phase 2

---

## Phase 2: `Command::ClosePane` Last-Pane Space & Unattached Terminal Cleanup

## Task 2: Close Named Spaces and Kill/Remove Unattached Terminals on Last Pane (`Command::ClosePane`)

**Description:** Using TDD, add unit tests in `tests/ui_snapshots.rs` for `Command::ClosePane` (`Ctrl+b q` / `Ctrl+b x`) running on the last pane (`!had_multi`) of a named space (`cur_space != UNASSIGNED_SPACE`): (a) when the pane is `(empty)` (`closed_addr == None`) and `model.terminals` has unattached live and/or finished terminals belonging to `cur_space`, and (b) when the pane has an attached terminal plus an unattached terminal in `cur_space`. Confirm these tests fail against the current `"Cannot close last pane"` implementation. Then in `src/ui/app.rs`, extract `kill_or_remove_terminal` and `close_space_terminals` helpers on `TuiApp`, make next-space selection deterministic (`.min()`) in `remove_empty_space` and `prune_empty_spaces`, and update `Command::ClosePane` so `!had_multi && cur_space != UNASSIGNED_SPACE` always kills/removes `closed_addr` (if `Some`) and all unattached terminals in `cur_space`, removes `cur_space` via `remove_empty_space`, emits `AppAction::RemoveLayout(cur_space)`, and never emits `"Cannot close last pane"`.

**Acceptance criteria:**
- [x] Running `Command::ClosePane` on the single pane (`!had_multi`) of a named space — whether `closed_addr` is `Some(addr)` or `None` (`(empty)`) — finds all terminals in `self.model.terminals` with `TerminalEntry { terminal }.space_name() == cur_space`, emits `Kill`+`Remove` (live/unknown) or `Remove` (finished) for each in deterministic sorted address order, and removes them from `self.model.terminals` and `self.sessions`.
- [x] The named space is removed from `self.model.declared_spaces` and `self.layouts.spaces` via `remove_empty_space`, `AppAction::RemoveLayout(cur_space)` is emitted, `active_space` switches deterministically to another named space (if present) or `UNASSIGNED_SPACE`, and `"Cannot close last pane"` is never emitted.
- [x] Multi-pane (`had_multi == true`) `ClosePane` behavior in named spaces continues to close only the focused pane and emit `SaveLayout(cur_space)` when open terminals remain or `RemoveLayout(cur_space)` when empty.

**Verification:**
- [x] Tests pass: `cargo test --test ui_snapshots`
- [x] Build succeeds: `cargo check --all-targets`

**Dependencies:** Task 1

**Files likely touched:**
- `src/ui/app.rs`
- `tests/ui_snapshots.rs`

**Estimated scope:** Small (2 files)

---

## Task 3: Handle `UNASSIGNED_SPACE` Last Pane in `Command::ClosePane` and Add `FakeHcServer` End-to-End Tests

**Description:** Using TDD, add unit tests and an async `FakeHcServer` + `LayoutStore` integration test in `tests/ui_snapshots.rs` covering `Command::ClosePane` on `UNASSIGNED_SPACE` when `!had_multi`: (a) `(empty)` pane with unattached live + finished unassigned terminals and another named space `"api"` present -> kills/removes unattached terminals, emits `RemoveLayout(UNASSIGNED_SPACE)`, removes `UNASSIGNED_SPACE` from `layouts.spaces`, switches to `"api"`, no error toast; (b) `(empty)` pane with 0 unassigned terminals and `"api"` present -> removes `UNASSIGNED_SPACE`, emits `RemoveLayout(UNASSIGNED_SPACE)`, switches to `"api"`, no error toast; (c) `(empty)` pane with 0 unassigned terminals and 0 other spaces -> no layout action emitted, clean informational toast (`"Workspace is already empty"`), `(unassigned)` absent from sidebar; and (d) async `FakeHcServer` + `LayoutStore` dispatch for closing a named space's `(empty)` last pane with unattached live terminal and `UNASSIGNED_SPACE` with unattached terminals, verifying `rpc.terminal_list()`, `LayoutStore` (remote + local fallback), `sidebar`, and idempotent `handle_core_event` replay. Observe failures first, then implement the `!had_multi && cur_space == UNASSIGNED_SPACE` branch in `src/ui/app.rs`.

**Acceptance criteria:**
- [x] In `UNASSIGNED_SPACE` with `!had_multi`, `Command::ClosePane` kills/removes any attached and unattached unassigned terminals in `self.model.terminals` & `self.sessions`, and when another non-unassigned space exists in `self.layouts.spaces`, removes `UNASSIGNED_SPACE`, emits `AppAction::RemoveLayout(UNASSIGNED_SPACE)`, and switches deterministically to that space without showing `"Cannot close last pane"`.
- [x] When no other space exists in `self.layouts.spaces`: if any terminal was closed/killed/removed, `AppAction::RemoveLayout(UNASSIGNED_SPACE)` is emitted with no error toast; if nothing was closed/killed (already 0 terminals and 1 empty pane), no layout action is emitted and `"Workspace is already empty"` toast is shown, with `(unassigned)` absent from the sidebar in all cases.
- [x] Full test suite, `cargo fmt --all -- --check`, and `cargo clippy --all-targets -- -D warnings` pass cleanly.

**Verification:**
- [x] Tests pass: `cargo test --all-targets`
- [x] Format & lint succeed: `cargo fmt --all -- --check && cargo clippy --all-targets -- -D warnings`
- [x] Manual check: Verify `Command::ConfirmKill`, `Command::ConfirmRemove`, and `tests/model.rs` (`ST-2`) remain intact.

**Dependencies:** Task 2

**Files likely touched:**
- `src/ui/app.rs`
- `tests/ui_snapshots.rs`

**Estimated scope:** Small (2 files)

---

## Checkpoint: Complete
- [x] All acceptance criteria met
- [x] `cargo test --all-targets`, `cargo fmt --all -- --check`, and `cargo clippy --all-targets -- -D warnings` pass cleanly
- [x] Ready for human review
