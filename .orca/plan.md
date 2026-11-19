# Implementation Plan: Fix Terminal Text Corruption on Outer Window & Pane Resize

## Overview
When resizing the outer terminal window in `htui` while one or more panes run an interactive shell with a right-aligned prompt (`zsh` + `RPROMPT` `✓`), `htui`'s own chrome (sidebar, tab bar, status bar, `│` pane dividers) stays completely intact, while the terminal pane text explodes into dozens of duplicated, horizontally concatenated, and mid-word-wrapped prompt fragments (`ht`/`ui`, `mai`/`n`, `> h`, `✓` embedded mid-line). Empirical probing against `hc 0.1.21` and live `htui` PTY resize storms (`170x45 -> 101x33 -> 134x39`, even at `200ms` per step) proved that `hc`'s own screen remains 100% clean (`terminal.read` shows a single prompt and intact `ls`), and the corruption originates inside `AlacrittyScreen` (`src/term/screen.rs`) and `htui`'s resize/event-loop coordination (`src/term/session.rs`, `src/ui/app.rs`, `src/ui/event_loop.rs`).

Fixing this requires three coordinated layers: (1) clearing stale `Flags::WRAPLINE` links on the preceding line whenever an erase-from-column-0 sequence (`CSI J`, `CSI K`, etc.) resets a line, (2) absorbing unused trailing blank viewport rows before `AlacrittyScreen::resize` pushes wrapped top lines into scrollback history, and (3) draining buffered PTY `DATA` frames before resizing `AlacrittyScreen` while debouncing rapid outer `Event::Resize` storms until the resize burst settles.

## Root-Cause Summary

| Area | Symptom | Exact Mechanism |
|---|---|---|
| Blank bottom rows ignored on shrink (`src/term/screen.rs`, `alacritty_terminal 0.26.0` `Grid::shrink_columns`) | `ls` output scrolls off into history despite 35+ blank rows below cursor; prompts reach `Line(0)` and wrap `✓` into scrollback | `zsh` emits `RPROMPT` (`✓` + colored trailing space) to `col = cols - 1` (`!Cell::is_empty()`). On every width shrink (`W -> W - d`), `AlacrittyScreen::resize` calls `Term::resize` (`Grid::shrink_columns`) **before** `zsh` processes `SIGWINCH`. Because `alacritty_terminal` retains all trailing blank rows at the bottom, every wrap pushes `Line(0)` into scrollback (`Line(-1)`). |
| Dangling `Flags::WRAPLINE` across `\r\x1b[J` prompt redraws (`src/term/screen.rs`, `alacritty_terminal` `Term::clear_screen` / `clear_line`) | Old prompts in scrollback fuse with new prompts into one giant soft-wrapped paragraph; `grow_lines` + `grow_columns` pull them down and concatenate prompts mid-line (`ht`/`ui`, `✓` mid-row) | When `Line(0)` splits into `Line(-1)` (prompt + `WRAPLINE`) and `Line(0)` (`✓`), `zsh` redraws via `\r\r\x1b[0m\x1b[27m\x1b[24m\x1b[J`. `Term::clear_screen(ClearMode::Below)` clears `Line(0)..bottom` but leaves `Flags::WRAPLINE` on `Line(-1)[last_col]` in scrollback. Subsequent `grow_lines` pulls `WRAPLINE`-chained prompts onto the viewport (shifting `cursor.point.line` down so `\r\x1b[J` leaves them above the cursor), and `grow_columns`/`shrink_columns` merges them onto shared rows. |
| `Event::Resize` storm + pre-drain `screen.resize` (`src/ui/event_loop.rs`, `src/term/session.rs`, `src/ui/app.rs`) | 20+ intermediate `SIGWINCH` / wrap cycles during window drag; old-width `DATA` parsed against narrower `screen` strands prompt rows above cursor | `run_tui_loop` triggers `sync_pane_sizes` immediately after `terminal.draw` on `Event::Resize` and `tick.tick()` flushes `pending_resize` at `RESIZE_DEBOUNCE = 50ms` **before** `drain_frames()`. If old-width `CUF` (`\x1b[NC`) + ` ✓ ` + `CUB` (`\x1b[MD`) hits a narrower `screen`, ` ✓ ` wraps to `Line(r + 1)` and `CUB` clamps at `col 0` of `Line(r + 1)`, so the next `\r\x1b[J` leaves `Line(r)` stranded on screen. |

