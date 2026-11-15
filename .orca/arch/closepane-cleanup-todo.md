# Task List: `Ctrl+b q` ClosePane Rebinding & Empty Space / Empty Split Cleanup

---

## Phase 1: Keymap & Layout Foundations

## Task 1: Rebind `Ctrl+b q` to `Command::ClosePane` and Update `bindings_help()`

**Description:** Update `PrefixRouter::route` in `src/ui/keymap.rs` so `KeyCode::Char('q')` maps to `Command::ClosePane` alongside `KeyCode::Char('x')`, while `KeyCode::Char('d')` remains bound to `Command::Quit`. Update `PrefixRouter::bindings_help()` to reflect `("Ctrl+B x / q", "Close focused pane")` and `("Ctrl+B d", "Detach and quit htui")`. Add TDD unit tests in `src/ui/keymap.rs` and update/extend `tests/ui_snapshots.rs` to verify `Ctrl+b q` triggers `ClosePane` (not `Quit`), `Ctrl+b d` still triggers `Quit`, and `HelpDialog` renders the updated help text.

**Acceptance criteria:**
- [x] `Ctrl+b q` and `Ctrl+b x` both route to `RouteResult::Command(Command::ClosePane)`.
- [x] `Ctrl+b d` routes to `RouteResult::Command(Command::Quit)` (and `Ctrl+b q` does NOT set `app.should_quit`).
- [x] `PrefixRouter::bindings_help()` and `HelpDialog` list `"Ctrl+B x / q"` for closing the focused pane and `"Ctrl+B d"` for detaching/quitting (without `"Ctrl+B d / q"`).
- [x] Unit tests in `src/ui/keymap.rs` and snapshot/app tests in `tests/ui_snapshots.rs` verify `Ctrl+b q`, `Ctrl+b x`, `Ctrl+b d`, and `HelpDialog` help lines.

**Verification:**
- [x] Tests pass: `cargo test --lib ui::keymap && cargo test --test ui_snapshots app_lifecycle && cargo test --test ui_snapshots dialogs_snapshot`
- [x] Build succeeds: `cargo check --all-targets`

**Dependencies:** None

**Files likely touched:**
- `src/ui/keymap.rs`
- `tests/ui_snapshots.rs`

**Estimated scope:** Small (2 files)

---

## Task 2: Add `SplitTree::has_terminals`, `SplitTree::collapse_empty_splits`, and `LayoutStore::prune_empty_spaces`

**Description:** Add `pub fn has_terminals(&self) -> bool` and `pub fn collapse_empty_splits(&mut self) -> bool` to `SplitTree` in `src/layout/tree.rs` so callers can detect when a split tree has zero terminal panels and collapse multi-pane empty splits back to a single empty leaf (while keeping `SplitTree::prune_removed` behavior intact when terminals still remain). Add `pub async fn prune_empty_spaces(&self, rpc: Option<&RpcClient>, removed_spaces: &HashSet<String>) -> HcResult<u64>` to `LayoutStore` in `src/layout/store.rs` using `mutate_doc` (CAS Doc API + local fallback) to remove any space in `removed_spaces` or with `!tree.has_terminals()` (including `UNASSIGNED_SPACE`). Add TDD tests in `src/layout/tree.rs` and `tests/layout.rs`.

**Acceptance criteria:**
- [x] `SplitTree::has_terminals()` returns `false` when all leaves have `terminal: None` (or `root` is `None`), and `true` if any leaf holds a `TermRef`.
- [x] `SplitTree::collapse_empty_splits()` collapses a multi-pane empty tree (`!has_terminals() && pane_ids().len() > 1`) into a single `Leaf { id, terminal: None }` and returns `true`, while returning `false` without mutating when at least one terminal remains or when already a single leaf.
- [x] `LayoutStore::prune_empty_spaces` removes all empty spaces (including `UNASSIGNED_SPACE` with empty splits) and any `removed_spaces` via both Doc API (`FakeHcServer`) and local `layout.json` fallback, while retaining spaces that still contain terminal panels.
- [x] Existing `split_close_resize_and_navigate` and `store_and_presence` tests continue to pass.

