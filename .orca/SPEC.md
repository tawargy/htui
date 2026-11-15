# Spec: herdr-tui (`htui`)

> **Status:** Finalized & Approved · **File:** `.orca/SPEC.md` · **Source of truth for the protocol:** hc OpenRPC 1.3.2, `herdr-core` 0.1.21, protocol generation 1

---

## 0. Fundamental Architectural Assumptions & Core Constraints

1. **Direct Core Communication:** `htui` connects directly to the `hc` core over its local Unix domain socket using JSON-RPC 2.0 (plain line-delimited or framed multiplexed mode). It does not shell out to the `hc` CLI or parse its human-readable output.
2. **Space Concept:** A "space" is a client-side categorization backed by terminal labels (`space = <name>`), plus a persistent entry in `htui.layout` doc so empty spaces can exist and retain pane configurations.
3. **Namespaced State:** All `htui`-owned state on the core lives in dedicated documents: `htui.layout` (per-space split arrangements) and `htui.presence` (cooperative attach leases).
4. **Agent Definition:** A terminal where `template != null || reported == true`. All other terminals are plain shells/commands. Both appear in the sidebar.
5. **Crate Organization:** Single Cargo package `herdr-tui` containing a library crate (`herdr_tui`) and binary (`htui`).
6. **Operating Systems:** Linux and macOS (Unix sockets).
7. **GNOME Terminal (VTE) & Arabic Script Support:** The primary runtime target is GNOME Terminal (VTE). Rendering must preserve UTF-8 grapheme clusters and combining marks (Arabic letters + harakat/diacritics: Fatha, Damma, Kasra, Shadda, Sukun, Tanwin, ZWJ/ZWNJ) without splitting or corrupting characters across pane borders. SGR 1006 mouse tracking must work accurately without coordinate offset drift.
8. **Attach Control & Presence:** Terminals are attached with `observe: true` by default. Promotion to `observe: false` (Active) requires either absence of conflicting leases/foreign resize events or an explicit user **Take over** action.

---

## 1. Objective

**What:** `htui` is a fast, keyboard-driven terminal multiplexer and client for `herdr-core` (`hc`). It displays **machines → spaces → agents/terminals** in an interactive sidebar alongside a tmux-style split-pane workspace showing live, interactive terminals.

**Why:** `hc` keeps agent terminals running headless. `htui` gives SSH and terminal-first developers a responsive, multiplexed view of all running agents, their real-time OSC 7501 status, and interactive controls without leaving the terminal.

**Users:** Developers running multi-agent workflows (Claude Code, Codex, etc.) locally and across remote machines configured via `hc machine add`.

### User Stories
1. **Multiplexed Overview:** As a user, I launch `htui` and view all machines, spaces, and terminals with live status glyphs (`idle`, `working`, `blocked`, `done`, `error`) and progress indicators.
2. **Interactive Split Panes:** I can split panes horizontally and vertically, navigate between them with vim-style or arrow keys, type into active terminals, and observe output in real time.
3. **Arabic & BiDi Text Rendering:** When running interactive programs that output Arabic text with diacritics (harakat), text is rendered without corruption, truncation, or border cell desynchronization in GNOME Terminal.
4. **Cooperative Multi-Viewer Safety:** If another viewer or htui instance controls a terminal, the pane opens in read-only `[observing]` mode with an explicit **Take over** banner.
5. **Persistent Workspace Layouts:** Pane arrangements per space are automatically saved to `htui.layout` (or local file fallback) and restored upon relaunch.
6. **Agent & Shell Lifecycle:** I can spawn agents from templates (`templates.list`), start plain shells, kill runaway processes with confirmation (`terminal.kill`), and purge finished terminals (`terminal.rm`).
7. **Scriptable CLI Automation:** In scripts, I can run `htui ls`, `htui status`, `htui events --json`, and `htui open <space>`.

---

## 2. Tech Stack & Dependencies

