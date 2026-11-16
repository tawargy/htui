# Task Breakdown: Fix Side-by-Side Terminal Resize Text Corruption

## Task 1: Harden `Vt100Screen::cell` and `Vt100Screen::render_to_buffer` against boundary, wide, zero-width, and stale-cell corruption

**Description:** Update `src/term/screen.rs` so that `Vt100Screen` never emits symbols or buffer states that desynchronize `ratatui::Buffer::diff` or `CrosstermBackend::draw` across side-by-side pane boundaries. Normalize zero-width and control-character graphemes to `" "` in `Vt100Screen::cell` (while preserving multi-codepoint combining graphemes like Arabic harakat whose base character has width >= 1). In `Vt100Screen::render_to_buffer`, compute `UnicodeWidthStr::width` for every rendered symbol, prevent wide glyphs from overflowing `area.width` (`c + sym_width > area.width`), reset `skip_cols` continuation cells in `buf`, replace unexpected `wide_continuation` cells with styled `" "` instead of skipping via `continue`, and reset any cells in `area` outside `(0..max_cols, 0..max_rows)`. Check `src/ui/panes.rs` to ensure `render_panes` never writes outside each pane's `Rect`.

**Acceptance criteria:**
- [x] `Vt100Screen::cell` replaces any grapheme with `UnicodeWidthStr::width == 0` or `chars().any(char::is_control)` with `" "`, while preserving Arabic harakat clusters (e.g. `"بِ"`, `"لَّ"`, `"شُ"`).
- [x] `Vt100Screen::render_to_buffer` never writes a cell of `UnicodeWidthStr::width > 1` where `c as usize + sym_width > area.width as usize` (substituting `" "` with the cell's style), resets covered continuation cells (`skip_cols`), never leaves stale symbols in `wide_continuation` cells, and resets cells in `area` beyond `(max_cols, max_rows)`.
- [x] Focused tests in `tests/term.rs` and `tests/ui_snapshots.rs` verify wide chars at the rightmost column, orphaned `vt100` wide cells after shrink resize, zero-width/control chars, and Arabic harakat do not alter the adjacent pane's `│` border or `Buffer::diff` column alignment.

**Verification:**
- [x] Tests pass: `cargo test --test term --test ui_snapshots`
- [x] Build succeeds: `cargo check --all-targets`

**Dependencies:** None

**Files likely touched:**
- `src/term/screen.rs`
- `src/ui/panes.rs` (only if border/clipping guard needed; inspect during implementation)
- `tests/term.rs`
- `tests/ui_snapshots.rs`

**Estimated scope:** Small (2-3 files)

---

## Checkpoint: After Task 1
- [x] `cargo test --test term --test ui_snapshots` passes
- [x] `cargo check --all-targets` succeeds without warnings
- [x] Wide/zero-width/Arabic boundary rendering tests pass and preserve adjacent pane borders

---

## Task 2: Synchronize `AttachSession` screen resizing with actual PTY `RESIZE` transmission

**Description:** Refactor `AttachSession` in `src/term/session.rs` so `self.screen` always reflects the true PTY size rather than prematurely resizing during debounced drags or in `Observing` mode. Only call `self.screen.resize(cols, rows)` when `self.state == SessionState::Active` and `ch.send_resize(cols, rows)` is actually sent (in `request_resize` when un-debounced, in `flush_pending_resize` when the 50 ms debounce expires, and in a new `flush_pending_resize_now` method for drag completion). Skip redundant `send_resize` calls when `(cols, rows) == self.screen.size()` (clearing `pending_resize`). In `attach_observe`, initialize `self.screen` to `(res.terminal.cols, res.terminal.rows)` when non-zero. Add `take_over_at(&mut self, client: &FramedClient, cols: u16, rows: u16)` (with `take_over` delegating using `self.screen.size()`) so taking over an observed pane attaches at the pane's current inner size and syncs `self.screen`.

**Acceptance criteria:**
- [x] `AttachSession::request_resize` does NOT resize `self.screen` when `state != SessionState::Active` or when debounced (`can_send == false`), and avoids sending duplicate `RESIZE` frames when size is unchanged.
- [x] `AttachSession::flush_pending_resize` resizes `self.screen` when `ch.send_resize` succeeds, and `flush_pending_resize_now` sends any pending resize immediately (bypassing the 50 ms debounce) and resizes `self.screen`.
- [x] `AttachSession::attach_observe` initializes `self.screen` from non-zero `res.terminal.cols`/`rows`, and `take_over_at` attaches at `(cols, rows)` and resizes `self.screen` to `(cols, rows)`.

**Verification:**
- [x] Tests pass: `cargo test --test term`
- [x] Build succeeds: `cargo check --all-targets`

**Dependencies:** Task 1

**Files likely touched:**
- `src/term/session.rs`
- `tests/term.rs`

**Estimated scope:** Small (2 files)

---

## Task 3: Coordinate `EndDrag`, `sync_pane_sizes`, `tick`, `Spawn`, `TakeOver`, and `"size"` events in `TuiApp` and `event_loop`

**Description:** Update `src/ui/app.rs` and `src/ui/event_loop.rs` so layout changes, divider drags, outer terminal resizes, splits, and take-overs stay synchronized with `AttachSession` and trigger timely redraws. In `src/ui/app.rs`, flush pending resizes immediately on `MouseAction::EndDrag` (via `flush_pending_resize_now`), update `sync_pane_sizes` to skip non-`Active` sessions and set `self.dirty = true` whenever any session's `screen.size()` changes (returning `bool` if any resized), and ensure `DragDivider` / `EndDrag` / `sync_pane_sizes` cooperate cleanly. In `src/ui/event_loop.rs`, set `app.dirty = true` when `tick` flushes a pending resize, call `app.sync_pane_sizes(now)` after `AppAction::Spawn` places a terminal (so a split sibling pane that shrank resizes immediately), update `AppAction::TakeOver` to pass `pane_inner_size` via `take_over_at`, and in `handle_core_event_with_framed` resize `Observing` sessions on `"size"` events when `!sess.resize_notifications`.

**Acceptance criteria:**
- [ ] `MouseAction::EndDrag` flushes pending resizes on active sessions immediately and marks `app.dirty = true` if any flushed.
- [ ] `TuiApp::sync_pane_sizes` skips non-`Active` sessions and marks `self.dirty = true` whenever any `Active` session screen is resized; `tick` sets `app.dirty = true` when `flush_pending_resize` returns `Ok(true)`.
- [ ] `AppAction::Spawn` (including `SplitPane`) syncs sibling pane sizes, `AppAction::TakeOver` uses `pane_inner_size`, and `"size"` events update `Observing` sessions without `resize_notifications`.

**Verification:**
- [ ] Tests pass: `cargo test --test term --test ui_snapshots`
- [ ] Build succeeds: `cargo check --all-targets`

**Dependencies:** Task 2

**Files likely touched:**
- `src/ui/app.rs`
- `src/ui/event_loop.rs`

**Estimated scope:** Small (2 files)

---

## Checkpoint: After Tasks 2-3
- [ ] `cargo test` passes across all unit and integration targets
- [ ] `cargo check --all-targets` succeeds without warnings
- [ ] Divider drag, `EndDrag`, split spawn, outer resize, and `TakeOver` flows all keep PTY and `Vt100Screen` sizes in sync

---

## Task 4: Add comprehensive side-by-side resize regression tests and pass all quality gates

**Description:** Add automated regression tests in `tests/term.rs` and `tests/ui_snapshots.rs` (using `TestBackend`, `Buffer::diff`, and `FakeHcServer`) covering the full bug report: two side-by-side panes resized via divider drag + `EndDrag`, horizontal split spawn shrinking an existing pane, outer terminal resize, observing-mode pane preservation during resize + `TakeOver`, and wide/zero-width/Arabic characters near the pane divider across successive `Buffer::diff` frames. Run the full verification suite (`cargo test`, `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`).

**Acceptance criteria:**
- [ ] `tests/term.rs` verifies `AttachSession` debounce/flush/observing/take-over resize behavior and `Vt100Screen` wide/zero-width/Arabic/resize invariants.
- [ ] `tests/ui_snapshots.rs` verifies side-by-side panes preserve intact `│` borders, adjacent pane text, and valid `Buffer::diff` updates across divider drag, horizontal split spawn, and outer resize.
- [ ] `cargo test`, `cargo fmt --all -- --check`, and `cargo clippy --all-targets -- -D warnings` all pass with zero failures or warnings.

**Verification:**
- [ ] Tests pass: `cargo test`
- [ ] Formatting passes: `cargo fmt --all -- --check`
- [ ] Lints pass: `cargo clippy --all-targets -- -D warnings`

**Dependencies:** Task 3

**Files likely touched:**
- `tests/term.rs`
- `tests/ui_snapshots.rs`

**Estimated scope:** Small (2 files)

---

## Checkpoint: Complete
- [ ] All acceptance criteria met
- [ ] `cargo test`, `cargo fmt --all -- --check`, and `cargo clippy --all-targets -- -D warnings` pass
- [ ] Ready for human review / `build-tasks`
