# Implementation Plan: Replace `vt100` with `alacritty_terminal` Behind `ScreenModel`

## Overview
While `SyncCrosstermBackend` (`src/ui/backend.rs`) fixed `htui`'s outer `│` pane borders and frame synchronization, full-screen TUI applications like `nvim` inside a pane still suffer severe internal rendering corruption (`lualine` right-aligned `20:56` collapsing next to `1%`, missing lines, truncated titles, mid-line overwrites, and stale scrollbar cells). The root cause is `vt100 0.15.2` (`Vt100Screen` in `src/term/screen.rs`): it silently drops numerous `xterm-256color` CSI/SGR sequences (`REP` `CSI Ps b`, `CNL` `E`, `CPL` `F`, `HVP` `f`, `HPA` `` ` ``, `HPR` `a`, `VPR` `e`, `CHT` `I`, `CBT` `Z`, `NEL`, colon-separated SGR like `38:2::R:G:B` and `4:3`, synchronized updates `CSI ? 2026 h/l`) and truncates lines instead of reflowing soft-wrapped text on resize.

Replacing `Vt100Screen` with `AlacrittyScreen` backed by `alacritty_terminal 0.26.0` behind the existing `ScreenModel` trait (`src/term/screen.rs`) eliminates these parser and resize gaps cleanly without changing any UI rendering or session protocol contracts.

## Root-Cause Summary

| Area | Symptom | Exact Mechanism |
|---|---|---|
| `vt100 0.15.2` missing CSI/SGR sequences | `lualine` `20:56` collapses onto `1%` at col 5; missing lines; truncated/overwritten rows; stale scrollbar cells | `infocmp -1 xterm-256color` advertises `rep=%p1%c\E[%p2%{1}%-%db` (`CSI Ps b`), `CNL`, `CPL`, `HVP`, `HPA`, `CHT`, `CBT`, colon-SGR (`38:2::R:G:B`, `4:3`), etc., which `vt100 0.15.2` ignores or drops. |
| `vt100` resize truncation | Wrapped lines permanently lost/truncated when pane shrinks and re-expands | `vt100` does not track soft-wrap (`WRAPLINE`) or reflow primary-screen lines on `set_size`. |
| Dual `unicode-width` versions | Width divergence risk | `vt100 0.15.2` depends on `unicode-width 0.1.14`, whereas `ratatui 0.29` and `alacritty_terminal 0.26` use `unicode-width 0.2`. |

## Architecture Decisions

1. **Full migration to `AlacrittyScreen` and removal of `vt100`:**
   - Every production caller interacts via `ScreenModel` (`AttachSession::screen: Box<dyn ScreenModel>` in `src/term/session.rs` and `extract_selected_text(screen: &dyn ScreenModel, ...)` in `src/ui/selection.rs`). Only `AttachSession::new` and 6 test functions in `tests/term.rs` reference `Vt100Screen` directly.
   - Completely replace `Vt100Screen` with `AlacrittyScreen` in `src/term/screen.rs`, migrate `tests/term.rs`, and remove `vt100 = "0.15"` from `Cargo.toml` / `Cargo.lock`.
   - **MSRV & feature note:** `alacritty_terminal 0.26.0` specifies `rust-version = "1.85.0"` (edition 2024) and re-exports `vte 0.15.0` (`pub use vte;`). Update `Cargo.toml`'s `rust-version` from `"1.75"` to `"1.85"` (local toolchain is `rustc 1.99.1`) and add `alacritty_terminal = { version = "0.26", default-features = false }` (omits optional `serde` feature on `alacritty_terminal` since `htui` does not serialize `Term`/`Grid`).

2. **Safe `EventListener` and Synchronized Update timeout guard:**
   - Implement a zero-overhead `NoopEventListener` for `alacritty_terminal::event::EventListener` (`fn send_event(&self, _event: Event) {}`) so `Event::PtyWrite`, `Title`, `ResetTitle`, `Bell`, `ColorRequest`, `TextAreaSizeRequest`, `ClipboardStore`, `ClipboardLoad`, `Wakeup`, etc. never panic or leak bytes to `htui`'s own `stdout`.
   - Wrap `Term<NoopEventListener>` and `alacritty_terminal::vte::ansi::Processor` (default `StdSyncHandler`).
   - In `AlacrittyScreen::process(&mut self, bytes: &[u8])`, check `if let Some(deadline) = self.parser.sync_timeout().sync_timeout() { if Instant::now() >= deadline { self.parser.stop_sync(&mut self.term); } }` before `self.parser.advance(&mut self.term, bytes)`. This ensures normal `CSI ? 2026 h` ... `CSI ? 2026 l` frames flush atomically when `ESU` arrives while preventing permanent wedging if a child process exits or drops mid-BSU.