| Concern | Crate / Tool | Purpose / Constraints |
|---|---|---|
| Language | Rust stable (2021 edition) | Strict typing, no `unsafe`, pinned `rust-version` |
| Async Runtime | `tokio` (1.x) | Features: `rt-multi-thread`, `net`, `io-util`, `sync`, `time`, `macros`, `signal` |
| TUI Framework | `ratatui` (0.29+) + `crossterm` (0.28+) | Features: `event-stream`, SGR 1006 mouse tracking |
| Terminal Emulation | `vt100` + `tui-term` | Behind `ScreenModel` trait; UTF-8 grapheme cluster preservation |
| Text Segmentation | `unicode-segmentation` + `unicode-width` | Combining marks & Arabic diacritic handling |
| Serialization | `serde`, `serde_json`, `toml` | Tolerant parsing, `#[serde(default)]` |
| CLI Argument Parsing | `clap` (v4, derive) | Subcommands: `ls`, `status`, `events`, `open` |
| Directories | `directories` | XDG paths (`~/.config/herdr-tui/`) |
| Errors | `thiserror` (lib), `anyhow` (bin) | Structured domain errors |
| Logging | `tracing` + `tracing-appender` | File logging only, off by default, zero keystroke logging |
| Dev Dependencies | `insta` (snapshots), `tempfile`, `pretty_assertions` | UI snapshot and regression testing |

---

## 3. Project Structure

```
herdr-tui/
├── Cargo.toml / Cargo.lock
├── src/
│   ├── main.rs              # Bin entrypoint: CLI dispatch & TUI bootstrap
│   ├── lib.rs               # Library root re-exporting modules
│   ├── config.rs            # Config load/save (~/.config/herdr-tui/config.toml)
│   ├── hc/                  # Module: hc-client
│   │   ├── mod.rs
│   │   ├── transport.rs     # Unix socket discovery & connection management
│   │   ├── rpc.rs           # JSON-RPC 2.0 message framing & request correlation
│   │   ├── framed.rs        # Framed binary multiplexer (DATA, RESIZE, CREDIT, etc.)
│   │   ├── types.rs         # Serde data models for OpenRPC 1.3.2
│   │   ├── methods.rs       # Strongly typed RPC method wrappers
│   │   ├── events.rs        # events.subscribe stream & heartbeat monitor
│   │   └── error.rs         # HcError and HcErrorKind mappings
│   ├── model/               # Module: state-store
│   │   ├── mod.rs
│   │   ├── address.rs       # Address parsing: t7, name, machine/t7
│   │   ├── tree.rs          # Machines -> Spaces -> Terminals view model
│   │   └── reducer.rs       # Pure reducer: apply(state, event) -> state
│   ├── term/                # Module: term-emulator & view
│   │   ├── mod.rs
│   │   ├── screen.rs        # ScreenModel trait, Vt100Screen, Arabic/combining marks
│   │   ├── session.rs       # Attach session state machine (Observing/Active/Ended)
│   │   └── keys.rs          # Crossterm KeyEvent -> xterm byte sequence encoder
│   ├── layout/              # Module: layout-engine
│   │   ├── mod.rs
│   │   ├── tree.rs          # Split-tree data structure & operations (pure)
│   │   ├── store.rs         # htui.layout CAS persistence & local file fallback
│   │   └── presence.rs      # htui.presence cooperative lease manager
│   ├── ui/                  # Module: tui-shell & app
│   │   ├── mod.rs
│   │   ├── app.rs           # Application state & widget composition
│   │   ├── event_loop.rs    # Tokio event loop, tick coalescing (60 FPS)
│   │   ├── keymap.rs        # Prefix key router (Ctrl+B)
│   │   ├── sidebar.rs       # Machines/spaces/terminals tree widget
│   │   ├── panes.rs         # Split-pane container & Arabic-safe cell rendering
│   │   ├── mouse.rs         # SGR 1006 mouse event handling & split dragging
│   │   ├── statusbar.rs     # Status bar & transient toasts
│   │   ├── theme.rs         # Styling & color palettes
│   │   └── dialogs/         # Modal dialogs (spawn, kill, rename, help, takeover)
│   └── cli/                 # Module: cli
│       ├── mod.rs           # Clap CLI definition
│       ├── ls.rs            # htui ls implementation
│       ├── status.rs        # htui status implementation
│       ├── events.rs        # htui events --json streaming
│       └── open.rs          # htui open <space> launcher
├── tests/
│   ├── support/
│   │   └── fake_hc.rs       # In-process fake hc core (UnixListener, plain & framed)
│   ├── fixtures/
│   │   ├── openrpc.json     # hc OpenRPC 1.3.2 schema fixture
│   │   ├── events.jsonl     # Captured notification event samples
│   │   └── streams/         # Captured binary terminal stream recordings
│   ├── hc_client.rs         # RPC & transport integration tests
│   ├── model.rs             # State reducer unit tests
│   ├── term.rs              # Terminal emulation & Arabic rendering tests
│   ├── layout.rs            # Split tree & presence tests
│   ├── ui_snapshots.rs      # Ratatui TestBackend snapshot tests
│   ├── cli.rs               # Subcommand integration tests
│   └── live.rs              # Opt-in live test against real hc core
├── docs/
│   └── protocol-notes.md    # HC-S0 spike report (framing, sockets, events)
└── .orca/
    ├── SPEC.md
    ├── plan.md
    └── todo.md
```

