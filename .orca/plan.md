# Implementation Plan: Add Comprehensive Root `README.md` for `htui` (`herdr-tui`)

## Overview
Create a comprehensive, source-verified `README.md` at the repository root documenting `htui` (`herdr-tui`), a keyboard-driven terminal multiplexer and client for `herdr-core` (`hc`). Because `README.md` currently does not exist, users and contributors lack a single entry point explaining installation, `hc` daemon discovery/auto-start, CLI subcommands and exit codes, TOML configuration and `htui.layout` persistence, spaces/panels/split-pane workflows, `Ctrl+B` keybindings, SGR mouse passthrough/selection/clipboard behavior, terminal emulation guarantees (Arabic/grapheme safety, `SyncCrosstermBackend`, DEC Mode 2048), and testing workflows. Every documented fact will be strictly verified against the current Rust source, `Cargo.toml`, and `docs/protocol-notes.md`.

---

## Architecture & Content Decisions

1. **Source Code Is the Single Source of Truth (Ignore Stale Draft Specs):**
   - *Rationale:* `.orca/SPEC.md` reflects earlier design drafts (such as `vt100`/`tui-term` and unconditional `observe: true` initial attach) that were superseded by later implementations (`alacritty_terminal`, isolated multi-panel workspaces, `AttachMode`, DEC Mode 2048, and `SyncCrosstermBackend`). All CLI flags, defaults, keybindings, glyphs, env vars, and behaviors in `README.md` must match `src/`, `Cargo.toml`, and `docs/protocol-notes.md` verbatim.
2. **Accurate, Non-Overclaiming Documentation of `AttachMode` and `Apache-2.0` License:**
   - *Rationale:* `src/config.rs` defines `attach.mode` (`"auto"` | `"active"` | `"observe"`, default `"auto"`), while `src/ui/event_loop.rs` currently attaches panes in observing mode when `mode == "observe"` and active mode otherwise (with `[Take over]` / `Ctrl+B t` promoting observing panes). Similarly, `Cargo.toml` specifies `license = "Apache-2.0"` without a separate root `LICENSE` file. `README.md` must describe both faithfully without inventing nonexistent files or unimplemented behaviors.
3. **Structured Reference Layout for Both Users and Contributors:**
   - *Rationale:* Organize `README.md` top-down from user onboarding (Overview + ASCII layout diagram → Prerequisites & Installation → Quick Start → CLI Reference & Exit Codes → Socket Discovery, Auto-Start & Configuration) into interactive operation (Spaces, Panels & Agents → Observing vs Active Takeover → Keybindings → Mouse, Scrollback & Clipboard → Modal Dialogs) and technical reference (Terminal Emulation & Rendering Highlights → Architecture & Project Layout → Development & Testing → License).
4. **Zero Broken Links & Verifiable Cross-Checks:**
   - *Rationale:* Only link to repository files verified to exist (`docs/protocol-notes.md`, `Cargo.toml`), and require an explicit source-vs-README verification checklist before completion alongside standard Rust quality gates (`cargo test`, `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`).

---

## Verified Source-of-Truth Inventory