3. **Minimum column safety (`MIN_COLUMNS = 2` vs `cols == 1`):**
   - `alacritty_terminal::term::MIN_COLUMNS` is `2` (`Term::input` panics if a width-2 character is written into a 1-column grid). Meanwhile `TuiApp::pane_inner_size` (`src/ui/app.rs:1127`) clamps to `.max(1)`.
   - Track `logical_cols: u16` (`cols.max(1)`) and `logical_rows: u16` (`rows.max(1)`) on `AlacrittyScreen`, passing `cols.max(2)` to `Term::new` / `Term::resize` via a private `ScreenDims` implementing `alacritty_terminal::grid::Dimensions`.
   - `size()` returns `(self.logical_cols, self.logical_rows)` (so `AttachSession::request_resize` / `TuiApp::sync_pane_sizes` never spin in a resize mismatch loop at 1 column), and `cell(row, col)` returns `None` if `row >= self.logical_rows || col >= self.logical_cols`.

4. **Grid, Scrollback, Sanitization, Color, Modifier, Cursor, and Mode Mapping:**
   - **Scrollback-aware cell indexing:** `let grid = self.term.grid(); let line = Line(row as i32) - grid.display_offset();` (verified against `alacritty_terminal::term::viewport_to_point`), guarded by `line >= grid.topmost_line() && line <= grid.bottommost_line()`.
   - **`set_scrollback(offset)`:** Clamp `target = offset.min(grid.history_size())` and compute signed `delta = (target as isize - grid.display_offset() as isize).clamp(i32::MIN as isize, i32::MAX as isize) as i32`, calling `self.term.scroll_display(Scroll::Delta(delta))`. `scrollback()` returns `self.term.grid().display_offset()`.
   - **Grapheme & sanitization (`cell`):** Concatenate `grid_cell.c` (when `!= '\0'`) and `grid_cell.zerowidth()` chars, strip `c.is_control() || is_invisible_format_or_vs(c)` **before** calling `preserve_first_grapheme`, and normalize empty or `UnicodeWidthStr::width == 0` results to `" "`. Arabic harakat (`Mn` combining marks like `"بِ"`, `"لَّ"`, `"شُ"`) remain preserved while ZWJ/VS/Cf/soft-hyphen/control skew is prevented.
   - **Wide & continuation cells:** Map `grid_cell.flags.intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER)` to `wide_continuation: true`. Keep `render_to_buffer`'s `c + sym_width <= area.width` boundary guard and `skip_cols` reset.
   - **Colors & modifiers:**
     - `Color::Spec(rgb)` -> `ratatui::style::Color::Rgb(rgb.r, rgb.g, rgb.b)`
     - `Color::Indexed(idx)` -> check `colors[idx as usize]` for OSC-4 palette override, else `ratatui::style::Color::Indexed(idx)`
     - `Color::Named(named)` -> check `colors[named]` for OSC override; otherwise map `Black..=White` / `BrightBlack..=BrightWhite` to `Color::Indexed(named as u8)` (0..=15), `DimBlack..=DimWhite` to `Color::Indexed((named as usize - NamedColor::DimBlack as usize) as u8)` with `Modifier::DIM`, and `Foreground | Background | Cursor | BrightForeground | DimForeground` to `Color::Reset`.
     - `Flags`: `BOLD` -> `Modifier::BOLD`, `DIM` -> `Modifier::DIM`, `ITALIC` -> `Modifier::ITALIC`, `ALL_UNDERLINES` (`UNDERLINE | DOUBLE_UNDERLINE | UNDERCURL | DOTTED_UNDERLINE | DASHED_UNDERLINE`) -> `Modifier::UNDERLINED`, `INVERSE` -> `Modifier::REVERSED`, `STRIKEOUT` -> `Modifier::CROSSED_OUT`.
   - **Cursor:** `let display_offset = grid.display_offset(); let visible = self.term.mode().contains(TermMode::SHOW_CURSOR) && display_offset == 0;`, returning `(col.min(logical_cols - 1), row.min(logical_rows - 1), visible)`.
   - **Modes:** Map `TermMode::APP_CURSOR`, `BRACKETED_PASTE`, `ALT_SCREEN`, mouse mode (`MOUSE_MOTION` -> `AnyMotion`, `MOUSE_DRAG` -> `ButtonMotion`, `MOUSE_REPORT_CLICK` -> `PressRelease`, else `None`), and mouse encoding (`SGR_MOUSE` -> `Sgr`, `UTF8_MOUSE` -> `Utf8`, else `Default`). Note: `alacritty_terminal` intentionally ignores legacy X10 `?9h` (`MouseProtocolMode::Press` remains in `MouseProtocolMode` enum for `src/term/mouse.rs` compatibility, and `tests/term.rs` is updated accordingly).

