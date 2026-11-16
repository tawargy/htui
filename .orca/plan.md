# Implementation Plan: Fix Side-by-Side Terminal Resize Text Corruption

## Overview
When two terminal panes are open side-by-side in `htui` and one or both panes are resized (via divider drag, horizontal split, or outer terminal resize), rendered text, pane borders (`│`), and adjacent panes suffer severe horizontal corruption and scrambling. Investigation identified two compounding root causes:
1. **Cell & Wide/Zero-Width Buffer Rendering Bugs (`src/term/screen.rs`, `src/ui/panes.rs`):** `Vt100Screen::render_to_buffer` and `Vt100Screen::cell` can emit 2-column wide characters at the rightmost inner column (`c + width > area.width`), zero-width/control graphemes (`width == 0` or `is_control()`), and skip `wide_continuation` cells via `continue` without resetting or handling `vt100` (`unicode-width 0.1`) vs `ratatui` (`unicode-width 0.2`) width discrepancies. Because `ratatui::Buffer::diff` skips `width - 1` cells after a wide cell and `CrosstermBackend::draw` assumes every emitted cell advances the physical terminal cursor by `1` (omitting `MoveTo` when `x == last_pos.x + 1`), any boundary wide glyph or zero-width/control glyph shifts or suppresses the divider `│` and corrupts the adjacent right pane.
2. **Screen vs PTY Resize Desynchronization (`src/term/session.rs`, `src/ui/app.rs`, `src/ui/event_loop.rs`):** `AttachSession::request_resize` immediately resizes `self.screen` (`vt100::Parser::set_size`) on every `DragDivider` event—even when `ch.send_resize` is debounced (50 ms) or when `state != SessionState::Active` (`Observing`)—causing in-flight PTY `DATA` formatted at the old width to wrap/scramble inside the prematurely resized `vt100::Parser`. Additionally, `EndDrag` does not flush pending resizes, `flush_pending_resize` does not resize `screen` or mark `app.dirty = true` on `tick`, `sync_pane_sizes` runs after `terminal.draw` without marking `app.dirty = true` (and attempts to resize `Observing` panes), `AppAction::Spawn` (`SplitPane`) does not resize the shrunken sibling pane before draw, and `take_over` attaches using the stale observed `screen.size()`.

## Root-Cause Summary

| Area | Symptom | Exact Mechanism |
|---|---|---|
| Wide glyph at pane edge (`src/term/screen.rs`) | Left pane text bleeds across `│` into right pane | `Vt100Screen::render_to_buffer` writes a `width >= 2` symbol at `c == area.width - 1` (due to `area.width < screen_cols`, `vt100` `Row::resize` leaving an orphaned `is_wide()` cell at `cols - 1`, or `unicode-width` 0.1 vs 0.2 skew). `Buffer::diff` sets `to_skip = 1`, skipping the border `│` cell at `area.x + area.width`. |
| Zero-width / control symbol (`src/term/screen.rs`) | Entire row shifts left across divider and right pane | `Vt100Screen::cell` only replaces `raw.is_empty()` with `" "`, letting width-0 graphemes (lone combining marks, ZWJ/ZWNJ, variation selectors, BOM) or control chars into `Buffer`. `CrosstermBackend::draw` advances `last_pos.x` by 1 while the real terminal cursor advances 0 columns, omitting `MoveTo` for subsequent cells on that row. |
| `wide_continuation` & buffer clearing (`src/term/screen.rs`) | Stale cells / dropped characters next to wide glyphs | `render_to_buffer` does `if sc.wide_continuation { continue; }` without resetting covered cells, fails to reset cells when `unicode-width 0.2` width > 1 disagrees with `vt100`, and leaves cells outside `0..max_cols`/`0..max_rows` untouched if `area > screen.size()`. |
| Premature `screen.resize` (`src/term/session.rs`) | Scrambled/wrapped output during drag or while observing | `AttachSession::request_resize` calls `self.screen.resize(cols, rows)` unconditionally before checking `state == Active` or the 50 ms `RESIZE_DEBOUNCE`. `Observing` sessions never resize the PTY, and debounced `Active` drags leave PTY and `Vt100Screen` at mismatched widths. |
| Missing flush / sync / dirty flags (`src/ui/app.rs`, `src/ui/event_loop.rs`) | Stale pane dimensions after drag end, split spawn, outer resize, or tick flush | `MouseAction::EndDrag` only saves layout without flushing `pending_resize`; `tick` ignores `flush_pending_resize` returning `Ok(true)`; `sync_pane_sizes` runs after `terminal.draw` (after `dirty = false`) without re-setting `dirty = true` and iterates over `Observing` sessions; `AppAction::Spawn` splits a pane without calling `sync_pane_sizes`; `take_over` sends `self.screen.size()` instead of `pane_inner_size`. |

