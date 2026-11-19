# Task List: Add Comprehensive Root `README.md` for `htui` (`herdr-tui`)

---

## Phase 1: Overview, Setup, CLI & Configuration

## Task 1: Author `README.md` Overview, UI Layout Diagram, Prerequisites, Installation & Quick Start

**Description:** Create `README.md` at the repository root establishing the document structure and covering the top-level user introduction: project title (`htui` / `herdr-tui`), description aligned with `Cargo.toml`, concise feature highlights, an ASCII diagram illustrating the TUI workspace layout (left `Spaces` + `Agents` sidebar, top panel tab bar with `[+H]` `[+V]` `[+ New Terminal]`, split panes with focus border / `[observing]` + `[Take over]` / `[+ New Terminal (Ctrl+B n)]`, and bottom status bar with `[PREFIX]` / toast), prerequisites (`herdr-core` / `hc` verified at `0.1.21` protocol generation `1`, Linux or macOS Unix domain sockets, Rust MSRV `1.85` for source builds), installation instructions (`cargo install --path . --locked` and `cargo build --release`), and quick-start examples (`htui`, `htui open <space> --create`).

**Acceptance criteria:**
- [x] `README.md` exists at the repository root and accurately introduces `htui` (`herdr-tui` v`0.1.0`, `htui` binary).
- [x] Prerequisites accurately state Rust `1.85` MSRV (`Cargo.toml`), `hc` (`herdr-core`, verified against `0.1.21` protocol generation `1`), and Linux/macOS Unix domain socket support.
- [x] ASCII TUI layout diagram matches `src/ui/sidebar.rs`, `src/ui/tabs.rs`, `src/ui/panes.rs`, and `src/ui/statusbar.rs` labels (`Spaces`, `Agents`, `[+ New Space]`, `[+H]`, `[+V]`, `[+ New Terminal]`, `[observing]`, `[Take over]`, `[+ New Terminal (Ctrl+B n)]`, `[PREFIX]`).

**Verification:**
- [x] File exists and renders cleanly as valid Markdown: `test -s README.md`
- [x] Manual cross-check against `Cargo.toml`, `docs/protocol-notes.md`, `src/ui/sidebar.rs`, `src/ui/tabs.rs`, `src/ui/panes.rs`, and `src/ui/statusbar.rs`.

**Dependencies:** None

**Files likely touched:**
- `README.md`

**Estimated scope:** Small (1 file)

---

## Task 2: Document CLI Reference, Exit Codes, Socket Discovery, `hc` Auto-Start & Configuration

**Description:** Extend `README.md` with the complete CLI and configuration reference verified directly against `src/main.rs`, `src/cli/*.rs`, `src/hc/transport.rs`, `src/config.rs`, and `src/layout/store.rs`. Cover:
1. Global flag `--socket <PATH>` and all subcommands (`htui`, `htui ls [--json] [--all] [--space <SPACE>] [--label <K=V>]...`, `htui status [--json]`, `htui events [--json]`, `htui open <SPACE> [--create]`), including `ls` table columns (`MACHINE`, `SPACE`, `ADDRESS`, `NAME`, `STATUS`, `STATE`, `AGE`) and `events` JSON/plain output.
2. Complete exit code table (`0` success / clean `BrokenPipe`, `1` general/RPC/I/O/TUI error, `2` invalid `--label` or CLI syntax, `3` `SocketUnreachable`, `4` missing space in `htui open` without `--create`).
3. Socket discovery precedence (`--socket` → `HTUI_HC_SOCKET` → `[hc].socket` → `$XDG_RUNTIME_DIR/herdr-core/${HC_SESSION:-default}.sock`, Linux `/run/user/<uid>` & `/tmp/herdr-core-<uid>` fallback, temp dir fallback), environment variables (`HTUI_HC_SOCKET`, `HC_SESSION`, `HTUI_HC_BIN`, `SHELL`), and `hc` auto-start behavior (runs `<HTUI_HC_BIN> server start` detached with 50 ms polling up to 5 s only on interactive TUI launch, skips custom sockets, never runs for `ls`/`status`/`events`).
4. Configuration file (`~/.config/herdr-tui/config.toml` / `$XDG_CONFIG_HOME/herdr-tui/config.toml`) with full annotated TOML defaults for `[hc]`, `[ui]` (`show_finished = true`, `show_unassigned = true`, `prefix_timeout_ms = 1500`, `sidebar_width = 30`), and `[attach]` (`mode = "auto"`, noting `"auto"`/`"active"` vs `"observe"` behavior), plus `htui.layout` CAS persistence (`doc.get`/`doc.set`, 3 retries) and `~/.config/herdr-tui/layout.json` fallback.