| `README.md` Topic | Authoritative Source Files | Verified Facts to Document |
|---|---|---|
| Package, MSRV, License | `Cargo.toml` | Package `herdr-tui` v`0.1.0` (`edition = "2021"`), lib `herdr_tui`, bin `htui`, MSRV `1.85`, `license = "Apache-2.0"`. |
| CLI & Exit Codes | `src/main.rs`, `src/cli/mod.rs`, `ls.rs`, `status.rs`, `events.rs`, `open.rs`, `tests/cli.rs` | Global `--socket <SOCKET>`; `ls [--json] [--all] [--space <SPACE>] [--label <K=V>]...`, `status [--json]`, `events [--json]`, `open <SPACE> [--create]`. Exit codes: `0` OK / clean `BrokenPipe`, `1` RPC/I/O/TUI error, `2` malformed `--label` or CLI parse error, `3` unreachable socket, `4` missing space on `open` without `--create`. |
| Socket & `hc` Auto-Start | `src/hc/transport.rs`, `docs/protocol-notes.md` | Precedence: `--socket` → `HTUI_HC_SOCKET` → `config.hc.socket` → `$XDG_RUNTIME_DIR/herdr-core/${HC_SESSION:-default}.sock` (Linux fallback `/run/user/<uid>/herdr-core/<session>.sock` or `/tmp/herdr-core-<uid>/<session>.sock`; generic fallback `<temp_dir>/herdr-core/<session>.sock`). Env vars: `HTUI_HC_SOCKET`, `HC_SESSION`, `HTUI_HC_BIN` (default `"hc"`), `SHELL`. Auto-start runs `<HTUI_HC_BIN> server start` with null stdio (50 ms poll, 5 s timeout) only on interactive startup (`htui` / `htui open`) and skips custom sockets. Verified against `hc 0.1.21` (protocol gen `1`, OpenRPC `1.3.2`). |
| Configuration & Layout Store | `src/config.rs`, `src/layout/store.rs`, `src/ui/event_loop.rs` | Config file: `~/.config/herdr-tui/config.toml` (`$XDG_CONFIG_HOME/herdr-tui/config.toml`). Defaults: `[hc]` `socket = None`; `[ui]` `show_finished = true`, `show_unassigned = true`, `prefix_timeout_ms = 1500`, `sidebar_width = 30`; `[attach]` `mode = "auto"` (`"auto"` \| `"active"` \| `"observe"`). Layout stored in `hc` doc `htui.layout` (`doc.get`/`doc.set`, CAS max 3 retries) with mirror/fallback at `~/.config/herdr-tui/layout.json` (or `/tmp/htui-layout.json`). |
| Workspaces, Agents & Takeover | `src/model/tree.rs`, `src/layout/panel.rs`, `src/ui/sidebar.rs`, `src/ui/tabs.rs`, `src/ui/panes.rs`, `src/ui/theme.rs`, `src/term/session.rs` | Spaces (`(unassigned)` fallback), isolated Panels (tab pills with `(+N)`), `SplitTree` panes. Agent detection: `template`, `reported`, `status`, or known bins (`agy`, `claude`, `codex`, `opencode`, `gemini`, `aider`, `goose`, `cursor`, `pi`). Priority-sorted Agents panel. Status glyphs: `◐` Working, `⚠` Blocked, `✖` Error, `✔` Done, `●` Idle, `◈` Other, `·` Live (no status), `○` Finished. `[observing]` vs Active modes with `[Take over]` button (`Ctrl+B t`). Kanagawa Wave theme. |
| Keybindings & Dialogs | `src/ui/keymap.rs`, `src/ui/app.rs`, `src/ui/dialogs/` | `Ctrl+B` prefix (`1500` ms default, `[PREFIX]` indicator) with all 17 bindings + aliases (`%`/`\|`, `"`/`-`, `H`, `V`, `[`, `]`, `x`/`q`, `h`/`j`/`k`/`l`/arrows, `t`, `n`, `c`, `K`, `R`, `r`, `b`, `Tab`, `?`, `d`). `Ctrl+Shift+C` copy selection. Sidebar nav (`Up`/`k`, `Down`/`j`, `Left`/`Right`, `Tab`, `Enter`, `Esc`). Modals: Spawn (templates / `$SHELL` / `sh`, Command/Template, Task, Space, predicted `InitialSize`), New Space, Rename Space, Confirm Kill/Remove, Help. |
| Mouse, Scrollback & Clipboard | `src/ui/mouse.rs`, `src/term/mouse.rs`, `src/ui/selection.rs`, `src/ui/clipboard.rs`, `src/ui/tabs.rs`, `src/ui/panes.rs` | Tab clicks (`[+H]`, `[+V]`, `[+ New Terminal]`, panel/unattached pills), sidebar row clicks, split divider left-drag (`±1` cell tolerance, `0.1..=0.9` ratio, `50` ms debounce, immediate flush + layout save on release), `[Take over]` & `[+ New Terminal (Ctrl+B n)]` clicks, child mouse passthrough (X10, UTF-8 `?1005`, SGR `?1006`; `Shift` bypasses passthrough), wheel scrollback (`±3` lines), drag-select auto-copy via OSC 52 + `wl-copy` / `xclip` / `xsel`. |
| Terminal Emulation & Resize | `src/term/screen.rs`, `src/term/session.rs`, `src/ui/backend.rs`, `src/ui/event_loop.rs` | `alacritty_terminal 0.26` (`2000`-line scrollback), truecolor RGB & colon-SGR, UTF-8 grapheme cluster preservation (Arabic harakat/combining marks) + `Cf`/variation-selector sanitization, `SyncCrosstermBackend` (`\x1b[?2026h`/`l` Synchronized Output + post-non-ASCII `MoveTo`), DEC Mode 2048 (`?2048h`/`?2048l` per-screen tracking, `\x1b[48;{rows};{cols};0;0t` emission when `Active`), `120` ms outer resize settle, startup `last_panes_area` priming, `RunParams.size` (`InitialSize`), primary shrink scrollback reclaim. |
| Tests & Protocol Docs | `tests/`, `docs/protocol-notes.md` | `cargo test` (`cli`, `hc_client`, `layout`, `model`, `term`, `ui_snapshots`, `live`), `HTUI_LIVE_HC=1 cargo test --test live`, `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`. |

