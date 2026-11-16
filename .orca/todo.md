# Task Breakdown: Replace `vt100` with `alacritty_terminal` Behind `ScreenModel`

## Task 1: Add `alacritty_terminal` dependency and implement `AlacrittyScreen` in `src/term/screen.rs`

**Description:** Update `Cargo.toml` to add `alacritty_terminal = { version = "0.26", default-features = false }` and bump `rust-version` from `"1.75"` to `"1.85"` (matching `alacritty_terminal 0.26.0`'s MSRV, while temporarily retaining `vt100` until Task 2 migrates callers and tests). In `src/term/screen.rs`, implement `AlacrittyScreen` backed by `alacritty_terminal::term::Term<NoopEventListener>` and `alacritty_terminal::vte::ansi::Processor`, implementing `ScreenModel`:
1. `NoopEventListener` implementing `alacritty_terminal::event::EventListener` (safely ignoring `Event::PtyWrite`, `Title`, `Bell`, etc. without panicking or writing to stdout).
2. Private `ScreenDims` implementing `alacritty_terminal::grid::Dimensions`, clamping `Term` columns to `cols.max(2)` (`MIN_COLUMNS`) while tracking `logical_cols = cols.max(1)` and `logical_rows = rows.max(1)` so `size()` matches requested dimensions even at 1 column.
3. `process(&mut self, bytes: &[u8])` with stale `CSI ? 2026 h` timeout recovery (`parser.sync_timeout().sync_timeout()` -> `parser.stop_sync(&mut self.term)`), `resize(cols, rows)` (`Term::resize` with automatic soft-wrap reflow on primary screen), `cursor()` (`TermMode::SHOW_CURSOR && display_offset == 0`), `modes()` (`APP_CURSOR`, `BRACKETED_PASTE`, `ALT_SCREEN`, mouse mode/encoding), `scrollback()` / `set_scrollback(offset)` (`Scroll::Delta`), `cell(row, col)` (combining `cell.c` + `cell.zerowidth()`, running control/`Cf`/soft-hyphen/VS sanitization before `preserve_first_grapheme`, mapping `WIDE_CHAR_SPACER | LEADING_WIDE_CHAR_SPACER` to `wide_continuation`, mapping `Color` and `Flags`), `row_text`, and `render_to_buffer`.
Include focused unit tests in `src/term/screen.rs` verifying `REP`, `HVP`/`CNL`/`CPL`/`HPA`/`ECH`, colon-SGR, `cols == 1` wide-char safety, Arabic harakat, `Cf`/VS/ZWJ sanitization, scrollback, cursor visibility, and `Event::PtyWrite` safety.

**Acceptance criteria:**
- [x] `AlacrittyScreen` implements `ScreenModel` in `src/term/screen.rs` with `NoopEventListener` and `cols.max(2)` `Term` clamping + `logical_cols`/`logical_rows` tracking.
- [x] `cell(row, col)` respects `display_offset`, combines `c` + `zerowidth()`, sanitizes control/`Cf`/soft-hyphen/VS before `preserve_first_grapheme`, preserves Arabic harakat, and maps `WIDE_CHAR_SPACER | LEADING_WIDE_CHAR_SPACER`, colors, and modifiers accurately.
- [x] Unit tests in `src/term/screen.rs` pass (`cargo test --lib term::screen`).

**Verification:**
- [x] Tests pass: `cargo test --lib term::screen`
- [x] Build succeeds: `cargo check --all-targets`

**Dependencies:** None

**Files likely touched:**
- `Cargo.toml`
- `src/term/screen.rs`

**Estimated scope:** Medium (2 files)

---

## Task 2: Wire `AlacrittyScreen` into `AttachSession::new`, migrate tests, and remove `vt100`

**Description:** Update `AttachSession::new` in `src/term/session.rs` to construct `AlacrittyScreen::new(cols, rows, 2000)`. Migrate all `Vt100Screen` usages in `tests/term.rs` to `AlacrittyScreen` (updating the mouse protocol test in `tests/term.rs` for `alacritty_terminal`'s modern `?1000h`/`?1002h`/`?1003h`/`?1005h`/`?1006h` support, noting legacy X10 `?9h` is not emitted by `alacritty_terminal`). Remove `Vt100Screen` and its `vt100` helpers from `src/term/screen.rs`, and remove `vt100 = "0.15"` from `Cargo.toml` (refreshing `Cargo.lock`).

**Acceptance criteria:**
- [x] `AttachSession::new` constructs `AlacrittyScreen` and no production or test code references `vt100` or `Vt100Screen`.
- [x] `vt100` is removed from `Cargo.toml` and `Cargo.lock`.
- [x] All existing tests in `--lib`, `tests/term.rs`, `tests/ui_snapshots.rs`, and `tests/live.rs` pass.

**Verification:**
- [x] Tests pass: `cargo test --lib && cargo test --test term --test ui_snapshots --test live`
- [x] Build succeeds: `cargo check --all-targets`

**Dependencies:** Task 1

**Files likely touched:**
- `Cargo.toml`
- `Cargo.lock`
- `src/term/screen.rs`
- `src/term/session.rs`
- `tests/term.rs`

**Estimated scope:** Medium (5 files)

---

## Checkpoint: After Tasks 1-2
- [x] `cargo check --all-targets` passes with zero warnings
- [x] `cargo test --lib` and `cargo test --test term --test ui_snapshots --test live` pass
- [x] `vt100` completely removed from `Cargo.toml` and `Cargo.lock`

---

## Task 3: Add `nvim`/`xterm-256color`, reflow, Arabic, scrollback, and mode regression tests

**Description:** Add comprehensive regression tests in `tests/term.rs` and `tests/ui_snapshots.rs` verifying the exact `nvim` / `xterm-256color` failure modes and all `AlacrittyScreen` invariants:
1. `CSI Ps b` (`REP`) padding: verify `lualine`-style statusline `"1%"` + `REP` spaces + `"20:56"` stays right-aligned at the rightmost columns instead of collapsing next to `1%`.
2. Realistic `nvim` `SIGWINCH` redraw sequence: `CSI ? 1049 h` (alt screen), `CSI ? 2026 h` / `l` (including split-chunk BSU+ESU delivery), `REP`, `HVP` (`f`), `CNL` (`E`), `CPL` (`F`), `HPA` (`` ` ``), `ECH` (`X`), `CHT` (`I`), `CBT` (`Z`), colon-separated SGR (`38:2::R:G:B`, `4:3`), cursor visibility/position, and full overwrite leaving zero stale cells from prior width.
3. Bidirectional soft-wrap reflow on `resize(cols, rows)`: write a long unwrapped line on wide primary screen, shrink width so it wraps across multiple rows, then grow width back and verify lines unwrap/restore cleanly (while alt screen resizes cleanly without bogus reflow).
4. Edge cases & UI integration: Arabic harakat (`"بِ"`, `"لَّ"`, `"شُ"`), wide CJK boundary clamping, `Cf`/soft-hyphen/VS/ZWJ sanitization, `set_scrollback` content + cursor hidden when `scrollback() > 0`, mouse protocols + `APP_CURSOR`/`BRACKETED_PASTE`/`ALT_SCREEN`, and a `tests/ui_snapshots.rs` side-by-side pane test rendering an `nvim`-like `REP` + colon-SGR + `lualine` pane next to a right pane through `SyncCrosstermBackend`.

**Acceptance criteria:**
- [x] `tests/term.rs` verifies `CSI Ps b` (`REP`), `nvim` `SIGWINCH` redraw sequences (`?1049h`, `?2026h/l`, `HVP`/`CNL`/`CPL`/`HPA`/`ECH`/`CHT`/`CBT`, colon-SGR), bidirectional soft-wrap reflow, scrollback, cursor visibility, Arabic harakat, wide CJK clamping, and modes.
- [x] `tests/ui_snapshots.rs` verifies side-by-side `nvim`-like pane rendering with `REP` statusline alignment and intact `│` borders via `render_panes` and `SyncCrosstermBackend`.

**Verification:**
- [x] Tests pass: `cargo test --test term --test ui_snapshots`

**Dependencies:** Task 2

**Files likely touched:**
- `tests/term.rs`
- `tests/ui_snapshots.rs`

**Estimated scope:** Small (2 files)

---

## Task 4: Run full quality gates and reinstall `htui`

**Description:** Run the full workspace test suite, formatter check, and strict clippy check across all targets, then reinstall the updated `htui` binary locally using `cargo install --path . --locked`.

**Acceptance criteria:**
- [x] `cargo test` passes with zero failures or skipped assertions.
- [x] `cargo fmt --all -- --check` passes with zero formatting diffs.
- [x] `cargo clippy --all-targets -- -D warnings` passes with zero warnings.
- [x] `cargo install --path . --locked` succeeds.

**Verification:**
- [x] `cargo test`
- [x] `cargo fmt --all -- --check`
- [x] `cargo clippy --all-targets -- -D warnings`
- [x] `cargo install --path . --locked`

**Dependencies:** Task 3

**Files likely touched:**
- None (verification and local binary install)

**Estimated scope:** XS (0 files)

---

## Checkpoint: Complete
- [x] All acceptance criteria met
- [x] `cargo test`, `cargo fmt --all -- --check`, and `cargo clippy --all-targets -- -D warnings` pass with zero warnings
- [x] Updated `htui` binary installed and ready for manual verification with `nvim` in GNOME Terminal / Ghostty
