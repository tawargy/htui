# Implementation Plan: Fix `nvim` Rendering & Resize Corruption in `htui`

## Overview
When running `nvim` (`v0.12.5`) inside `htui` (`hc 0.1.21`), outer window resizes, pane splits, or re-attaching at a new pane width cause severe visual corruption inside the `nvim` pane while `htui` borders and sibling `zsh` panes remain intact. As seen in [Screenshot From 2026-11-19 13-31-49.png](file:///home/tawargy/Documents/0-WorkSpace/1-SideProjects/htui/Screenshot%20From%202026-11-19%2013-31-49.png), untouched `Cargo.toml` buffer lines and `bufferline` remain truncated at ~52 columns with `Color::Reset` (`#282a36`) across columns 52–77, while updated lines (`9`, `19` with inline git-blame, `27`, `34`) and `lualine` draw at a stale `>= 126`-column width into the 78-column `AlacrittyScreen`, wrapping and overwriting lines `10`, `20`, `28`, and `35` (placing `colorcolumn=120` at col `125 - 78 = 47`) and splitting `lualine` across two rows.

Empirical probing against live `nvim v0.12.5`, `hc 0.1.21`, and `/home/tawargy/.cargo/bin/htui` established the exact mechanism: `hc`'s embedded `libghostty-vt` answers `nvim`'s `DECRQM` Mode 2048 query (`\x1b[?2048$p`) with `\x1b[?2048;2$y`, causing `nvim` to enable DEC Private Mode 2048 (`\x1b[?2048h` — In-Band Window Resize Notifications) and ignore `SIGWINCH`. Because `hc 0.1.21` only updates `TIOCSWINSZ`/`SIGWINCH` on `FrameKind::Resize` and `terminal.attach` without sending `CSI 48 ; rows ; cols ; 0 ; 0 t`, `nvim` never learns the new pane size. Fixing this requires tracking Mode 2048 in `AlacrittyScreen`, emitting `CSI 48 ; rows ; cols ; 0 ; 0 t` from `AttachSession` on active resizes/attaches/takeovers while Mode 2048 is active, and sizing initial startup attaches and `RunParams` spawns cleanly.

## Root-Cause Summary

| Area | Symptom | Exact Mechanism (Empirically Verified) |
|---|---|---|
| Mode 2048 (`?2048h`) ignored (`src/term/screen.rs`, `src/term/session.rs`, `hc 0.1.21`) | `nvim` stays frozen at old width (`~130` or `200x120`) after outer resize, `[+H]`/`[+V]` split, divider drag, or attach/take-over; `AlacrittyScreen` `ALT_SCREEN` crops to `52` cols then pads `52..77` with `Color::Reset`, while subsequent `nvim` line/statusline redraws at old width wrap over adjacent lines | `hc 0.1.21` replies `\x1b[?2048;2$y` to `\x1b[?2048$p`. `nvim` sends `\x1b[?1049h...\x1b[?2048h` and stops handling `SIGWINCH` (`FrameKind::Resize`, `hc attach` `TIOCSWINSZ`, and direct `kill -WINCH` all produce `0` bytes from `nvim`). `hc` never emits `CSI 48 ; rows ; cols ; ... t` on `FrameKind::Resize` or `terminal.attach`. `vte 0.15.0` parses `2048` as `PrivateMode::Unknown(2048)` which `alacritty_terminal 0.26.0` ignores. Sending `\x1b[48;{rows};{cols};0;0t` immediately triggers a full clean `nvim` redraw (`9407`–`14032` bytes). |
| Unsafe if emitted when `?2048h` is off (`src/term/session.rs`, `src/term/screen.rs`) | Blindly sending `CSI 48` to `zsh` injects `8;rows;cols;0;0t` into the shell line editor | Live test confirmed `zsh` ZLE rings BEL and inserts `8;24;80;0;0t` literally at the prompt. `in_band_resize` must be strictly tracked per screen (`?2048h` / `?2048l` / `RIS` / alt-screen exit) and only sent while `SessionState::Active`. |
| Duplicate query replies hazard (`src/term/screen.rs`) | Forwarding `NoopEventListener` `Event::PtyWrite` (`DA1`/`DSR`/`6n`) would corrupt `nvim` input | `hc` already intercepts and answers `\x1b[c`, `\x1b[>q`, `\x1b[18t`, `\x1b[?u`, `\x1b[5n`, `\x1b[6n`, `OSC 10/11`, and `DECRQM` between the PTY and `libghostty-vt`. `AlacrittyScreen` must keep `NoopEventListener` for `PtyWrite` and only synthesize `CSI 48` when Mode 2048 is active. |
| Startup `(80, 24)` bounce & `200x120` spawn (`src/ui/event_loop.rs`, `src/hc/types.rs`, `src/ui/app.rs`) | Initial attach forces `(80, 24)` before first draw; spawned TUIs launch at `200x120` | `run_tui_loop` runs `attach_active_panel_terminals` while `app.last_panes_area` is `0x0` (`pane_inner_size` falls back to `(80, 24)`). `AppAction::Spawn` calls `terminal_run` without `RunParams.size` (`InitialSize`), starting commands at `200x120`. |

## Architecture Decisions

1. **Track DECSET 2048 per screen (`primary` vs `alt`) in `AlacrittyScreen` (`src/term/screen.rs`):**
   - Add `pub in_band_resize: bool` to `ScreenModes` (default `false`).
   - Add `in_band_resize_primary: bool` and `in_band_resize_alt: bool` on `AlacrittyScreen`, passed into `ClearWrapHandler` in `AlacrittyScreen::process`.
   - Intercept `set_private_mode`, `unset_private_mode`, and `reset_state` in `ClearWrapHandler`:
     - `PrivateMode::Unknown(2048)` on `set_private_mode` sets `in_band_resize_alt` when `TermMode::ALT_SCREEN` is active, else `in_band_resize_primary`.
     - `PrivateMode::Unknown(2048)` on `unset_private_mode` clears the active screen's flag (and if `ALT_SCREEN` is active and `!in_band_resize_primary`, keeps primary false; note `nvim` emits `\x1b[?2048l` before `\x1b[?1049l` on both `:q!` and `Ctrl+Z`, and `hc` snapshots reset `\x1b[?2048l` before `\x1b[?1049l`).
     - `NamedPrivateMode::SwapScreenAndSetRestoreCursor` on `unset_private_mode` (leaving `ALT_SCREEN`) clears `in_band_resize_alt = false` so a crashed/unclean alt-screen exit cannot leak Mode 2048 back onto `zsh`, while preserving `in_band_resize_primary` for primary-screen Mode 2048 apps (e.g. `fish`).
     - `reset_state()` (`\x1bc` RIS) clears both flags.
   - `AlacrittyScreen::modes()` populates `in_band_resize` from `in_band_resize_alt` when `TermMode::ALT_SCREEN` is set, else `in_band_resize_primary`.
   - Keep `NoopEventListener` unchanged so `htui` never duplicates `hc`'s `DA1`/`DSR`/`DECRQM` replies.

2. **Synthesize `CSI 48 ; rows ; cols ; 0 ; 0 t` in `AttachSession` while active and Mode 2048 is enabled (`src/term/session.rs`):**
   - Track `last_in_band_resize_sent: Option<(u16, u16)>` on `AttachSession`.
   - Add helper `send_in_band_resize_if_needed(&mut self) -> HcResult<bool>`:
     - Requires `self.state == SessionState::Active && self.screen.modes().in_band_resize`.
     - Let `(cols, rows) = self.screen.size()`. If `self.last_in_band_resize_sent == Some((cols, rows))`, returns `Ok(false)`.
     - Sends `format!("\x1b[48;{rows};{cols};0;0t")` via `ch.send_data(...)` and records `self.last_in_band_resize_sent = Some((cols, rows))`.
   - Call/reset consistently across all session lifecycle points:
     - `attach_observe` / `release`: reset `last_in_band_resize_sent = None` (never emits in `Observing` mode).
     - `attach_active`: reset `last_in_band_resize_sent = None`, then call `send_in_band_resize_if_needed()` (snapshot frames arriving later via `handle_frame(Data)` will also trigger it as soon as `?2048h` is processed).
     - `take_over_at`: reset `last_in_band_resize_sent = None`, resize screen to `(cols, rows)`, then call `send_in_band_resize_if_needed()` (immediately syncs if `Observing` had already seen `?2048h`, and avoids duplicate sends if the new snapshot replays `?2048h` at the same `(cols, rows)`).
     - `request_resize` / `flush_pending_resize_inner`: right after `ch.send_resize(cols, rows)?` and `self.screen.resize(cols, rows)`, call `self.send_in_band_resize_if_needed()?`.
     - `handle_frame(FrameKind::Data)`: after `self.screen.process(&frame.payload)` and credit flow, if `!self.screen.modes().in_band_resize`, clear `self.last_in_band_resize_sent = None`; else if `self.state == SessionState::Active`, call `let _ = self.send_in_band_resize_if_needed();`.
     - `handle_frame(FrameKind::Resize)`: after `self.screen.resize(cols, rows)`, if `self.state == SessionState::Active`, call `let _ = self.send_in_band_resize_if_needed();`.
     - `handle_frame(FrameKind::Close)`: clear `self.last_in_band_resize_sent = None`.

3. **Prime startup pane geometry & send target `InitialSize` on `Spawn` (`src/hc/types.rs`, `src/ui/app.rs`, `src/ui/event_loop.rs`):**
   - Extract `TuiApp::compute_panes_area(&self, area: Rect) -> Rect` from `TuiApp::render` (`src/ui/app.rs`) and add `TuiApp::predict_spawn_inner_size(&self, target: &SpawnTarget) -> Option<(u16, u16)>` using existing `SplitTree::compute_rects` / split ratio logic.
   - In `run_tui_loop` (`src/ui/event_loop.rs`), before `attach_active_panel_terminals`, initialize `if let Ok(sz) = terminal.size() { app.last_panes_area = app.compute_panes_area(Rect::new(0, 0, sz.width, sz.height)); }` so startup attachments use the real pane size instead of falling back to `(80, 24)`.
   - In `src/hc/types.rs`, add `InitialSize { pub cols: u16, pub rows: u16 }` and `#[serde(skip_serializing_if = "Option::is_none")] pub size: Option<InitialSize>` to `RunParams`. Populate `size` in `dispatch_action(AppAction::Spawn)` from `app.predict_spawn_inner_size(&effective_target)` so newly spawned `nvim` instances start directly at their target pane dimensions rather than `200x120`.

## Dependency Graph

```
Task 1: Track DECSET 2048 (in_band_resize) in AlacrittyScreen & ScreenModes
  │     (src/term/screen.rs)
  │
  └──▶ Task 2: Emit CSI 48;rows;cols;0;0t in AttachSession on resize/attach/take-over
         │     (src/term/session.rs + tests/term.rs)
         │
         ├──▶ Task 3: Prime startup panes_area & pass RunParams.size on Spawn
         │      │     (src/hc/types.rs + src/ui/app.rs + src/ui/event_loop.rs)
         │      │
         │      └──▶ Task 4: Add nvim Mode 2048 shrink/grow, split & SyncBackend regressions
         │             │     (tests/term.rs + tests/ui_snapshots.rs)
         │             │
         │             └──▶ Task 5: Quality gates, reinstall htui & live nvim + hc verification
```

## Task List

### Phase 1: Mode 2048 Tracking & In-Band Resize Transport
- [x] **Task 1:** Track DECSET 2048 (`in_band_resize`) in `ScreenModes` and `AlacrittyScreen` (`src/term/screen.rs`) with unit tests covering primary/alt isolation, `?2048l`, `?1049l`, `RIS`, and `hc` snapshot resets.
- [x] **Task 2:** Emit `\x1b[48;{rows};{cols};0;0t` in `AttachSession` (`src/term/session.rs`) on active resize, attach snapshot activation, and `take_over_at`, with `FakeHcServer` integration tests in `tests/term.rs`.

### Checkpoint: After Tasks 1–2
- [x] `cargo check --all-targets`, `cargo test --lib`, and `cargo test --test term` pass with zero warnings.

### Phase 2: Startup & Spawn Sizing + Regression Suite
- [x] **Task 3:** Add `InitialSize` to `RunParams` (`src/hc/types.rs`), extract `compute_panes_area` + `predict_spawn_inner_size` in `TuiApp` (`src/ui/app.rs`), prime `app.last_panes_area` before startup attach, and populate `RunParams.size` in `dispatch_action(Spawn)` (`src/ui/event_loop.rs`), with unit/client tests.
- [ ] **Task 4:** Add end-to-end `nvim` `Cargo.toml` Mode 2048 shrink/grow, `[+H]` split, and `SyncCrosstermBackend` regression tests in `tests/term.rs` and `tests/ui_snapshots.rs`.

### Checkpoint: After Tasks 3–4
- [ ] `cargo test` passes across all workspace tests.

### Phase 3: Full Quality Gate & Live `nvim` Verification
- [ ] **Task 5:** Run `cargo test`, `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, reinstall `htui` (`cargo install --path . --locked`), and run isolated live `hc` + `nvim +19 Cargo.toml` + `htui` PTY resize, attach, split, and `:q!` -> `zsh` verification.

### Checkpoint: Complete
- [ ] All acceptance criteria met
- [ ] `cargo test`, `cargo fmt --all -- --check`, and `cargo clippy --all-targets -- -D warnings` pass cleanly
- [ ] Reinstalled `htui` verified clean on live `nvim` resize, split, attach, and exit

## Risks and Mitigations
| Risk | Impact | Mitigation |
|---|---|---|
| `CSI 48` emitted after an app exits or crashes without `\x1b[?2048l`, injecting `8;rows;cols;0;0t` into `zsh` | High | Track `in_band_resize_primary` and `in_band_resize_alt` separately; clear `in_band_resize_alt` on `?1049l` (`ALT_SCREEN` exit), clear both on `RIS` (`reset_state`), and never send `CSI 48` unless `self.screen.modes().in_band_resize` is `true` while `SessionState::Active`. |
| Duplicate `CSI 48` sent if a future `hc` version also emits `CSI 48` or `take_over_at` overlaps with snapshot replay | Med | Live testing confirmed `nvim v0.12.5` handles identical repeated `\x1b[48;rows;cols;0;0t` cleanly and idempotently; additionally, `last_in_band_resize_sent` prevents `htui` itself from emitting duplicates for the same `(cols, rows)`. |
| `RunParams.size` rejected by older `hc` or mocks | Low | `hc 0.1.21` OpenRPC schema explicitly defines `RunParams.size: Option<InitialSize>`; `skip_serializing_if = "Option::is_none"` preserves wire compatibility for callers with `size: None`, and `FakeHcServer` ignores unknown fields (or can record `size` cleanly). |
| `FakeHcServer` `echo_attach_input: true` echoes `\x1b[48;...t` back into `AlacrittyScreen` during existing tests | Low | Verified `vte 0.15.0` treats `CSI 48 ; ... t` as `unhandled!()` (`src/vte/ansi.rs:1739-1745`), leaving screen state untouched; verify in `tests/term.rs`. |
| `TuiApp::compute_panes_area` or `predict_spawn_inner_size` diverges from `TuiApp::render` layout constraints | Med | `TuiApp::render` calls `self.compute_panes_area(area)` directly so there is a single source of truth for layout math. |

## Open Questions
- None; all mechanisms and edge cases were empirically verified against `nvim v0.12.5`, `hc 0.1.21`, `alacritty_terminal 0.26.0`, and live `/home/tawargy/.cargo/bin/htui` PTY traces.