**Verification:**
- [x] Tests pass: `cargo test --lib layout::tree && cargo test --test layout`
- [x] Build succeeds: `cargo check --all-targets`

**Dependencies:** None

**Files likely touched:**
- `src/layout/tree.rs`
- `src/layout/store.rs`
- `tests/layout.rs`

**Estimated scope:** Medium (3 files)

---

## Checkpoint: After Tasks 1-2
- [x] `cargo test --lib ui::keymap && cargo test --lib layout::tree && cargo test --test layout` pass
- [x] `cargo clippy --all-targets -- -D warnings` passes with no warnings

---

## Phase 2: Application & Event Loop Lifecycle Cleanup

## Task 3: Enforce Empty-Space & Empty-Split Cleanup in `TuiApp`

**Description:** Update `TuiApp` in `src/ui/app.rs` to consistently remove spaces with no terminal panels and collapse empty splits across all interactive state transitions:
- `Command::ClosePane`: remove an empty named space (`cur_space != UNASSIGNED_SPACE && !space_has_open_terminals(&cur_space)`) without emitting `"Cannot close last pane"`, and for `UNASSIGNED_SPACE` when `!current_tree().has_terminals()` collapse empty splits and emit `AppAction::RemoveLayout(UNASSIGNED_SPACE)` instead of `SaveLayout` when `had_multi || closed_addr.is_some()`.
- `Command::Quit`: add and call `pub fn cleanup_empty_spaces_on_quit(&mut self) -> HashSet<String>` which removes every space without terminal panels (`!tree.has_terminals()`, including `UNASSIGNED_SPACE`) from `self.layouts.spaces` and `self.model.declared_spaces` while still returning `vec![AppAction::Quit]`.
- `switch_space(&mut self, space: String) -> Option<String>`: when switching away from `prev != space`, remove `prev` if it has no terminal panels / open terminals and return `Some(prev)` so callers in `handle_key` and `handle_mouse_event` push `AppAction::RemoveLayout(prev)` and refresh the sidebar.
- `prune_terminal_from_spaces(&mut self, addr: &str) -> Vec<String>`: also collapse empty splits via `collapse_empty_splits()` (updating `focused_pane` if needed) and include `UNASSIGNED_SPACE` in the returned emptied list when `!tree.has_terminals()` so `LayoutStore` removes it.
- `prune_empty_spaces(&mut self) -> Vec<String>` & `refresh_sidebar(&mut self)`: prune removed terminals (and terminals whose `labels["space"]` moved elsewhere in `model.terminals`) before rebuilding `declared_spaces` and `sidebar`; remove non-active empty spaces and any active space whose terminals were just pruned (`pruned > 0`), while preserving a newly created active space (`Ctrl+b c` / `initial_space`) when `pruned == 0`.

**Acceptance criteria:**
- [x] Pressing `Ctrl+b q` or `Ctrl+b x` on the last pane of a space removes the space from `layouts.spaces`, `declared_spaces`, and emits `AppAction::RemoveLayout`; in `UNASSIGNED_SPACE`, closing a pane leaving no terminals emits `AppAction::RemoveLayout(UNASSIGNED_SPACE)` and never `SaveLayout`.
- [x] `Ctrl+b c` creates a new empty space that survives `refresh_sidebar()` while active (`pruned == 0`), and switching away from it (via sidebar Enter/click or creating another space) removes it and emits `AppAction::RemoveLayout`.
- [x] `Command::Quit` (`Ctrl+b d`) clears all spaces without terminal panels (including empty `UNASSIGNED_SPACE` split panels and unspawned `Ctrl+b c` spaces) from `layouts.spaces` and `model.declared_spaces` while returning `vec![AppAction::Quit]`.
- [x] `prune_terminal_from_spaces` and `refresh_sidebar` collapse empty multi-pane splits when no terminal panels remain and remove emptied spaces (except a newly created active space with `pruned == 0`).

