# Task List: `Command::ClosePane` (`Ctrl+b q` / `Ctrl+b x`) Kills and Removes Terminal in `hc`

---

## Phase 1: `Command::ClosePane` Action & State Cleanup

## Task 1: Update `Command::ClosePane` to Emit `Kill`/`Remove`, Clean `model.terminals`, and Remove `AppAction::UnassignSpace`

**Description:** Using TDD, first update/add unit tests in `tests/ui_snapshots.rs` for `Command::ClosePane` (`Ctrl+b x` / `Ctrl+b q`) covering live, finished, unknown-state, unassigned, and multi-pane split cases, and confirm they fail against the current `AppAction::UnassignSpace` implementation. Then update `Command::ClosePane` in `src/ui/app.rs` to inspect `self.model.terminals.get(&addr)` before removing `addr` from `self.model.terminals`, `self.sessions`, and the split tree: emit `AppAction::Remove(addr)` when `Some(t)` with `!t.is_live()` (already finished), or emit `AppAction::Kill(addr.clone())` + `AppAction::Remove(addr)` when live or absent from `self.model.terminals`. Finally, remove the unused `AppAction::UnassignSpace(String)` variant from `src/ui/app.rs` and its handler arm from `dispatch_action` in `src/ui/event_loop.rs`.

**Acceptance criteria:**
- [x] Closing a pane with a live terminal (`state == "live"`) or unknown-state terminal (absent from `model.terminals`) emits `AppAction::Kill(addr)` followed by `AppAction::Remove(addr)`, and never emits `AppAction::UnassignSpace`.
- [x] Closing a pane with an already-finished terminal (`!t.is_live()`, e.g. `state == "finished"`) emits `AppAction::Remove(addr)` without `AppAction::Kill(addr)`.
- [x] Closing a pane removes `addr` from `app.model.terminals`, `app.sessions`, and the pane tree immediately so `app.refresh_sidebar()` never places `addr` under `(unassigned)`, while continuing to emit `RemoveLayout` when a space becomes empty or `SaveLayout` when other panes remain.
- [x] Unused `AppAction::UnassignSpace` variant and its `dispatch_action` match arm are cleanly removed.
- [x] Updated `close_last_terminal_removes_terminal_and_empty_space`, `close_pane_and_quit_cleanup_empty_spaces_and_unassigned_splits`, and new finished/multi-pane `ClosePane` tests in `tests/ui_snapshots.rs` pass.

**Verification:**
- [x] Tests pass: `cargo test --test ui_snapshots`
- [x] Build succeeds: `cargo check --all-targets`

**Dependencies:** None

**Files likely touched:**
- `src/ui/app.rs`
- `src/ui/event_loop.rs`
- `tests/ui_snapshots.rs`

**Estimated scope:** Small (3 files)

---

## Checkpoint: After Task 1
- [x] `cargo test --test ui_snapshots` and `cargo test --all-targets` pass
- [x] `cargo check --all-targets` succeeds with zero warnings
- [x] Review progress before Phase 2

---

## Phase 2: End-to-End `FakeHcServer` Verification

## Task 2: Enforce `live` Guard in `FakeHcServer` `terminal.rm` and Add Async `ClosePane` Dispatch Integration Test

**Description:** Using TDD, update `FakeHcServer`'s `"terminal.rm"` handler in `tests/support/fake_hc.rs` to return a `-32000` RPC error with `data.kind = "live"` when the terminal is still `"live"` (matching `tests/fixtures/openrpc.json`). Add an async integration test in `tests/ui_snapshots.rs` that spawns live and finished terminals in `FakeHcServer`, saves layouts in `LayoutStore`, invokes `Ctrl+b x` / `Ctrl+b q` (`Command::ClosePane`) on `TuiApp`, runs `dispatch_action` for every emitted `AppAction` against `RpcClient` and `LayoutStore`, and replays resulting `"finished"`/`"removed"` events through `handle_core_event`. Verify that the terminals are killed and removed from `FakeHcServer` (`rpc.terminal_list()`), do not appear in `(unassigned)`, and are cleanly removed from `app.model.terminals`, `app.sessions`, `app.sidebar`, and `LayoutStore` (remote `htui.layout` doc and local `layout.json` fallback) with no error toasts.

**Acceptance criteria:**
- [x] `FakeHcServer` rejects `"terminal.rm"` with `{ "kind": "live" }` if the target terminal in `st.terminals` has `state == "live"`.
- [x] End-to-end `ClosePane` -> `dispatch_action` test against `FakeHcServer` + `LayoutStore` verifies live terminal is killed then removed (`rpc.terminal_list()` no longer contains it, never moves to `(unassigned)`), finished terminal is removed cleanly, `LayoutStore` (remote + local fallback) removes emptied spaces / updates remaining split spaces, and `handle_core_event` on subsequent `"finished"` / `"removed"` notifications is idempotent with no error toast.
- [x] Full test suite, formatter check, and clippy pass with zero warnings.

**Verification:**
- [x] Tests pass: `cargo test --all-targets`
- [x] Format & lint succeed: `cargo fmt --all -- --check && cargo clippy --all-targets -- -D warnings`
- [x] Manual check: Confirm `git diff` / file contents have no unused `UnassignSpace` references or unintended changes to `ConfirmKill` / `ConfirmRemove`.

**Dependencies:** Task 1

**Files likely touched:**
- `tests/support/fake_hc.rs`
- `tests/ui_snapshots.rs`

**Estimated scope:** Small (2 files)

---

## Checkpoint: Complete
- [x] All acceptance criteria met
- [x] `cargo test --all-targets`, `cargo fmt --all -- --check`, and `cargo clippy --all-targets -- -D warnings` pass cleanly
- [x] Ready for human review
