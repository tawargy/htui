# Task Breakdown: Fix Terminal Text Corruption on Outer Window & Pane Resize

## Task 1: Break stale `WRAPLINE` links on erase-from-column-0 in `AlacrittyScreen::process`

**Description:** In `src/term/screen.rs`, wrap `&mut self.term` in a private `ClearWrapHandler<'a>(&'a mut alacritty_terminal::Term<NoopEventListener>)` implementing `alacritty_terminal::vte::ansi::Handler`, forwarding all `Handler` methods implemented by `Term<NoopEventListener>` and passing `&mut handler` to both `self.parser.stop_sync` and `self.parser.advance` in `AlacrittyScreen::process`. Intercept `clear_line` and `clear_screen` so that whenever a line is erased from column 0 (`LineClearMode::All | Left`, `LineClearMode::Right` when `!cursor.input_needs_wrap && cursor.point.column == Column(0)`, `ClearMode::Below` when `cursor.point.column == Column(0)`, `ClearMode::Above`, or `ClearMode::All`), `Flags::WRAPLINE` is removed from the last column of the preceding line (`line - 1`, including scrollback `Line(-1)` when `line == Line(0)`). Check `.flags.contains(Flags::WRAPLINE)` immutably before taking `&mut` so `Row::occ` is never inflated on rows without `WRAPLINE`. Add focused unit tests in `src/term/screen.rs`.

**Acceptance criteria:**
- [x] `AlacrittyScreen::process` routes both `stop_sync` and `advance` through `ClearWrapHandler`, forwarding all `Term<NoopEventListener>` `Handler` methods accurately.
- [x] Erasing from column 0 via `\r\x1b[J`, `\x1b[2K`, `\x1b[K` (at col 0), or `\x1b[2J` clears `Flags::WRAPLINE` on `line - 1` (including `Line(-1)` in scrollback), while mid-line `\x1b[K` (`col > 0`) and untouched soft-wrapped lines retain `Flags::WRAPLINE`.
- [x] Unit tests in `src/term/screen.rs` verify that shrinking a wrapped `zsh`-like prompt+`RPROMPT` into scrollback, redrawing with `\r\x1b[J`, and growing lines/cols back never merges the old and new prompts onto one row.

**Verification:**
- [x] Tests pass: `cargo test --lib term::screen`
- [x] Build succeeds: `cargo check --all-targets`

**Dependencies:** None

**Files likely touched:**
- `src/term/screen.rs`

**Estimated scope:** Small (1 file)

---

## Task 2: Reclaim spurious scrollback pushes into trailing blank bottom rows in `AlacrittyScreen::resize`

**Description:** In `src/term/screen.rs`, update `AlacrittyScreen::resize(cols, rows)` so that on the primary screen (`!TermMode::ALT_SCREEN`) when shrinking columns (`target_cols < old_term_cols`): (1) resize rows first at `old_term_cols` if height changed, (2) capture `hist_before = grid.history_size()`, `pre_cursor`, and `pre_last_occupied`, (3) run `self.term.resize(target)`, (4) count `trailing_blanks` below `max(cursor_row, last_non_clear_row)` and `pushed = grid.history_size().saturating_sub(hist_before)`, (5) reclaim `reclaim = pushed.min(trailing_blanks)` lines back from scrollback via `self.term.resize(target_rows + reclaim)` followed by `self.term.resize(target_rows)`, restoring `saved_display_offset` if `> 0`, and (6) when `reclaim > 0` and `cursor.point.line == Line(0)` right after step 3, compute the exact un-clamped cursor row via a padded scratch `Grid` over `0..=pre_last_occupied` so the cursor remains on the prompt row rather than drifting onto a wrapped `RPROMPT` continuation row. Update `tests/term.rs::alacritty_screen_bidirectional_soft_wrap_reflow` to assert both blank-bottom-row preservation (`40x10 -> 20x10 -> 80x10`) and full-screen scrollback reflow (`40x2 -> 20x2 -> 80x2`), and add unit tests in `src/term/screen.rs`.