---

## Dependency Graph

```
Task 1: Author README.md header, overview, UI diagram, prerequisites, installation & quick start
  │
  └──▶ Task 2: Document CLI subcommands, exit codes, socket discovery, hc auto-start & config
         │
         ├── Checkpoint: Phase 1 Verification (CLI / Transport / Config accuracy check)
         │
         └──▶ Task 3: Document spaces/panels/agents, keybindings, mouse/clipboard & dialogs
                │
                └──▶ Task 4: Document terminal emulation, architecture, testing commands & license
                       │
                       └── Checkpoint: Complete (Full source audit, link check, cargo test/fmt/clippy)
```

---

## Task List

### Phase 1: Overview, Setup, CLI & Configuration
- [x] **Task 1:** Create `README.md` with project overview, feature summary, ASCII TUI architecture diagram, prerequisites (`hc 0.1.21`, Linux/macOS, Rust `1.85`), installation (`cargo install --path . --locked`), and quick start.
- [x] **Task 2:** Document the complete CLI reference (`ls`, `status`, `events`, `open`, global `--socket`), exit codes (`0`–`4`), socket discovery precedence, environment variables, `hc` auto-start semantics, `config.toml` schema/defaults, and `htui.layout` persistence in `README.md`.

### Checkpoint: After Tasks 1–2
- [x] Cross-check Task 1–2 sections line-by-line against `Cargo.toml`, `src/main.rs`, `src/cli/*.rs`, `src/hc/transport.rs`, `src/config.rs`, `src/layout/store.rs`, and `docs/protocol-notes.md`.

### Phase 2: Interactive Controls, Terminal Emulation & Developer Guide
- [x] **Task 3:** Document workspaces (`Spaces`, isolated `Panels`, split panes), `Agents` panel detection and status glyphs, Observing vs Active `[Take over]`, complete `Ctrl+B` prefix keybindings and sidebar/dialog keys, mouse controls/passthrough/`Shift` selection/scrollback, and OSC 52 + native clipboard integration in `README.md`.
- [x] **Task 4:** Document terminal emulation & rendering highlights (`alacritty_terminal`, Arabic/grapheme handling, `SyncCrosstermBackend`, DEC Mode 2048, resize/spawn hygiene), repository layout & `docs/protocol-notes.md` reference, testing/linting commands, and `Apache-2.0` license in `README.md`.

### Checkpoint: Complete
- [x] All acceptance criteria met with zero unverified claims and zero broken Markdown links.
- [x] `cargo test`, `cargo fmt --all -- --check`, and `cargo clippy --all-targets -- -D warnings` pass cleanly.

---

## Risks and Mitigations
| Risk | Impact | Mitigation |
|---|---|---|
| Stale details copied from `.orca/SPEC.md` (e.g., `vt100` crate, default `observe: true`) | High | Verify every claim exclusively against `src/`, `tests/`, `Cargo.toml`, and `docs/protocol-notes.md`. |
| Missing or inaccurate keybinding aliases, exit codes, or config defaults | Med | Cross-check `PrefixRouter::route` (`src/ui/keymap.rs`), `src/cli/*.rs`, and `Config::default()` (`src/config.rs`) line by line during verification. |
| Broken relative links in `README.md` (e.g., `LICENSE` or `CONTRIBUTING.md`) | Low | Verify every linked path in `README.md` exists on disk (`docs/protocol-notes.md`, `Cargo.toml`). |
| Concurrent cleanup commits in `src/`/`tests/` shifting minor behavior | Med | Re-read target modules right before writing and run `cargo test`, `cargo fmt --all -- --check`, and `cargo clippy --all-targets -- -D warnings` at the final checkpoint. |

## Open Questions
- None; all documented flags, bindings, defaults, and internal mechanisms were verified directly in the repository source.
