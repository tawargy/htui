# Implementation Plan: Fix Remaining Side-by-Side Divider Breakage & Cross-Pane Text Bleeding During Drag

## Overview
Even with PTY/screen resize synchronization and cell boundary clamping in place, dragging a side-by-side pane divider (`MouseAction::DragDivider`) still exhibits `│` divider breakage and cross-pane text bleeding in real terminal emulators (such as Ghostty). Investigation identified three remaining root causes in the terminal-cell sanitization and ANSI rendering layers:
1. **`CrosstermBackend::draw` cursor-tracking `MoveTo` elision (`ratatui 0.29.0`, `src/backend/crossterm.rs:163`):** `CrosstermBackend::draw` elides `MoveTo(x, y)` whenever consecutive updated cells satisfy `x == last_pos.x + 1 && y == last_pos.y`. Whenever any non-ASCII glyph's rendered width in the physical terminal emulator differs from `unicode-width 0.2.0` (e.g. Nerd Font / PUA icons, ambiguous-width symbols like `❯` or `⚠`, VS16/ZWJ emoji, or UI icons like `▸`/`•`/`│`), the physical cursor drifts horizontally. Because dragging the divider invalidates and re-emits contiguous horizontal spans across the moving `│` border on every row, cursor drift directly breaks `│` and shifts adjacent pane text across panes.
2. **Missing Synchronized Output (`CSI ? 2026 h` / `l`) in `src/ui/event_loop.rs`:** `run_tui` (`src/ui/event_loop.rs:54-56, 193-198`) constructs a raw `CrosstermBackend::new(io::stdout())` without bracketing drawn frames in Synchronized Output (`BeginSynchronizedUpdate` / `EndSynchronizedUpdate`), allowing mid-frame tearing during rapid divider drag updates.
3. **Incomplete invisible/format & variation-selector filtering in `Vt100Screen::cell` (`src/term/screen.rs:107-121`):** `Vt100Screen::cell` currently only filters `char::is_control` and `UnicodeWidthStr::width == 0`. It allows Unicode format characters (`Cf`, e.g., BIDI overrides/isolates `U+202A..U+202E`, `U+2066..U+2069`, `U+200B..U+200F`, `U+FEFF`), soft hyphen (`U+00AD`), and variation selectors / multi-codepoint VS16/ZWJ clusters (e.g. `⚠️` = `U+26A0 U+FE0F`, `👨‍👩‍👧`) whose display width diverges between `vt100` (`unicode-width 0.1.14`), `ratatui` (`unicode-width 0.2.0`), and physical terminals.

## Root-Cause Summary

| Area | Symptom | Exact Mechanism |
|---|---|---|
| `CrosstermBackend::draw` `MoveTo` elision | `│` divider breaks or shifts; text bleeds into adjacent pane during drag | `CrosstermBackend::draw` assumes every emitted cell advances the physical cursor by 1 column (`x == p.x + 1 && y == p.y`). Any non-ASCII cell whose terminal glyph width differs from `unicode-width 0.2.0` desynchronizes all subsequent unpositioned cells on that row. |
| Lack of Synchronized Output (`?2026h`/`?2026l`) | Mid-frame flicker/tearing during rapid `DragDivider` updates | `Terminal::draw` flushes partial chunks to `stdout` mid-frame when output exceeds `Stdout`'s internal buffer, letting the terminal paint a half-moved divider. |
| Format/BIDI/VS15/VS16/ZWJ leakage in `Vt100Screen::cell` (`src/term/screen.rs`) | Width skew between `vt100` (1 col), `ratatui` (2 cols), and terminal; invisible chars corrupting rows | Stripping `char::is_control` and `width == 0` on the final cluster misses `U+00AD`, `Cf` BIDI/format chars appended to base chars or standalone, `VS15`/`VS16` (`⚠️` = 1 col in `vt100` vs 2 cols in `unicode-width 0.2`), and ZWJ clusters (`👨‍👩‍👧`). |