**Verification:**
- [x] Tests pass: `cargo test --test ui_snapshots`
- [x] Build succeeds: `cargo check --all-targets`

**Dependencies:** Task 1, Task 2

**Files likely touched:**
- `src/ui/app.rs`
- `tests/ui_snapshots.rs`

**Estimated scope:** Small (2 files)

---

## Task 4: Wire Persistence Cleanup in `event_loop.rs` (`AppAction::Quit`, Startup, Core Events, `Missed`, Reconnect, `Spawn`)

**Description:** Update `src/ui/event_loop.rs` to keep `LayoutStore` (`htui.layout` and local fallback) synchronized with `TuiApp` empty-space and empty-split cleanup:
- Extract startup layout reconciliation in `run_tui_loop` into `pub async fn reconcile_startup_layouts(app: &mut TuiApp, rpc: Option<&RpcClient>, store: &LayoutStore, initial_space_name: Option<&str>)`: prune dead terminals from loaded layouts, collapse empty splits on `initial_space_name` and `UNASSIGNED_SPACE`, remove stale empty spaces (plus `UNASSIGNED_SPACE` from `LayoutStore` when `!tree.has_terminals()`), save any remaining non-empty spaces that had dead terminals pruned, preserve `initial_space_name` with 1 empty pane while active (`htui open <space> --create`), ensure `app.active_space` has a valid tree and `focused_pane`, and refresh the sidebar.
- In `dispatch_action(AppAction::Quit, ...)`: call `let removed = app.cleanup_empty_spaces_on_quit();` and `let _ = store.prune_empty_spaces(rpc, &removed).await;` before setting `app.should_quit = true`.
- In `dispatch_action(AppAction::Spawn, ...)`: if `app.switch_space(sp.clone())` returns `Some(removed)`, call `store.remove_space(Some(client), &removed).await`.
- In `handle_core_event`, `EventMessage::Missed`, and reconnect `Snapshot` handling: filter reconnect `snap.terminals` with `.filter(|t| t.is_live())`, call `app.prune_empty_spaces()`, and remove any emptied spaces (including `UNASSIGNED_SPACE` when `!tree.has_terminals()`) from `LayoutStore`.
- Add integration tests in `tests/ui_snapshots.rs` covering `dispatch_action(AppAction::Quit)`, `reconcile_startup_layouts` (stale spaces, empty `UNASSIGNED_SPACE` splits, and `htui open <space> --create`), and `"labels"` / `"finished"` core events pruning empty spaces from `LayoutStore`.

**Acceptance criteria:**
- [x] `dispatch_action(AppAction::Quit, ...)` removes empty named spaces and empty `UNASSIGNED_SPACE` split layouts from both `FakeHcServer` `htui.layout` and local `layout.json` while keeping spaces with terminal panels.
- [x] `reconcile_startup_layouts` removes stale spaces and empty `UNASSIGNED_SPACE` splits from `app` and `LayoutStore`, keeps `initial_space_name` (`--create`) alive with 1 empty pane while active, and ensures `app.focused_pane` points to a valid pane.
- [x] `handle_core_event` (`finished`, `removed`, or `labels` moving the last terminal out of a space), `Kill`, `Remove`, `Missed`, and reconnect remove emptied spaces from `app` and `LayoutStore` and collapse empty splits.
- [x] Full test suite, `cargo fmt`, and `cargo clippy` pass cleanly.

**Verification:**
- [x] Tests pass: `cargo test --all-targets`
- [x] Format & lint succeed: `cargo fmt --all -- --check && cargo clippy --all-targets -- -D warnings`

**Dependencies:** Task 2, Task 3

**Files likely touched:**
- `src/ui/event_loop.rs`
- `tests/ui_snapshots.rs`

**Estimated scope:** Small (2 files)

---

## Checkpoint: Complete
- [x] All acceptance criteria met
- [x] `cargo test --all-targets`, `cargo fmt --all -- --check`, and `cargo clippy --all-targets -- -D warnings` pass cleanly
- [x] Ready for human review
