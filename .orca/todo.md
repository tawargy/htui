# Task Breakdown: Fix `nvim` Rendering & Resize Corruption in `htui`

## Task 1: Track DECSET 2048 (`in_band_resize`) in `ScreenModes` and `AlacrittyScreen`

**Description:** In `src/term/screen.rs`, add `pub in_band_resize: bool` to `ScreenModes` (default `false`) and add `in_band_resize_primary: bool` and `in_band_resize_alt: bool` fields to `AlacrittyScreen`. Pass mutable references to those flags into `ClearWrapHandler` inside `AlacrittyScreen::process` and intercept `set_private_mode`, `unset_private_mode`, and `reset_state`: (1) `PrivateMode::Unknown(2048)` sets or clears the flag for the active screen (`TermMode::ALT_SCREEN` -> `in_band_resize_alt`, primary -> `in_band_resize_primary`), (2) `NamedPrivateMode::SwapScreenAndSetRestoreCursor` on `unset_private_mode` clears `in_band_resize_alt = false` while leaving `in_band_resize_primary` intact, and (3) `reset_state` (`\x1bc` RIS) clears both flags. In `AlacrittyScreen::modes()`, set `in_band_resize` to `in_band_resize_alt` when `TermMode::ALT_SCREEN` is active and `in_band_resize_primary` otherwise. Keep `NoopEventListener` unchanged so `htui` never duplicates `hc`'s server-side `DA1`/`DSR`/`DECRQM` replies. Add focused unit tests in `src/term/screen.rs`.

**Acceptance criteria:**
- [x] `ScreenModes::default().in_band_resize` is `false`; `\x1b[?2048h` enables `modes().in_band_resize` and `\x1b[?2048l` disables it on both primary and alternate screens.
- [x] Exiting alternate screen (`\x1b[?1049l`) clears alt-screen `in_band_resize` even if `\x1b[?2048l` was omitted, without clobbering primary-screen `in_band_resize`, and `\x1bc` (`RIS`) clears both.
- [x] Replaying an `hc`-style snapshot (`\x18\x1b[!p\x1b[?2048l\x1b[?1049l...\x1b[?1049h...\x1b[?2048h`) ends with `modes().alternate_screen == true` and `modes().in_band_resize == true`.

**Verification:**
- [x] Tests pass: `cargo test --lib term::screen`
- [x] Build succeeds: `cargo check --all-targets`

**Dependencies:** None

**Files likely touched:**
- `src/term/screen.rs`

**Estimated scope:** Small (1 file)

---

## Task 2: Emit `CSI 48 ; rows ; cols ; 0 ; 0 t` in `AttachSession` when Mode 2048 is active

**Description:** In `src/term/session.rs`, add `last_in_band_resize_sent: Option<(u16, u16)>` to `AttachSession` and implement `send_in_band_resize_if_needed(&mut self) -> HcResult<bool>` to send `\x1b[48;{rows};{cols};0;0t` via `ch.send_data(...)` when `self.state == SessionState::Active && self.screen.modes().in_band_resize` and `self.last_in_band_resize_sent != Some((cols, rows))`, recording `Some((cols, rows))`. Wire this cleanly across all `AttachSession` lifecycle and resize paths:
1. `request_resize` and `flush_pending_resize_inner` send `CSI 48` right after `ch.send_resize(cols, rows)?` and `self.screen.resize(cols, rows)`.
2. `handle_frame(FrameKind::Data)` clears `last_in_band_resize_sent = None` when `!self.screen.modes().in_band_resize`, and calls `send_in_band_resize_if_needed()` when `in_band_resize && state == Active` (handling both headless `nvim` snapshots on `attach_active` and live `\x1b[?2048h` transitions).
3. `handle_frame(FrameKind::Resize)` calls `send_in_band_resize_if_needed()` if `state == Active`, and `FrameKind::Close` / `attach_observe` / `release` clear `last_in_band_resize_sent = None` (never emitting `CSI 48` while `Observing`).
4. `attach_active` and `take_over_at` clear `last_in_band_resize_sent = None` and call `send_in_band_resize_if_needed()` so promoting `Observing -> Active` or attaching to an active session syncs size immediately without duplicate sends on snapshot replay.
Add integration tests in `tests/term.rs` against `FakeHcServer`.

