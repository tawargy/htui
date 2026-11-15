# Task List: Terminal Mouse Pass-Through in `htui`

---

## Phase 1: Terminal Protocol & Mouse Encoding

## Task 1: Expose Mouse Protocol Mode and Encoding on `ScreenModes`

**Description:** Add `MouseProtocolMode` (`None`, `Press`, `PressRelease`, `ButtonMotion`, `AnyMotion`) and `MouseProtocolEncoding` (`Default`, `Utf8`, `Sgr`) enums to `src/term/screen.rs` with `#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]`. Add `pub mouse_mode: MouseProtocolMode` and `pub mouse_encoding: MouseProtocolEncoding` fields to `ScreenModes` and map `vt100::Screen::mouse_protocol_mode()` and `vt100::Screen::mouse_protocol_encoding()` in `Vt100Screen::modes()`. Add TDD tests verifying DECSET/DECRST transitions (`?9h/l`, `?1000h/l`, `?1002h/l`, `?1003h/l`, `?1005h/l`, `?1006h/l`).

**Acceptance criteria:**
- [x] `ScreenModes::default()` has `mouse_mode == MouseProtocolMode::None` and `mouse_encoding == MouseProtocolEncoding::Default`.
- [x] `Vt100Screen::modes()` accurately reflects `vt100::Screen::mouse_protocol_mode()` and `mouse_protocol_encoding()` after processing DECSET/DECRST sequences.
- [x] Tests in `tests/term.rs` verify all four active modes, all three encodings, and resetting back to `None`/`Default`.

**Verification:**
- [x] Tests pass: `cargo test --test term`
- [x] Build succeeds: `cargo check --all-targets`

**Dependencies:** None

**Files likely touched:**
- `src/term/screen.rs`
- `tests/term.rs`

**Estimated scope:** Small (2 files)

---

## Task 2: Implement Terminal Mouse Sequence Encoder (`encode_mouse`)

**Description:** Create `src/term/mouse.rs` (and export `pub mod mouse;` in `src/term/mod.rs`) implementing `pub fn encode_mouse(event: MouseEvent, rel_col: u16, rel_row: u16, modes: ScreenModes) -> Option<Vec<u8>>`. Convert 0-based pane-inner `(rel_col, rel_row)` to 1-based `(cx, cy)`, filter events strictly against `modes.mouse_mode` (`None`, `Press`, `PressRelease`, `ButtonMotion`, `AnyMotion`), compute xterm `cb` (button, motion bit `32`, and `SHIFT=4`/`ALT=8`/`CONTROL=16` modifiers), and format output according to `modes.mouse_encoding` (`Sgr`, `Default`/X10, `Utf8`).

**Acceptance criteria:**
- [x] `encode_mouse` filters events by `modes.mouse_mode` (`Press` allows `Down` only; `PressRelease` adds `Up` and `Scroll*`; `ButtonMotion` adds `Drag`; `AnyMotion` adds `Moved`; `None` returns `None`).
- [x] `Sgr` encodes `CSI < Cb ; Cx ; Cy M` (or `m` for `Up(_)`) with 1-based coordinates (including `> 223`), modifiers, and wheel buttons (`64`..`67`).
- [x] `Default` encodes `[0x1b, b'[', b'M', cb + 32, cx + 32, cy + 32]` using button `3` for `Up(_)` and returns `None` when `cx > 223 || cy > 223`; `Utf8` encodes `\x1b[M` followed by UTF-8 characters for `cb + 32`, `cx + 32`, `cy + 32` (with button `3` for `Up(_)`).
- [x] Unit tests in `src/term/mouse.rs` cover all modes, encodings, buttons, scroll directions, modifiers, and out-of-range X10 coordinates.

**Verification:**
- [x] Tests pass: `cargo test --lib term::mouse`
- [x] Build succeeds: `cargo check --all-targets`

**Dependencies:** Task 1

**Files likely touched:**
- `src/term/mouse.rs`
- `src/term/mod.rs`

**Estimated scope:** Small (2 files)

---

## Checkpoint: After Tasks 1-2
- [x] `cargo test --lib term` and `cargo test --test term` pass
- [x] `cargo clippy --all-targets -- -D warnings` passes with no warnings

---

## Phase 2: UI Hit-Testing, Routing & Dispatch

## Task 3: Add Passthrough Hit-Testing and Active Drag Tracking in `src/ui/mouse.rs`