**Acceptance criteria:**
- [x] Every CLI command, flag, exit code (`0`–`4`), and env var (`HTUI_HC_SOCKET`, `HC_SESSION`, `HTUI_HC_BIN`, `SHELL`) matches `src/main.rs`, `src/cli/*.rs`, `src/hc/transport.rs`, and `src/ui/dialogs/spawn.rs`.
- [x] Socket resolution precedence and `hc` auto-start conditions (interactive launch only, skipped on custom sockets, 5 s / 50 ms polling) are stated accurately without omissions.
- [x] `config.toml` path, all `[hc]` / `[ui]` / `[attach]` keys and exact defaults, and `htui.layout` / `layout.json` persistence match `src/config.rs` and `src/layout/store.rs`.

**Verification:**
- [x] Manual cross-check against `src/main.rs`, `src/cli/mod.rs`, `src/cli/ls.rs`, `src/cli/status.rs`, `src/cli/events.rs`, `src/cli/open.rs`, `src/config.rs`, `src/hc/transport.rs`, and `src/layout/store.rs`.
- [x] Confirm `htui --help` expectations match `src/cli/mod.rs` clap attributes.

**Dependencies:** Task 1

**Files likely touched:**
- `README.md`

**Estimated scope:** Small (1 file)

---

## Checkpoint: After Tasks 1–2
- [x] Top half of `README.md` (Overview, Diagram, Installation, Quick Start, CLI, Socket/Auto-Start, Configuration) is complete and verified against source.
- [x] Zero unverified flags, paths, env vars, or config options exist in `README.md`.

---

## Phase 2: Interactive Controls, Terminal Emulation & Developer Guide

## Task 3: Document Workspaces, Agents, Takeover, Keybindings, Mouse Controls & Dialogs

**Description:** Add the full interactive usage sections to `README.md` verified against `src/model/tree.rs`, `src/layout/panel.rs`, `src/ui/sidebar.rs`, `src/ui/tabs.rs`, `src/ui/panes.rs`, `src/ui/keymap.rs`, `src/ui/mouse.rs`, `src/term/mouse.rs`, `src/ui/selection.rs`, `src/ui/clipboard.rs`, `src/ui/app.rs`, and `src/ui/dialogs/*.rs`:
1. Workspace hierarchy (`Machines → Spaces → Isolated Panels (tabs) → Split Panes`, `(unassigned)` space, automatic empty-space cleanup on close/quit).
2. `Agents` panel detection (`template`, `reported`, `status`, or foreground/argv matching `agy`, `claude`, `codex`, `opencode`, `gemini`, `aider`, `goose`, `cursor`, `pi`), priority sorting (`Blocked`/`Error` → `Working` → `Done` → `Idle` → `Other` → finished), and all 8 status glyphs (`◐`, `⚠`, `✖`, `✔`, `●`, `◈`, `·`, `○`).
3. Observing vs Active session modes (`[observing]` badge, toast warning on input, `[Take over]` button / `Ctrl+B t` promotion at exact pane inner dimensions).
4. Complete `Ctrl+B` prefix keybinding table covering all 17 commands and key aliases (`%`/`|`, `"`/`-`, `H`, `V`, `[`, `]`, `x`/`q`, `h`/`j`/`k`/`l`/arrow keys, `t`, `n`, `c`, `K`, `R`, `r`, `b`, `Tab`, `?`, `d`), plus `Ctrl+Shift+C` selection copy and sidebar navigation (`Up`/`k`, `Down`/`j`, `Left`/`Right`, `Tab`, `Enter`, `Esc`).
5. Mouse interactions: tab bar (`[+H]`, `[+V]`, `[+ New Terminal]`, panel & unattached terminal pills), sidebar clicks, split divider dragging (`±1` cell tolerance, `0.1..=0.9` ratio, `50` ms debounce, immediate flush & layout save on release), pane buttons, child mouse passthrough (X10 / UTF-8 `?1005` / SGR `?1006`, `Shift` bypass), wheel scrollback (`3` lines/tick), text selection & clipboard (`OSC 52` + `wl-copy` / `xclip` / `xsel`), and modal dialogs (Spawn with `hc` templates / `$SHELL` / Task / Space & predicted `InitialSize`, New Space, Rename Space, Confirm Kill/Remove, Help).