## Architecture Decisions

1. **Break dangling `WRAPLINE` links on erase-from-column-0 via a thin `vte::ansi::Handler` wrapper (`src/term/screen.rs`):**
   - Introduce a private `ClearWrapHandler<'a>(&'a mut alacritty_terminal::Term<NoopEventListener>)` implementing `alacritty_terminal::vte::ansi::Handler` (using a concise forwarding macro for all methods implemented by `Term`), and pass `&mut ClearWrapHandler(&mut self.term)` to both `self.parser.stop_sync` and `self.parser.advance` in `AlacrittyScreen::process`.
   - Intercept `clear_line(mode)` and `clear_screen(mode)`:
     - On `clear_line`: when `mode` is `LineClearMode::All | LineClearMode::Left`, or `LineClearMode::Right` with `!cursor.input_needs_wrap && cursor.point.column == Column(0)`, remove `Flags::WRAPLINE` from `prev_line`'s last column (when `line > grid.topmost_line()`).
     - On `clear_screen`: when `mode` is `ClearMode::Below` with `cursor.point.column == Column(0)`, `ClearMode::Above`, or `ClearMode::All` (after `clear_viewport` scrolls into history, check `Line(0)`'s predecessor `Line(-1)`), remove `Flags::WRAPLINE` from `prev_line`'s last column.
   - Read `grid[prev_line][last_col].flags.contains(Flags::WRAPLINE)` immutably before indexing mutably so `Row::occ` is never inflated on empty rows.

2. **Reclaim spurious scrollback pushes into trailing blank viewport rows in `AlacrittyScreen::resize` (`src/term/screen.rs`):**
   - On the alternate screen (`TermMode::ALT_SCREEN`) or when `target_cols >= old_term_cols`, delegate directly to `self.term.resize(target)`.
   - On the primary screen when `target_cols < old_term_cols`:
     1. Snapshot `saved_display_offset = self.term.grid().display_offset()`.
     2. If `target_rows != old_term_rows`, resize rows first at `old_term_cols` so `hist_before = self.term.grid().history_size()` isolates lines pushed solely by column wrapping.
     3. Snapshot `pre_cursor = self.term.grid().cursor.point` and `pre_last_occupied` (needed only if `cursor` clamps at `Line(0)`), then call `self.term.resize(target)`.
     4. Let `pushed = self.term.grid().history_size().saturating_sub(hist_before)`. Count `trailing_blanks` at the bottom of the viewport strictly below `max(cursor_row, last_non_clear_row)` (via `Row::is_clear()`).
     5. If `reclaim = pushed.min(trailing_blanks) > 0`, temporarily expand viewport lines by `reclaim` (`target_rows + reclaim`) and immediately shrink back to `target_rows` (`Term::resize`). Because `last_occupied + reclaim < target_rows`, `grow_lines` pulls the `reclaim` wrapped lines back onto the visible screen and `shrink_lines` discards only the trailing blank rows.
     6. Edge case guard: if `reclaim > 0` and `cursor.point.line == Line(0)` right after step 3 (meaning `alacritty_terminal`'s `max(line - 1, Line(0))` clamp fired because `Line(0)` wrapped), compute the exact un-clamped cursor row from `0..=pre_last_occupied` on a scratch `Grid` pre-padded with blank top rows (`K` rows so it cannot clamp at `0`) and restore `grid.cursor.point.line` (plus `saved_display_offset` when `> 0`).
   - Update `tests/term.rs::alacritty_screen_bidirectional_soft_wrap_reflow` to verify both blank-bottom-row preservation (`40x10 -> 20x10 -> 80x10` keeps all 3 wrapped rows visible on screen without spurious scrollback) and full-screen scrollback reflow (`40x2 -> 20x2 -> 80x2`).

3. **Drain buffered frames before resize and settle outer `Event::Resize` bursts (`src/term/session.rs`, `src/ui/app.rs`, `src/ui/event_loop.rs`):**
   - In `AttachSession::request_resize` and `AttachSession::flush_pending_resize_inner` (`src/term/session.rs`), call `self.drain_frames()` at entry before checking `self.state` / `self.screen.size()` / resizing so any already-queued old-width `DATA` frames are processed at the old width before `self.screen.resize` runs.
   - Add a trailing-edge outer resize settle state on `TuiApp` (e.g., `OUTER_RESIZE_SETTLE = Duration::from_millis(120)` with `mark_outer_resize(now)`, `clear_outer_resize_settle()`, `take_settled_outer_resize(now) -> bool`, `has_pending_outer_resize() -> bool` so the logic is unit-testable without a TTY):
     - On `Event::Resize(_, _)` (`src/ui/event_loop.rs`): call `app.mark_outer_resize(now)` and set `app.dirty = true` so `htui` chrome/borders (`│`) redraw immediately at the new `area` with zero tearing, while `run_tui_loop`'s `if app.dirty` draw block skips `app.sync_pane_sizes(now)` while `app.has_pending_outer_resize()` is true.
     - On layout-mutating key/mouse actions (e.g. `SplitHorizontal`, `SplitVertical`, `ClosePane`, `ToggleSidebar`, `EndDrag`, `Spawn`, `TakeOver`), clear `outer_resize_settle_at` so `sync_pane_sizes` runs immediately as before (`tests/term.rs` and `tests/ui_snapshots.rs` contracts stay intact).
     - On `tick.tick()`: first drain frames across all sessions (`sess.drain_frames()`, marking `app.dirty = true` if any), then if `app.take_settled_outer_resize(now)` is true, call `app.sync_pane_sizes(now)` (and `flush_pending_resize` on remaining sessions). Preserve `AttachSession::RESIZE_DEBOUNCE = 50ms` for `DragDivider` / `EndDrag`.

## Dependency Graph

```
Task 1: Clear stale WRAPLINE on erase-from-col-0 in AlacrittyScreen::process
  │     (src/term/screen.rs)
  │
  ├──▶ Task 2: Reclaim trailing blank rows on AlacrittyScreen::resize shrink
  │      │     (src/term/screen.rs + tests/term.rs)
  │      │
  │      └──▶ Task 3: Drain frames before resize & settle Event::Resize bursts
  │             │     (src/term/session.rs + src/ui/app.rs + src/ui/event_loop.rs)
  │             │
  │             └──▶ Task 4: Add zsh RPROMPT shrink/grow & side-by-side resize storm regressions
  │                    │     (tests/term.rs + tests/ui_snapshots.rs)
  │                    │
  │                    └──▶ Task 5: Full quality gate, reinstall htui & live PTY verification
```

## Task List

### Phase 1: `AlacrittyScreen` `WRAPLINE` & Resize Integrity
- [x] **Task 1:** Add `ClearWrapHandler` in `src/term/screen.rs` to clear dangling `Flags::WRAPLINE` on `prev_line` (including `Line(-1)`) when `clear_line` or `clear_screen` erases from column 0, with unit tests.
- [x] **Task 2:** Update `AlacrittyScreen::resize` in `src/term/screen.rs` to reclaim spurious column-shrink scrollback pushes into trailing blank bottom rows (with exact `Line(0)` cursor-clamp correction) and update `tests/term.rs::alacritty_screen_bidirectional_soft_wrap_reflow`.

### Checkpoint: After Tasks 1–2
- [x] `cargo check --all-targets`, `cargo test --lib`, and `cargo test --test term` pass without warnings.

### Phase 2: Session Frame-Drain Ordering & Outer `Event::Resize` Settle
- [x] **Task 3:** Drain `ch.rx` frames before resize in `AttachSession` (`src/term/session.rs`), add trailing-edge `OUTER_RESIZE_SETTLE` coordination in `TuiApp` (`src/ui/app.rs`) and `run_tui_loop` (`src/ui/event_loop.rs`), and add unit/integration tests.
- [x] **Task 4:** Add end-to-end `zsh` prompt + `RPROMPT` (`✓`) shrink/grow and side-by-side `Event::Resize` storm regression tests in `tests/term.rs` and `tests/ui_snapshots.rs`.

### Checkpoint: After Tasks 3–4
- [x] `cargo test` passes with all new and existing tests green.

### Phase 3: Full Quality Gate & Live Verification
- [ ] **Task 5:** Run `cargo test`, `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, reinstall `htui` (`cargo install --path . --locked`), and verify via an isolated live `hc` + `zsh` PTY resize-storm test.

### Checkpoint: Complete
- [ ] All acceptance criteria met
- [ ] `cargo test`, `cargo fmt --all -- --check`, and `cargo clippy --all-targets -- -D warnings` pass with zero warnings
- [ ] Reinstalled `htui` binary verified clean against live `hc` + `zsh`

## Risks and Mitigations
| Risk | Impact | Mitigation |
|---|---|---|
| `ClearWrapHandler` missing a `vte::ansi::Handler` method implemented on `Term` (silent no-op via trait default) | High | Verify every method in `impl<T: EventListener> Handler for Term<T>` (`alacritty_terminal 0.26.0/src/term/mod.rs`) is forwarded via macro/explicit methods, and assert existing `term::screen` + `tests/term.rs` sequences (`REP`, `HVP`, `CNL`, `CPL`, `HPA`, `ECH`, `CHT`, `CBT`, `OSC`, `CSI ? 2026 h/l`) still pass. |
| `Line(0)` cursor-clamp correction during blank-row reclamation places cursor on `✓` wrapped row instead of prompt row | High | Explicitly check `post_shrink_cursor_line == 0 && reclaim > 0`, reconstruct exact un-clamped cursor line using a padded scratch `Grid`, and test `Line(0)` shrink+grow with `zsh` prompt+`RPROMPT` directly. |
| `alacritty_screen_bidirectional_soft_wrap_reflow` (`tests/term.rs:1093`) currently asserts `40x10 -> 20x10` pushes row 0 to scrollback despite 8 blank rows below | Med | Update that test to assert both cases: blank-bottom preservation on `40x10 -> 20x10 -> 80x10` (`scrollback == 0`) and full-screen scrollback reflow on `40x2 -> 20x2 -> 80x2` (`scrollback == 1`). |
| Outer `Event::Resize` settle delay (`120ms`) interfering with `app.sync_pane_sizes` in `tests/ui_snapshots.rs` or `SplitHorizontal`/`ToggleSidebar` | Med | Keep `app.sync_pane_sizes(now)` synchronous when called directly; only skip automatic post-draw `sync_pane_sizes` in `run_tui_loop` while `has_pending_outer_resize()` is active, and clear settle state on layout-mutating commands / `EndDrag`. |
| `display_offset > 0` (user scrolled back) distorted by two-step `reclaim` resize (`+reclaim` -> `target_rows`) | Low | Save `saved_display_offset` before reclamation and restore via `self.set_scrollback(saved_display_offset)` if `> 0`. |

## Open Questions
- None; all mechanisms were empirically verified against `hc 0.1.21`, `zsh`, `alacritty_terminal 0.26.0`, and live `htui` PTY resize traces.
