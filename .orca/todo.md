# Task Breakdown: Fix Remaining Side-by-Side Divider Breakage & Cross-Pane Text Bleeding During Drag

## Task 1: Harden `Vt100Screen::cell` against `Cf`, `U+00AD`, variation selectors, and ZWJ clusters

**Description:** Update `src/term/screen.rs` so `Vt100Screen::cell` strips control characters, soft hyphen (`U+00AD`), Unicode `Cf` format characters (e.g. BIDI overrides/isolates `U+202A..=U+202E`, `U+2066..=U+2069`, `U+200B..=U+200F`, `U+FEFF`, etc.), and variation selectors (`U+FE00..=U+FE0F`, `U+E0100..=U+E01EF`) from the raw cell contents *before* extracting the first grapheme via `preserve_first_grapheme`, then falls back to `" "` if the resulting grapheme is empty or `UnicodeWidthStr::width == 0`. Stripping before `preserve_first_grapheme` ensures ZWJ emoji sequences (e.g. `"👨\u{200D}👩"`) collapse to `"👨"` (matching `vt100`'s width) instead of concatenating multiple wide emojis into one cell, and `⚠️` (`U+26A0 U+FE0F`) collapses to `⚠` (width 1 in both `vt100` and `ratatui`), while Arabic harakat combining marks (`Mn`, e.g. `"بِ"`, `"لَّ"`, `"شُ"`) remain preserved. Add focused unit tests in `tests/term.rs`.

**Acceptance criteria:**
- [x] `Vt100Screen::cell` strips `U+00AD`, Unicode `Cf` format characters (`U+202A..=U+202E`, `U+2066..=U+2069`, `U+200B..=U+200F`, `U+FEFF`, etc.), and variation selectors (`U+FE00..=U+FE0F`, `U+E0100..=U+E01EF`) before `preserve_first_grapheme`, normalizing empty or width-0 results to `" "`.
- [x] Multi-codepoint VS16/ZWJ clusters (`"⚠️"`, `"👨‍👩‍👧"`) do not produce width divergence (`UnicodeWidthStr::width` > `vt100` cell width) or consume adjacent cells in `render_to_buffer`.
- [x] Existing and new tests in `tests/term.rs` verify BIDI/format/soft-hyphen/VS16/ZWJ sanitization while keeping Arabic harakat (`"بِ"`, `"لَّ"`, `"شُ"`) intact.

**Verification:**
- [x] Tests pass: `cargo test --test term`
- [x] Build succeeds: `cargo check --all-targets`

**Dependencies:** None

**Files likely touched:**
- `src/term/screen.rs`
- `tests/term.rs`

**Estimated scope:** Small (2 files)

---

## Task 2: Implement `SyncCrosstermBackend<W>` and wire it into `run_tui`

**Description:** Create `src/ui/backend.rs` defining `SyncCrosstermBackend<W: io::Write>` wrapping `ratatui::backend::CrosstermBackend<W>`, export `pub mod backend;` in `src/ui/mod.rs`, and wire `SyncCrosstermBackend::new(io::stdout())` into `run_tui` / `run_tui_loop` in `src/ui/event_loop.rs`. Implement `ratatui::backend::Backend` for `SyncCrosstermBackend<W>` so that:
1. `draw` wraps the frame in `crossterm::terminal::BeginSynchronizedUpdate` (`\x1b[?2026h`) and `EndSynchronizedUpdate` (`\x1b[?2026l`).
2. `draw` queues `MoveTo(x, y)` whenever `prev_non_ascii || !matches!(last_pos, Some(p) if x == p.x + 1 && y == p.y)`, where `prev_non_ascii` is set to `!cell.symbol().is_ascii()` after each emitted cell—forcing explicit cursor repositioning immediately after any non-ASCII glyph while keeping `MoveTo` elision for consecutive ASCII runs.
3. Style, color, underline-color (`ratatui` default feature), and modifier diffing (including `BOLD`/`DIM` `NormalIntensity` interaction) are handled cleanly via a small helper without duplicating verbose chains, and all remaining `Backend` methods (`hide_cursor`, `show_cursor`, `get_cursor_position`, `set_cursor_position`, `clear`, `clear_region`, `append_lines`, `size`, `window_size`, `flush`) delegate to `self.inner`.
Include focused unit tests in `src/ui/backend.rs` using a `Vec<u8>` writer.

**Acceptance criteria:**
- [x] `SyncCrosstermBackend<W: io::Write>` implements `ratatui::backend::Backend` and delegates all lifecycle/cursor/clear/size/flush methods to `CrosstermBackend<W>`.
- [x] `SyncCrosstermBackend::draw` wraps every frame in `\x1b[?2026h` ... `\x1b[?2026l`, emits explicit `MoveTo(x, y)` after every non-ASCII cell (`!cell.symbol().is_ascii()`), and elides `MoveTo` for consecutive ASCII runs.
- [x] `run_tui` and `run_tui_loop` in `src/ui/event_loop.rs` use `SyncCrosstermBackend`.
- [x] Unit tests in `src/ui/backend.rs` verify `MoveTo` after non-ASCII, `MoveTo` elision on ASCII, synchronized update sequences, and modifier/color diffing.

**Verification:**
- [x] Tests pass: `cargo test --lib ui::backend`
- [x] Build succeeds: `cargo check --all-targets`

**Dependencies:** Task 1

**Files likely touched:**
- `src/ui/backend.rs`
- `src/ui/mod.rs`
- `src/ui/event_loop.rs`

**Estimated scope:** Small (3 files)

---

## Checkpoint: After Tasks 1-2
- [x] `cargo check --all-targets` succeeds without warnings
- [x] `cargo test --lib` and `cargo test --test term` pass
- [x] `Vt100Screen::cell` sanitization and `SyncCrosstermBackend` behavior verified independently

---

## Task 3: Add ANSI byte-stream & side-by-side divider-drag integration tests

**Description:** Add integration tests in `tests/term.rs` and `tests/ui_snapshots.rs` exercising `ratatui::Terminal` backed by `SyncCrosstermBackend<SharedVecWriter>` (capturing raw ANSI bytes into `Vec<u8>`) alongside `Vt100Screen`, `TuiApp`, sidebar/tabs, and `MouseAction::DragDivider` / `EndDrag`. Verify that:
- Explicit `MoveTo` ANSI sequences (`\x1b[row;colH`) are emitted immediately after non-ASCII glyphs (`⚠️` sanitized to `⚠`, Nerd Font/PUA e.g. `\u{e0b0}`, `❯`, Arabic harakat, `│`) before the `│` divider and right-pane content on every row during divider dragging.
- Pure ASCII runs across consecutive cells elide redundant `MoveTo` sequences.
- Synchronized output sequences (`\x1b[?2026h` and `\x1b[?2026l`) bracket each rendered frame.
- Format/BIDI/soft-hyphen/VS16 inputs do not corrupt pane dividers or Arabic harakat clusters across drag updates.

**Acceptance criteria:**
- [x] `tests/ui_snapshots.rs` tests `Terminal<SyncCrosstermBackend<_>>` across side-by-side divider drag with Nerd Font/PUA, `⚠️`, `❯`, Arabic, sidebar, and tabs, asserting exact `\x1b[row;colH` emission after non-ASCII cells, ASCII `MoveTo` elision, and `\x1b[?2026h`/`\x1b[?2026l` framing.
- [x] `tests/term.rs` tests `Vt100Screen` format/BIDI/soft-hyphen/VS16/ZWJ sanitization and Arabic harakat preservation end-to-end with `Buffer` and `SyncCrosstermBackend`.

**Verification:**
- [x] Tests pass: `cargo test --test term --test ui_snapshots`

**Dependencies:** Task 2

**Files likely touched:**
- `tests/term.rs`
- `tests/ui_snapshots.rs`

**Estimated scope:** Small (2 files)

---

## Task 4: Run full quality gates and reinstall `htui`

**Description:** Run the complete workspace test suite, formatter check, and strict clippy check across all targets, then reinstall the updated `htui` binary locally with `cargo install --path . --locked`.

**Acceptance criteria:**
- [ ] `cargo test` passes with zero failures or skipped assertions.
- [ ] `cargo fmt --all -- --check` passes with no formatting diffs.
- [ ] `cargo clippy --all-targets -- -D warnings` passes with zero warnings.
- [ ] `cargo install --path . --locked` succeeds.

**Verification:**
- [ ] `cargo test`
- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy --all-targets -- -D warnings`
- [ ] `cargo install --path . --locked`

**Dependencies:** Task 3

**Files likely touched:**
- None (verification and local binary install)

**Estimated scope:** XS (0 files)

---

## Checkpoint: Complete
- [ ] All acceptance criteria met
- [ ] `cargo test`, `cargo fmt --all -- --check`, and `cargo clippy --all-targets -- -D warnings` pass
- [ ] Updated `htui` binary installed and ready for manual verification in Ghostty