## Dependency Graph

```
Task 1: Add alacritty_terminal 0.26 to Cargo.toml & implement AlacrittyScreen + unit tests
  │     (Cargo.toml + src/term/screen.rs)
  │
  └──▶ Task 2: Wire AlacrittyScreen into AttachSession, migrate tests/term.rs, remove Vt100Screen & vt100 dep
         │     (src/term/session.rs + src/term/screen.rs + Cargo.toml + tests/term.rs)
         │
         └──▶ Task 3: Add nvim/xterm-256color, reflow, Arabic, scrollback & mode regression tests
                │     (tests/term.rs + tests/ui_snapshots.rs)
                │
                └──▶ Task 4: Full quality gate & reinstall htui binary
```

## Task List

### Phase 1: `AlacrittyScreen` Implementation & Wiring
- [x] **Task 1:** Add `alacritty_terminal = "0.26"` (`default-features = false`, update `rust-version = "1.85"`) in `Cargo.toml` and implement `AlacrittyScreen` + `NoopEventListener` + unit tests in `src/term/screen.rs`.
- [ ] **Task 2:** Update `AttachSession::new` (`src/term/session.rs`) to construct `AlacrittyScreen`, migrate `tests/term.rs` from `Vt100Screen` to `AlacrittyScreen`, and remove `Vt100Screen` and `vt100` from `src/term/screen.rs` / `Cargo.toml`.

### Checkpoint 1: Core Migration
- [ ] `cargo check --all-targets`, `cargo test --lib`, and existing integration tests (`cargo test --test term --test ui_snapshots --test live`) pass without warnings.

### Phase 2: Regression Coverage & Release Gate
- [ ] **Task 3:** Add comprehensive regression tests in `tests/term.rs` and `tests/ui_snapshots.rs` covering `REP` (`CSI Ps b`), `nvim` `SIGWINCH` redraw (`?1049h`, `?2026h/l`, `HVP`/`CNL`/`CPL`/`HPA`/`ECH`/`CHT`/`CBT`, colon-SGR `38:2::R:G:B` and `4:3`), bidirectional soft-wrap reflow, Arabic harakat, wide CJK boundary clamping, `Cf`/VS/ZWJ sanitization, scrollback, and mouse/app-cursor modes.
- [ ] **Task 4:** Run full verification gates (`cargo test`, `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`) and reinstall `htui` (`cargo install --path . --locked`).

### Checkpoint 2: Complete
- [ ] All acceptance criteria met
- [ ] `cargo test`, `cargo fmt --all -- --check`, and `cargo clippy --all-targets -- -D warnings` pass with zero warnings
- [ ] Reinstalled `htui` binary verified

## Risks and Mitigations
| Risk | Impact | Mitigation |
|---|---|---|
| `alacritty_terminal 0.26` MSRV (`1.85.0`) exceeds `Cargo.toml` `rust-version = "1.75"` | Med | Update `Cargo.toml` `rust-version = "1.85"` (local toolchain is `1.99.1`); verified in `alacritty_terminal 0.26.0` `Cargo.toml`. |
| `Term` panics when `cols < 2` (`MIN_COLUMNS = 2`) with wide chars | High | Clamp `Term` columns to `cols.max(2)` while tracking `logical_cols = cols.max(1)` for `size()` and bounds checks; test `cols == 1` explicitly. |
| Unterminated `CSI ? 2026 h` buffers indefinitely in `StdSyncHandler` | Med | Check `self.parser.sync_timeout().sync_timeout()` in `AlacrittyScreen::process` and call `self.parser.stop_sync(&mut self.term)` once the 150ms deadline elapses. |
| `alacritty_terminal` drops legacy X10 `?9h` mouse mode | Low | Keep `MouseProtocolMode::Press` in enum for `src/term/mouse.rs`; update `tests/term.rs` mouse protocol test to assert modern `?1000h`/`?1002h`/`?1003h`/`?1005h`/`?1006h` modes (and `?9h` ignored). |
| `NamedColor::Dim*` or `Foreground`/`Background` mapping mismatch | Low | Map `Dim*` to base ANSI 0..=7 with `Modifier::DIM`, and semantic defaults (`Foreground`/`Background`/`Cursor`/`BrightForeground`/`DimForeground`) to `Color::Reset` unless overridden via OSC palette in `term.colors()`. |

## Open Questions
- None; `alacritty_terminal 0.26.0` and `vte 0.15.0` APIs, MSRV, and `htui` call sites have all been verified directly from source.