## Architecture Decisions
1. **Sanitize format, soft-hyphen, variation-selector, and ZWJ clusters in `Vt100Screen::cell` (`src/term/screen.rs`):**
   - Add a small helper `is_invisible_format_or_vs(ch: char) -> bool` covering `U+00AD` (soft hyphen), Unicode `Cf` format characters (including `U+0600..=U+0605`, `U+061C`, `U+06DD`, `U+070F`, `U+0890..=U+0891`, `U+08E2`, `U+180E`, `U+200B..=U+200F`, `U+202A..=U+202E`, `U+2060..=U+206F`, `U+FEFF`, `U+FFF9..=U+FFFB`, `U+110BD`, `U+110CD`, `U+13430..=U+1343F`, `U+1BCA0..=U+1BCA3`, `U+1D173..=U+1D17A`, `U+E0001`, `U+E0020..=U+E007F`), and Variation Selectors (`U+FE00..=U+FE0F`, `U+E0100..=U+E01EF`).
   - Strip `c.is_control() || is_invisible_format_or_vs(c)` from `raw` **before** calling `preserve_first_grapheme`. Why before?
     - If ZWJ (`U+200D`) were stripped *after* `preserve_first_grapheme`, `"👨\u{200D}👩"` would become `"👨👩"` (width 4 in `ratatui`!). Stripping first and then taking `preserve_first_grapheme` yields `"👨"` (width 2, matching `vt100`'s `Cell::set` which sets `is_wide` from the first codepoint).
     - `"⚠️"` (`\u{26A0}\u{FE0F}`) becomes `"\u{26A0}"` (width 1 in both `vt100` and `ratatui`), preventing `render_to_buffer`'s `skip_cols` from eating the adjacent column.
     - Standalone `Cf` / `U+00AD` / VS chars become empty and normalize to `" "`.
     - Valid Arabic harakat combining marks (category `Mn`, e.g. `U+064B..=U+065F`, `U+0670`) are **not** in `Cf`/VS/control, so `"بِ"`, `"لَّ"`, and `"شُ"` stay intact.
   - Keep `render_to_buffer`'s existing boundary guard (`c + sym_width <= area.width`), `skip_cols` reset, and `wide_continuation` handling.
2. **Introduce a dedicated `SyncCrosstermBackend<W: io::Write>` in `src/ui/backend.rs`:**
   - Wraps `CrosstermBackend<W>` (`inner: CrosstermBackend<W>`) and implements `ratatui::backend::Backend`.
   - Note on `Cargo.toml` features: `htui`'s `Cargo.toml` specifies `ratatui = "0.29"` (default features `crossterm` + `underline-color` enabled; `htui` itself defines no `underline-color` feature, and `unstable-backend-writer` is not enabled). Because `CrosstermBackend<W>` implements `std::io::Write`, `SyncCrosstermBackend::draw` can `queue!` directly into `self.inner` without needing unstable `writer_mut()` and handles `cell.underline_color` cleanly via `crossterm::style::SetUnderlineColor` (matching `ratatui`'s default `underline-color` feature).
   - **Explicit `MoveTo` after non-ASCII cells:** Track `last_pos: Option<Position>` and `prev_non_ascii: bool`. For each `(x, y, cell)` in `content`, emit `MoveTo(x, y)` whenever `prev_non_ascii || !matches!(last_pos, Some(p) if x == p.x + 1 && y == p.y)`. Pure ASCII consecutive horizontal runs (`prev_non_ascii == false && x == p.x + 1 && y == p.y`) continue to elide `MoveTo(x, y)`. Any non-ASCII cell (`!cell.symbol().is_ascii()` — in pane content, `│` borders, sidebar, or tabs) guarantees the very next cell receives an explicit `MoveTo(x, y)` so width mismatches cannot drift horizontally across `│` or adjacent panes.
   - **Synchronized Output framing:** Emit `crossterm::terminal::BeginSynchronizedUpdate` (`\x1b[?2026h`) at the start of `draw()` and `crossterm::terminal::EndSynchronizedUpdate` (`\x1b[?2026l`) at the end of `draw()` (after resetting colors/attributes), keeping every `draw()` call balanced and self-contained even if called directly or via `Terminal::draw()`.
   - **Clean delegation & small helpers:** Extract a small `queue_modifier_diff` helper using a static `(Modifier, CAttribute)` mapping table rather than duplicating verbose `if removed.contains(...)` chains, and delegate `hide_cursor`, `show_cursor`, `get_cursor_position`, `set_cursor_position`, `clear`, `clear_region`, `append_lines`, `size`, `window_size`, and `flush` directly to `self.inner`.