**Acceptance criteria:**
- [x] When `in_band_resize` is `true` on an `Active` session, `request_resize`, `flush_pending_resize`, initial snapshot `?2048h` activation (`attach_active`), and `take_over_at` each emit `\x1b[48;{rows};{cols};0;0t` once for `(cols, rows)` without duplicate sends for identical dimensions.
- [x] `Observing` sessions never emit `CSI 48` on snapshot or resize.
- [x] Once `?2048l` or `?1049l` disables `in_band_resize`, subsequent `request_resize` calls emit only `FrameKind::Resize` (never `CSI 48`).

**Verification:**
- [x] Tests pass: `cargo test --lib term::session && cargo test --test term`
- [x] Build succeeds: `cargo check --all-targets`

**Dependencies:** Task 1

**Files likely touched:**
- `src/term/session.rs`
- `tests/term.rs`

**Estimated scope:** Small (2 files)

---

## Checkpoint: After Tasks 1–2
- [x] `cargo check --all-targets` passes with zero warnings
- [x] `cargo test --lib` and `cargo test --test term` pass
- [x] `AttachSession` emits `CSI 48 ; rows ; cols ; 0 ; 0 t` strictly when `SessionState::Active && modes().in_band_resize`

---

## Task 3: Prime startup pane geometry and pass `RunParams.size` on `Spawn`

**Description:** Eliminate startup `(80, 24)` attach bounces and `200x120` headless spawn size mismatches:
1. In `src/hc/types.rs`, add `#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)] pub struct InitialSize { pub cols: u16, pub rows: u16 }` and `#[serde(skip_serializing_if = "Option::is_none")] pub size: Option<InitialSize>` on `RunParams` (`Default` = `None`).
2. In `src/ui/app.rs`, extract `pub fn compute_panes_area(&self, area: Rect) -> Rect` from `TuiApp::render` (single source of truth for sidebar + tabbar + statusbar margins) and add `pub fn predict_spawn_inner_size(&self, target: &SpawnTarget) -> Option<(u16, u16)>` computing the exact inner `(cols, rows)` for `IntoPane`, `NewPanel`, and `SplitPane` when `last_panes_area` is non-empty.
3. In `src/ui/event_loop.rs`:
   - In `run_tui_loop`, before `attach_active_panel_terminals(&mut app, framed.as_ref()).await`, prime `if let Ok(sz) = terminal.size() { app.last_panes_area = app.compute_panes_area(Rect::new(0, 0, sz.width, sz.height)); }`.
   - In `dispatch_action(AppAction::Spawn)`, resolve `effective_target` before `client.terminal_run` and pass `size: app.predict_spawn_inner_size(&effective_target).map(|(cols, rows)| InitialSize { cols, rows })` in `RunParams`.
Add focused unit/client tests in `src/ui/app.rs` and `tests/hc_client.rs` (plus update `FakeHcServer` if needed to record `size` cleanly).

**Acceptance criteria:**
- [x] `TuiApp::render` and `TuiApp::compute_panes_area` share identical layout computation, and `predict_spawn_inner_size` matches post-placement `pane_inner_size` across `IntoPane`, `NewPanel`, and `SplitPane`.
- [x] `run_tui_loop` primes `app.last_panes_area` from `terminal.size()` before `attach_active_panel_terminals` so startup attach uses the real pane size instead of `(80, 24)`.
- [x] `RunParams` serializes `size` only when `Some(InitialSize { .. })`, and `dispatch_action(AppAction::Spawn)` sends the predicted target `(cols, rows)` to `terminal.run`.

**Verification:**
- [x] Tests pass: `cargo test --lib ui::app && cargo test --test hc_client`
- [x] Build succeeds: `cargo check --all-targets`

**Dependencies:** Task 2

**Files likely touched:**
- `src/hc/types.rs`
- `src/ui/app.rs`
- `src/ui/event_loop.rs`
- `tests/hc_client.rs`

**Estimated scope:** Medium (4 files)

---

## Task 4: Add end-to-end `nvim` Mode 2048 shrink/grow, split, and `SyncCrosstermBackend` regressions