**Acceptance criteria:**
- [x] Shrinking width on a primary screen with blank trailing rows (e.g., `ls` + `zsh` prompt+`RPROMPT` in top rows of a tall pane, or fresh prompt on `Line(0)`) does not push top lines or prompts into scrollback while blank rows remain below.
- [x] Cursor position remains exact on the prompt line (never clamped onto a wrapped `RPROMPT` `✓` row) across multi-step shrink and grow sequences, even when the prompt starts at `Line(0)`.
- [x] `alacritty_screen_bidirectional_soft_wrap_reflow` passes for both blank-bottom preservation and full-screen scrollback reflow, and alt-screen / `cols == 1` invariants remain intact.

**Verification:**
- [x] Tests pass: `cargo test --lib term::screen && cargo test --test term`
- [x] Build succeeds: `cargo check --all-targets`

**Dependencies:** Task 1

**Files likely touched:**
- `src/term/screen.rs`
- `tests/term.rs`

**Estimated scope:** Small (2 files)

---

## Checkpoint: After Tasks 1-2
- [x] `cargo check --all-targets` passes with zero warnings
- [x] `cargo test --lib` and `cargo test --test term` pass
- [x] `AlacrittyScreen` keeps `ls` + `zsh` prompt+`RPROMPT` clean across repeated shrink/grow resizes in isolation

---

## Task 3: Drain buffered frames before resize and coalesce outer `Event::Resize` storms

**Description:** Update `AttachSession`, `TuiApp`, and `run_tui_loop` to prevent stale old-width `DATA` frames from colliding with resized screens and to stop firing `20+` mid-drag `SIGWINCH` cycles during outer terminal window drags:
1. In `src/term/session.rs`, call `self.drain_frames()` at the start of `AttachSession::request_resize` and `AttachSession::flush_pending_resize_inner` before checking state/size or calling `ch.send_resize` / `self.screen.resize`.
2. In `src/ui/app.rs`, add trailing-edge outer resize settle helpers on `TuiApp` (e.g., `OUTER_RESIZE_SETTLE = Duration::from_millis(120)`, `outer_resize_settle_at: Option<Instant>`, `mark_outer_resize(now)`, `clear_outer_resize_settle()`, `has_pending_outer_resize()`, `take_settled_outer_resize(now) -> bool`) while leaving `TuiApp::sync_pane_sizes(now)` synchronous when called directly. Clear `outer_resize_settle_at` on layout-mutating commands (`SplitHorizontal`, `SplitVertical`, `ClosePane`, `ToggleSidebar`) and `EndDrag`.
3. In `src/ui/event_loop.rs`, handle `Event::Resize(_, _)` by calling `app.mark_outer_resize(now)` and setting `app.dirty = true`; in the `if app.dirty` draw block, skip `app.sync_pane_sizes(now)` while `app.has_pending_outer_resize()` is true (so `htui` borders/chrome redraw immediately without mid-drag PTY `RESIZE` storms); in `tick.tick()`, call `sess.drain_frames()` on all sessions first, then if `app.take_settled_outer_resize(now)` call `app.sync_pane_sizes(now)` before/alongside `sess.flush_pending_resize(now)`.
Add focused unit/integration tests for pre-resize frame draining and `TuiApp` outer resize settle behavior.

**Acceptance criteria:**
- [x] Buffered old-width `DATA` frames in `AttachSession::channel` are drained and processed at the old screen dimensions before `request_resize` or `flush_pending_resize` applies `self.screen.resize`.
- [x] Rapid `Event::Resize` events redraw `htui` chrome immediately (`app.dirty = true`) while deferring `sync_pane_sizes` until `OUTER_RESIZE_SETTLE` elapses after the final `Event::Resize`, without delaying startup sizing, `SplitHorizontal`/`SplitVertical`/`ClosePane`/`ToggleSidebar`, `Spawn`, `TakeOver`, or `DragDivider`/`EndDrag`.
- [x] `tick.tick()` drains incoming frames before flushing any pending resize.

**Verification:**
- [x] Tests pass: `cargo test --lib && cargo test --test term`
- [x] Build succeeds: `cargo check --all-targets`

**Dependencies:** Task 2