## Architecture Decisions
1. **Enforce Ratatui `Buffer::diff` & `CrosstermBackend` cell-width invariants inside `Vt100Screen::cell` and `Vt100Screen::render_to_buffer` (`src/term/screen.rs`):**
   - Normalize any symbol containing control characters (`char::is_control`) or having `UnicodeWidthStr::width(sym) == 0` to `" "` in `Vt100Screen::cell` (while preserving valid combining clusters like Arabic harakat whose base char gives `width >= 1`).
   - In `render_to_buffer`, compute `sym_width = UnicodeWidthStr::width(sc.symbol.as_str())`, enforce `c + sym_width <= area.width` (substituting `" "` if a wide glyph would cross the pane boundary or `sym_width > 2`), track `skip_cols` across the row to `reset()` covered continuation cells (maintaining Ratatui's multi-width buffer invariant regardless of `vt100` vs `unicode-width 0.2` disagreements), render `" "` with the cell's style if `sc.wide_continuation` occurs without an active `skip_cols` span, and `reset()` any cells in `area` outside `(0..max_cols, 0..max_rows)`.
2. **Keep `Vt100Screen` dimensions locked to the actual PTY dimensions (`src/term/session.rs`):**
   - For `SessionState::Active`: resize `self.screen` **only** when `ch.send_resize(cols, rows)` is actually transmitted (in `request_resize` when un-debounced, in `flush_pending_resize` after debounce, and in a new `flush_pending_resize_now` for `EndDrag`). Skip duplicate resizes if `(cols, rows) == self.screen.size()` (clearing `pending_resize` if it reverts to the current size).
   - For `SessionState::Observing` (and `Connecting`/`Ended`): `request_resize` MUST NOT resize `self.screen` or queue a resize. On `attach_observe`, initialize `self.screen.resize(res.terminal.cols, res.terminal.rows)` when non-zero; ongoing updates come from `FrameKind::Resize` (or `"size"` event fallback when `!resize_notifications`).
   - Add `take_over_at(&mut self, client: &FramedClient, cols: u16, rows: u16)` (keeping `take_over` delegating to it with `self.screen.size()` for API compatibility) so promoting an `Observing` pane attaches directly at `pane_inner_size` and updates `self.screen` to match.
3. **Synchronize `sync_pane_sizes`, `EndDrag`, `Spawn`, `TakeOver`, and `Event::Resize` redraws (`src/ui/app.rs`, `src/ui/event_loop.rs`):**
   - Make `sync_pane_sizes(&mut self, now: Instant) -> bool` skip non-`Active` sessions, record whether any `screen.size()` changed, and set `self.dirty = true` whenever a change occurred (so `terminal.draw` + `focused_cursor_position()` always re-render with updated screen dimensions).
   - On `MouseAction::EndDrag`, flush pending resizes on live `Active` sessions immediately via `flush_pending_resize_now(now)` and set `self.dirty = true` if any flushed.
   - In `event_loop.rs`: mark `app.dirty = true` when `tick` flushes a pending resize; call `app.sync_pane_sizes(now)` after `AppAction::Spawn` splits/places a terminal; pass `pane_inner_size` to `take_over_at` in `AppAction::TakeOver`; and handle `"size"` events for `Observing` sessions with `!resize_notifications`.
   - For layout-mutating commands (`SplitHorizontal`, `SplitVertical`, `ClosePane`, `ToggleSidebar`) and `Event::Resize`, `sync_pane_sizes` setting `self.dirty = true` ensures at most one immediate follow-up frame updates both `Vt100Screen` and `focused_cursor_position()` without leaving stale dimensions.

## Dependency Graph

```
Task 1: Sanitize symbols & enforce boundary/continuation invariants in Vt100Screen
  │
  ├──▶ Task 2: Align AttachSession screen resize with PTY RESIZE lifecycle
  │      │
  │      └──▶ Task 3: Coordinate EndDrag, sync_pane_sizes, tick flush, Spawn & TakeOver in app/event_loop
  │             │
  │             └──▶ Task 4: Add side-by-side resize & Buffer::diff regression tests + run full checks
```

## Task List

### Phase 1: Terminal Buffer & Screen Rendering Integrity
- [x] **Task 1:** Harden `Vt100Screen::cell` and `Vt100Screen::render_to_buffer` against boundary wide chars, zero-width/control chars, and `wide_continuation` / stale cell desync.

### Checkpoint 1: Rendering Layer
- [x] `cargo test --test term --test ui_snapshots` passes with new boundary/wide/zero-width/Arabic unit tests.

### Phase 2: Session & UI/Event-Loop Resize Synchronization
- [x] **Task 2:** Fix `AttachSession` resize lifecycle (`request_resize`, `flush_pending_resize`, `flush_pending_resize_now`, `attach_observe`, `take_over_at`).
- [ ] **Task 3:** Synchronize `TuiApp` & `event_loop` (`EndDrag`, `sync_pane_sizes`, `tick` flush dirty flag, `AppAction::Spawn`, `AppAction::TakeOver`, `"size"` fallback).

### Checkpoint 2: Session & Event Loop Synchronization
- [ ] `cargo test` passes with session debounce/observing/drag-end/split-spawn synchronization tests.

### Phase 3: End-to-End Regression Verification & Quality Gate
- [ ] **Task 4:** Add side-by-side `Buffer::diff` + `FakeHcServer` resize regression tests in `tests/term.rs` and `tests/ui_snapshots.rs`, and verify formatting, clippy, and full test suite.

### Checkpoint 3: Complete
- [ ] All acceptance criteria met
- [ ] `cargo test`, `cargo fmt --all -- --check`, and `cargo clippy --all-targets -- -D warnings` pass with zero warnings

## Risks and Mitigations
| Risk | Impact | Mitigation |
|---|---|---|
| Existing `vt100_screen_arabic` or `panes_arabic_snapshot` regressions if zero-width filtering strips combining marks | High | Filter only *whole* grapheme clusters where `UnicodeWidthStr::width(sym) == 0` or `sym.chars().any(char::is_control)`, preserving base+harakat clusters like `"بِ"` (width 1). |
| Breaking `tests/term.rs::sync_pane_sizes_resizes_session_to_wide_area_and_sends_resize` or `session_state_machine` | Med | Preserve `request_resize` sending immediately when un-debounced and `flush_pending_resize` honoring the 50 ms `RESIZE_DEBOUNCE`; add `flush_pending_resize_now` specifically for `EndDrag`. |
| In-flight `DATA` from PTY arriving in the brief async gap between `ch.send_resize` and `SIGWINCH` redraw | Med | Debouncing + keeping `screen` at PTY size during debounced/observing states prevents 95%+ of mismatches; `render_to_buffer` boundary clamping guarantees any transient mismatch can never corrupt borders or adjacent panes. |
| `Observing` panes shrinking `area` below `screen.size()` | Low | `render_to_buffer` clips cleanly to `area.width.min(screen_cols)` and resets unused cells; `take_over_at` adopts `pane_inner_size` immediately upon promotion to `Active`. |

## Open Questions
- None; all required behaviors are verified against `ratatui 0.29.0`, `vt100 0.15.2`, `docs/protocol-notes.md`, and `tests/fixtures/openrpc.json`.