**Description:** Add comprehensive regression tests in `tests/term.rs` and `tests/ui_snapshots.rs` modeling the exact `nvim Cargo.toml` Mode 2048 failure mode from `Screenshot From 2026-11-19 13-31-49.png`:
1. In `tests/term.rs`: run a `FakeHcServer` session where `nvim` enables `\x1b[?1049h\x1b[?2048h` and draws `Cargo.toml` lines with `colorcolumn=120` + `bufferline` + `lualine` + git-blame virtual text across attach (`200x120 -> 134x38`), shrink (`134x38 -> 52x33`), grow (`52x33 -> 78x33`), and exit (`?2048l\x1b[?1049l` -> `zsh`). Assert `\x1b[48;38;134;0;0t`, `\x1b[48;33;52;0;0t`, and `\x1b[48;33;78;0;0t` are emitted in order, `nvim` redraws cleanly without truncated col-52 lines or overwritten rows, and post-exit `zsh` resizes do NOT emit `CSI 48`.
2. In `tests/ui_snapshots.rs`: add a side-by-side `nvim` (`?2048h`, Kanagawa RGB background + NerdFont `bufferline`/`lualine`) + `zsh` pane resize storm (`166x42 -> 84x33 -> 110x33`) and horizontal split / divider drag test rendered through `SyncCrosstermBackend`, asserting `CSI 48` synchronization at settled widths, full-width Kanagawa background (no col-52 `Color::Reset` gaps), single-row `lualine`, intact line numbers `9..38`, and intact `│` borders.

**Acceptance criteria:**
- [ ] `tests/term.rs` verifies `CSI 48` emission on attach, shrink, grow, and suppression after `?2048l` / `?1049l` exit back to `zsh`.
- [ ] `tests/ui_snapshots.rs` verifies side-by-side `nvim` + `zsh` panes across outer resize bursts, `[+H]` split, and divider drags through `SyncCrosstermBackend` with intact borders, single-row `lualine`, and full-width `nvim` background/text.

**Verification:**
- [ ] Tests pass: `cargo test --test term --test ui_snapshots`
- [ ] Build succeeds: `cargo check --all-targets`

**Dependencies:** Task 3

**Files likely touched:**
- `tests/term.rs`
- `tests/ui_snapshots.rs`

**Estimated scope:** Small (2 files)

---

## Checkpoint: After Tasks 3–4
- [ ] `cargo test` passes with zero failures or skipped assertions
- [ ] All `nvim` Mode 2048 resize, split, attach, and `SyncCrosstermBackend` regression tests pass

---

## Task 5: Run full quality gates, reinstall `htui`, and verify live `nvim` + `hc` PTY flows

**Description:** Run the full workspace test suite, formatter check, and strict clippy lints across all targets, reinstall `htui` locally with `cargo install --path . --locked`, and run an isolated live `hc` (`HC_SESSION=...`) + `nvim +19 Cargo.toml` + `htui` PTY verification covering: (1) shrinking `166x42 -> 84x33`, moving cursor (`j`), and growing `84x33 -> 110x33` inside `nvim`; (2) attaching `htui` to an already-running `hc run nvim +19 Cargo.toml` (`200x120`); (3) splitting `[+H]` with `zsh` beside `nvim` and resizing; and (4) quitting `nvim` (`:q!`) and resizing `zsh` to verify zero `8;...t` leakage into the shell prompt.

**Acceptance criteria:**
- [ ] `cargo test` passes with zero failures or skipped assertions.
- [ ] `cargo fmt --all -- --check` passes with zero formatting diffs.
- [ ] `cargo clippy --all-targets -- -D warnings` passes with zero warnings.
- [ ] `cargo install --path . --locked` succeeds and the reinstalled `htui` binary passes the isolated live `hc` + `nvim` PTY resize, attach, split, and exit checks.

**Verification:**
- [ ] `cargo test`
- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy --all-targets -- -D warnings`
- [ ] `cargo install --path . --locked`
- [ ] Isolated live `hc` + `nvim +19 Cargo.toml` + `htui` PTY check confirms crisp full-width rendering on resize/attach/split and clean `zsh` post-exit resize

**Dependencies:** Task 4

**Files likely touched:**
- None (verification and local binary install)

**Estimated scope:** XS (0 files)

---

## Checkpoint: Complete
- [ ] All acceptance criteria met
- [ ] `cargo test`, `cargo fmt --all -- --check`, and `cargo clippy --all-targets -- -D warnings` pass with zero warnings
- [ ] Reinstalled `htui` binary verified clean against live `hc` + `nvim`