---

## 4. Module Specifications

### Module: `hc-client`
- **Transport:** Unix domain socket discovery priority: CLI `--socket` → Env `HTUI_HC_SOCKET` → Config `config.hc.socket` → Default paths from HC-S0 spike.
- **Multiplexed Attach:** Single framed connection running after `hello {generation: 1, framed: true}` multiplexing pane channels. Per-pane `mpsc` channels route `DATA`, `RESIZE`, `FOCUS`, `FACTS`, `CREDIT`, and `CLOSE`.
- **Event Stream:** Dedicated plain JSON-RPC connection for `events.subscribe`. Tracks heartbeats (45s timeout) and drops stale events after a `missed` snapshot (`seq <= snapshot.seq`).
- **Error Mapping:** Converts `data.kind` into strongly typed `HcErrorKind`.

### Module: `term-emulator` (`term-view`)
- **ScreenModel Trait:** Abstract terminal screen interface allowing pluggable parsers.
- **Arabic & Diacritics Preservation:** `Vt100Screen` processes VT byte streams and maps cells into Ratatui buffer cells. Multi-byte graphemes and combining marks (harakat) are attached to base characters without splitting across cells or line boundaries.
- **Attach State Machine:**
  - `Connecting` → `Observing` (default, `observe: true`).
  - `Observing` → `Active` (promoted on user Take Over or when no conflicting lease exists).
  - `Active` → `Observing` (user release or foreign takeover).
  - `Ended` (process exit, killed, or lost).

### Module: `layout-engine`
- **Split Tree:** Binary tree of splits (Horizontal / Vertical) with ratios clamped between `0.1` and `0.9`. Leaf nodes contain `Option<TermRef>`.
- **CAS Document Persistence:** Writes `htui.layout` using `doc.set {name, value, version}`. On version conflict, re-reads, merges per space, and retries up to 3 times before failing to local file `~/.config/herdr-tui/layout.json`.
- **Presence Leases:** Periodically writes `htui.presence` leases for active panes (renewed every 20s, stale after 60s). Foreign active leases inhibit auto-promotion.

### Module: `state-store`
- **View Hierarchy:** `MachineNode` → `SpaceNode` → `TerminalEntry`.
- **Pure Reducer:** `apply(state, input) -> State` transitions for all `events.subscribe` events, terminal status changes, and machine heartbeats.

### Module: `tui-shell`
- **Prefix Keymap:** Modal prefix handler default `Ctrl+B` (1500ms timeout).
- **GNOME Terminal / VTE Mouse:** SGR 1006 protocol capture. Accurate hit-testing for sidebar selection, pane focus, split divider dragging, and `[Take over]` button clicks.
- **Coalesced Redraws:** Max 60 FPS redraw loop using dirty tracking per pane.

### Module: `cli`
- Subcommands `ls`, `status`, `events`, `open` sharing underlying `hc-client` and `state-store` without initializing the TUI.

---

## 5. Acceptance Criteria