**Files likely touched:**
- `src/term/session.rs`
- `src/ui/app.rs`
- `src/ui/event_loop.rs`

**Estimated scope:** Medium (3 files)

---

## Task 4: Add end-to-end `zsh` `RPROMPT` shrink/grow and side-by-side resize-storm regression tests

**Description:** Add comprehensive regression tests in `tests/term.rs` and `tests/ui_snapshots.rs` covering the exact failure modes observed in the user screenshot and live `hc` + `zsh` reproduction:
1. In `tests/term.rs`: feed realistic `zsh` + `eza`/`ls` + `RPROMPT` (`✓`) sequences (`\r\r\x1b[0m\x1b[27m\x1b[24m\x1b[J...prompt...\x1b[K\x1b[NC ✓ \x1b[MD`) across multi-step shrink and grow cycles (`138x41 -> ... -> 69x29 -> ... -> 134x39`, plus `Line(0)` shrink+grow, and pre-resize buffered old-width `DATA` drain in `AttachSession`), asserting that `ls` output stays intact, only one prompt exists on screen, zero orphaned `✓` lines remain, and zero prompts concatenate onto the same row or split mid-word.
2. In `tests/ui_snapshots.rs`: add a side-by-side two-pane `zsh` `RPROMPT` + `ls` outer resize storm + divider-drag test through `SyncCrosstermBackend`, asserting intact `│` dividers, single clean prompt per pane, clean `Buffer::diff` / ANSI output, and settled outer resize synchronization.

**Acceptance criteria:**
- [x] `tests/term.rs` fails against the pre-fix code and passes cleanly with the exact `zsh` `RPROMPT` multi-step shrink/grow and buffered-frame-drain scenarios.
- [x] `tests/ui_snapshots.rs` verifies side-by-side `zsh` `RPROMPT` panes across outer resize bursts and divider drags through `SyncCrosstermBackend` with zero divider breakage or duplicated/split prompts.

**Verification:**
- [x] Tests pass: `cargo test --test term --test ui_snapshots`
- [x] Build succeeds: `cargo check --all-targets`

**Dependencies:** Task 3

**Files likely touched:**
- `tests/term.rs`
- `tests/ui_snapshots.rs`

**Estimated scope:** Small (2 files)

---

## Checkpoint: After Tasks 3-4
- [x] `cargo test` passes with zero failures or skipped assertions
- [x] All resize regression tests in `tests/term.rs` and `tests/ui_snapshots.rs` pass

---

## Task 5: Run full quality gates, reinstall `htui`, and verify live PTY resize storm

**Description:** Run the full workspace test suite, formatter check, and strict clippy lints across all targets, reinstall the `htui` binary locally with `cargo install --path . --locked`, and run an isolated `hc` + `zsh` PTY resize-storm smoke test (shrinking and growing both `cols` and `rows` rapidly and slowly, single and side-by-side panes) to confirm zero prompt duplication, zero mid-word splitting, and intact `ls` and `│` borders end-to-end.

**Acceptance criteria:**
- [x] `cargo test` passes with zero failures or skipped assertions.
- [x] `cargo fmt --all -- --check` passes with zero formatting diffs.
- [x] `cargo clippy --all-targets -- -D warnings` passes with zero warnings.
- [x] `cargo install --path . --locked` succeeds and the reinstalled `htui` binary passes the isolated live `hc` + `zsh` PTY resize-storm check.

**Verification:**
- [x] `cargo test`
- [x] `cargo fmt --all -- --check`
- [x] `cargo clippy --all-targets -- -D warnings`
- [x] `cargo install --path . --locked`
- [x] Live isolated `hc` + `zsh` PTY resize-storm check confirms single intact prompt and intact `ls` / `│` borders

**Dependencies:** Task 4

**Files likely touched:**
- None (verification and local binary install)

**Estimated scope:** XS (0 files)

---

## Checkpoint: Complete
- [x] All acceptance criteria met
- [x] `cargo test`, `cargo fmt --all -- --check`, and `cargo clippy --all-targets -- -D warnings` pass with zero warnings
- [x] Reinstalled `htui` binary verified clean on live outer window resize