3. **Wire `SyncCrosstermBackend` into `run_tui` (`src/ui/event_loop.rs`):**
   - Replace `CrosstermBackend::new(io::stdout())` with `SyncCrosstermBackend::new(io::stdout())` and update `run_tui_loop`'s terminal type parameter accordingly.

## Dependency Graph

```
Task 1: Harden Vt100Screen::cell against Cf/soft-hyphen/VS15/VS16/ZWJ clusters
  │     (src/term/screen.rs + tests/term.rs)
  │
  └──▶ Task 2: Add SyncCrosstermBackend<W> & wire into run_tui
         │     (src/ui/backend.rs + src/ui/mod.rs + src/ui/event_loop.rs)
         │
         └──▶ Task 3: Add ANSI byte-stream & divider-drag integration tests
                │     (tests/term.rs + tests/ui_snapshots.rs)
                │
                └──▶ Task 4: Full quality gate & reinstall htui binary
```

## Task List

### Phase 1: Terminal Cell Sanitization
- [x] **Task 1:** Harden `Vt100Screen::cell` against Unicode `Cf` format chars, `U+00AD`, variation selectors (`VS15`/`VS16`), and ZWJ clusters while preserving Arabic harakat.

### Phase 2: Synchronized & Cursor-Safe Terminal Backend
- [ ] **Task 2:** Implement `SyncCrosstermBackend<W>` in `src/ui/backend.rs`, export in `src/ui/mod.rs`, and wire into `run_tui` / `run_tui_loop` in `src/ui/event_loop.rs`.

### Checkpoint 1: Core Implementation
- [ ] `cargo check --all-targets`, `cargo test --lib`, and `cargo test --test term` pass without warnings.

### Phase 3: End-to-End ANSI & Divider-Drag Regression Verification
- [ ] **Task 3:** Add raw ANSI byte-stream (`Vec<u8>`) and side-by-side divider-drag tests in `tests/term.rs` and `tests/ui_snapshots.rs`.
- [ ] **Task 4:** Run full verification gates (`cargo test`, `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`) and reinstall `htui` (`cargo install --path . --locked`).

### Checkpoint 2: Complete
- [ ] All acceptance criteria met
- [ ] `cargo test`, `cargo fmt --all -- --check`, and `cargo clippy --all-targets -- -D warnings` pass with zero warnings
- [ ] Reinstalled `htui` binary verified

## Risks and Mitigations
| Risk | Impact | Mitigation |
|---|---|---|
| Stripping format/VS characters accidentally removes Arabic harakat or base characters | High | Filter only explicit control chars, `Cf` format ranges, `U+00AD`, and variation selectors (`U+FE00..=U+FE0F`, `U+E0100..=U+E01EF`); keep `Mn` combining marks intact and verify against `vt100_screen_arabic` and `panes_arabic_snapshot`. |
| `SyncCrosstermBackend::draw` diverges from `CrosstermBackend::draw` on modifier/underline-color handling | Med | Verify `Modifier` diffing (especially `BOLD`/`DIM` sharing `NormalIntensity`) and `underline_color` resetting against `ratatui 0.29.0`'s `crossterm.rs` with unit tests. |
| Double-emitted `MoveTo` or unbalanced `?2026h`/`?2026l` on empty diff frames | Low | Use a single `if prev_non_ascii || !matches!(last_pos, ...)` condition per cell, and always pair `BeginSynchronizedUpdate` at start of `draw` with `EndSynchronizedUpdate` at end of `draw` (before returning). |
| Tests capturing `Vec<u8>` cannot access private `CrosstermBackend::writer` without `unstable-backend-writer` | Low | Provide a `Clone` shared-buffer writer helper (e.g., `Arc<Mutex<Vec<u8>>>` in tests) or expose `Write`/`into_inner` cleanly; test `SyncCrosstermBackend` directly with `ratatui::Terminal`. |

## Open Questions
- None; `ratatui 0.29.0`, `crossterm 0.28.1`, `vt100 0.15.2`, and `htui`'s `Cargo.toml` features have been verified directly from source.