**Acceptance criteria:**
- [x] All 17 `PrefixRouter` commands and every character/arrow alias in `src/ui/keymap.rs` are documented accurately alongside `Ctrl+Shift+C` and sidebar keys.
- [x] All 8 status glyphs and agent detection/sorting rules match `src/model/tree.rs` and `src/ui/sidebar.rs`.
- [x] Mouse passthrough, `Shift` selection/scroll override, divider drag behavior, and `OSC 52` + `wl-copy`/`xclip`/`xsel` clipboard pipeline match `src/ui/mouse.rs`, `src/term/mouse.rs`, `src/ui/app.rs`, and `src/ui/clipboard.rs`.

**Verification:**
- [x] Manual cross-check against `src/ui/keymap.rs`, `src/ui/mouse.rs`, `src/ui/sidebar.rs`, `src/ui/tabs.rs`, `src/ui/panes.rs`, `src/ui/clipboard.rs`, `src/ui/dialogs/*.rs`, `src/ui/app.rs`, `src/model/tree.rs`, and `src/term/mouse.rs`.

**Dependencies:** Task 2

**Files likely touched:**
- `README.md`

**Estimated scope:** Small (1 file)

---

## Task 4: Document Terminal Emulation Highlights, Architecture, Testing & License

**Description:** Complete `README.md` with technical and contributor sections verified against `src/term/screen.rs`, `src/term/session.rs`, `src/ui/backend.rs`, `src/ui/event_loop.rs`, `docs/protocol-notes.md`, `tests/`, and `Cargo.toml`:
1. Terminal Emulation & Rendering highlights: `alacritty_terminal` (`0.26`) behind `ScreenModel` with 2000-line scrollback; truecolor RGB & colon-SGR / undercurl attribute mapping; UTF-8 grapheme cluster preservation for Arabic harakat/combining marks and invisible `Cf`/variation-selector sanitization; `SyncCrosstermBackend` Synchronized Output (`\x1b[?2026h` / `\x1b[?2026l`) + explicit `MoveTo` after non-ASCII cells for VTE/GNOME Terminal alignment; DEC Mode 2048 (`?2048h` / `?2048l`) per-screen tracking and `\x1b[48;{rows};{cols};0;0t` synthesis for active Mode 2048 apps like Neovim `0.12+`; resize & startup hygiene (`120` ms outer resize settle, `50` ms divider debounce, startup `last_panes_area` priming, `RunParams.size` `InitialSize` on spawn, primary-screen shrink scrollback reclaim).
2. Architecture & module overview (`src/cli`, `src/hc`, `src/model`, `src/layout`, `src/term`, `src/ui`, `src/config.rs`) with a working relative link to [`docs/protocol-notes.md`](docs/protocol-notes.md).
3. Development & Testing commands (`cargo test`, `HTUI_LIVE_HC=1 cargo test --test live`, `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`) noting `FakeHcServer` and `insta` snapshot tests.
4. License section (`Apache-2.0` per `Cargo.toml`, without linking to nonexistent files).

**Acceptance criteria:**
- [x] Terminal emulation details accurately cite `alacritty_terminal`, 2000-line scrollback, Arabic/grapheme preservation, `SyncCrosstermBackend` (`?2026h`/`l` + `MoveTo`), DEC Mode 2048 (`?2048h`/`l` + `CSI 48 ; rows ; cols ; 0 ; 0 t`), and resize/spawn timing/priming without inaccuracies.
- [x] Project layout, testing commands (`cargo test`, `HTUI_LIVE_HC=1 cargo test --test live`, `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`), and relative link to `docs/protocol-notes.md` are verified accurate.
- [x] License states `Apache-2.0` (`Cargo.toml`) with zero broken relative file links.

**Verification:**
- [x] Check all relative file links in `README.md` resolve to existing files in the repository.
- [x] Run `cargo test`, `cargo fmt --all -- --check`, and `cargo clippy --all-targets -- -D warnings` to verify clean build and test status.

**Dependencies:** Task 3

**Files likely touched:**
- `README.md`

**Estimated scope:** Small (1 file)

---

## Checkpoint: Complete
- [x] Full line-by-line audit of `README.md` against `Cargo.toml`, `src/`, `tests/`, and `docs/protocol-notes.md` passes with zero discrepancies.
- [x] Every relative Markdown link in `README.md` resolves to an existing repository file.
- [x] `cargo test`, `cargo fmt --all -- --check`, and `cargo clippy --all-targets -- -D warnings` pass with zero errors or warnings.