**Description:** Extend `MouseAction` with `PaneMouse { pane_id: PaneId, rel_col: u16, rel_row: u16 }` and `MouseDragState` with `pub active_passthrough_pane: Option<(PaneId, Rect)>`. Implement `pub fn handle_mouse_with_passthrough<F>(..., should_passthrough: F) -> MouseAction` while keeping `handle_mouse(...)` as a wrapper passing `|_| false`. Preserve `htui` ownership of active divider drags, active selections, `Down(Left)` split dividers, sidebar panels, `takeover_btn`, and `spawn_btn`. When `should_passthrough(pane_id)` is true inside a pane's `inner` rect, emit `MouseAction::PaneMouse` with pane-inner-relative 0-based coordinates, and clamp `Drag`/`Up` to `inner` via `active_passthrough_pane` until `Up`.

**Acceptance criteria:**
- [x] Existing `handle_mouse` callers and `mouse_hit_testing` test continue to pass unchanged.
- [x] Split dividers, sidebar rows, `takeover_btn`, and `spawn_btn` take precedence over pane mouse pass-through on `Down(Left)`.
- [x] `Down(Left | Middle | Right)` inside a passthrough-eligible pane `inner` rect sets `active_passthrough_pane`, clamps subsequent `Drag`/`Up` to `inner` even if the cursor leaves the pane, and clears `active_passthrough_pane` on `Up`.
- [x] `Moved`, `ScrollUp`, `ScrollDown`, `ScrollLeft`, and `ScrollRight` inside a passthrough-eligible pane `inner` rect emit `MouseAction::PaneMouse`; when `should_passthrough` returns `false`, `ScrollUp`/`ScrollDown` fall back to `MouseAction::ScrollUp`/`ScrollDown`.

**Verification:**
- [x] Tests pass: `cargo test --lib ui::mouse && cargo test --test ui_snapshots mouse_hit_testing`
- [x] Build succeeds: `cargo check --all-targets`

**Dependencies:** Task 1

**Files likely touched:**
- `src/ui/mouse.rs`

**Estimated scope:** Small (1 file)

---

## Task 4: Wire `AppAction::SendInput`, Routing Priority, and End-to-End Dispatch

**Description:** Add `AppAction::SendInput { terminal: String, bytes: Vec<u8> }` in `src/ui/app.rs` and handle it in `dispatch_action` in `src/ui/event_loop.rs` via `sess.send_input(&bytes)`. Update `TuiApp::handle_mouse_event` to block background mouse events when `self.dialog.is_some()`, preserve tab bar handling, query passthrough eligibility (`SessionState::Active && mouse_mode != None && !SHIFT`), focus the clicked pane on `MouseEventKind::Down(_)` (clearing `selection` and `sidebar_focused`), and encode/emit `AppAction::SendInput` for `MouseAction::PaneMouse`. Add snapshot/app tests in `tests/ui_snapshots.rs` and a `FakeHcServer` dispatch test in `tests/term.rs`.

**Acceptance criteria:**
- [x] Active panes with `MouseProtocolMode != None` forward clicks, drags, wheel events, and (when `AnyMotion`) motion as `AppAction::SendInput` with 1-based pane-inner coordinates, focusing unfocused panes on `Down(_)`.
- [x] Unrequested events (e.g., `Moved` in `PressRelease` or `ButtonMotion`) are dropped without sending PTY input and without starting `htui` selection.
- [x] `Shift`+click/drag/scroll falls back to `htui` text selection (`CopyToClipboard`) and scrollback (`screen.scrollback()`).
- [x] `Observing` panes never emit `AppAction::SendInput` (preserving focus, selection, scrollback, and `[Take over]` clicks), and open modals (`self.dialog.is_some()`) block mouse pass-through and background clicks.
- [x] `dispatch_action` forwards `AppAction::SendInput` over the framed channel to `FakeHcServer`.

**Verification:**
- [x] Tests pass: `cargo test --all-targets`
- [x] Format & lint succeed: `cargo fmt --all -- --check && cargo clippy --all-targets -- -D warnings`

**Dependencies:** Task 2, Task 3

**Files likely touched:**
- `src/ui/app.rs`
- `src/ui/event_loop.rs`
- `tests/ui_snapshots.rs`
- `tests/term.rs`

**Estimated scope:** Medium (4 files)

---

## Checkpoint: Complete
- [x] All acceptance criteria met
- [x] Full test suite (`cargo test --all-targets`), `cargo fmt --all -- --check`, and `cargo clippy --all-targets -- -D warnings` pass cleanly
- [x] Ready for human review