- **HC-1:** `connect()` performs handshake `hello {generation: 1}`. Server error `unsupported_generation` returns `HcErrorKind::UnsupportedGeneration`.
- **HC-2:** `connect()` validates that `hello.methods` contains `terminal.list`, `terminal.attach`, and `events.subscribe`.
- **HC-3:** Concurrent requests on the plain RPC connection correlate IDs correctly.
- **HC-4:** Every schema type in `types.rs` deserializes cleanly from fixture JSON with tolerant defaults.
- **HC-5:** `EventStream` correctly parses `snapshot`, `event`, `heartbeat`, and `missed` messages.
- **HC-6:** Missing heartbeats for 45s triggers disconnect and backoff reconnect.
- **HC-7:** Framed multiplexer routes frames across at least 8 concurrent channels independently.
- **HC-8:** Socket discovery correctly obeys flag → env → config → default precedence.
- **ST-1:** Terminal entries correctly group under machines and spaces, including synthetic `(unassigned)`.
- **ST-2:** Modifying the `space` label moves the terminal in the tree without dropping undeclared spaces.
- **ST-3:** Snapshot replacement drops obsolete events with `seq <= snapshot.seq`.
- **ST-4:** Unknown status words classify as `Other(String)` without panics.
- **ST-5:** Unreachable or `needs_login` machines display non-error status nodes.
- **ST-6:** Machine rename retains persistent `MachineId` associations.
- **TV-1:** Panes attach in `observe: true` mode initially.
- **TV-2:** Keystrokes in `Observing` mode do not send `DATA` frames and trigger the takeover hint.
- **TV-3:** Executing Take Over switches attach to `observe: false` and enables keyboard input forwarding.
- **TV-4:** Terminal resizing from core applies to `ScreenModel` before paint bytes.
- **TV-5:** Key encoding table covers full xterm translations including bracketed paste and app cursor mode.
- **TV-6:** Recorded agent stream fixtures render into text grids without sequence errors.
- **TV-7:** Terminals transition `lost` → `restarted` and reattach automatically.
- **TV-8:** Continuous resize dragging debounces `RESIZE` frames to at most one per 50ms.
- **TV-ARABIC:** Arabic text with combining marks (harakat: Fatha, Damma, Kasra, Shadda, Sukun, Tanwin) renders into `Vt100Screen` without byte truncation, corruption, or character width misalignment.
- **LY-1:** Split tree operations (`split`, `close`, `focus_dir`, `resize`) preserve tree integrity and bounds.
- **LY-2:** Removed terminals are pruned from layout trees and changes are saved.
- **LY-3:** Layout CAS conflicts trigger three-way space merging and retry up to 3 times.
- **LY-4:** Absence of core doc methods triggers transparent fallback to `layout.json`.
- **LY-5:** Active presence lease from another instance prevents auto-promotion.
- **LY-6:** Foreign `size` events within 10s inhibit auto-promotion.
- **LY-7:** Takeover event from instance B immediately demotes instance A to observing within 1s.
- **CL-1:** `htui ls --json` outputs structured JSON matching the server state.
- **CL-2:** `htui events --json | head -n 3` handles `SIGPIPE` cleanly with exit code 0.
- **CL-3:** `htui open nosuch` exits with code 4 when space is missing, or creates space when `--create` is passed.
- **CL-4:** Unreachable socket returns exit code 3 for all CLI commands.
- **CL-5:** Malformed `--label` parameters exit with code 2.
- **UI-1:** Sidebar snapshot test confirms machine/space/terminal hierarchy and glyphs.
- **UI-2:** Vertical and horizontal pane splits render correctly and persist layout state.
- **UI-3:** Observing panes display `[Take over]` banner and respond to clicks and `Prefix t`.
- **UI-4:** Template spawn dialog dispatches `terminal.run` with valid arguments and sizing.
- **UI-5:** Terminal kill requires modal confirmation before sending `terminal.kill`.
- **UI-6:** Removing a live terminal prompts to kill first.
- **UI-7:** Renaming a space relabels all member terminals and updates the layout.
- **UI-8:** Core disconnection shows a toast and automatically reconnects when available.
- **UI-9:** Exiting via `Prefix d` or SIGINT cleanly restores the terminal and drops presence leases.
- **UI-10:** `Prefix ?` displays a comprehensive help dialog derived from the live keymap.
- **UI-ARABIC:** Split panes displaying Arabic text and diacritics render into Ratatui buffer cells without character clipping, glyph corruption, or panics in GNOME Terminal.
- **UI-VTE-MOUSE:** SGR 1006 mouse events in GNOME Terminal accurately trigger pane focus, split divider dragging, and button activations without coordinate drift.

---

## 6. Boundaries & Quality Constraints

- **Never** shell out to `hc` CLI for core functionality.
- **Never** bypass confirmation dialogs before killing processes.
- **Never** log, persist, or expose terminal I/O bytes or keystrokes in logs.
- **Never** use `unwrap()` or `expect()` outside unit test harnesses.
- **Coverage Requirement:** Minimum 80% line coverage across `hc/`, `model/`, `term/`, and `layout/` modules.
- **Compiler Checks:** `cargo fmt --all -- --check` and `cargo clippy --all-targets -- -D warnings` must pass before every commit.
