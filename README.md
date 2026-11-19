# htui (`herdr-tui`)

A keyboard-driven terminal multiplexer and client for [`herdr-core` (`hc`)](docs/protocol-notes.md), built in Rust with `ratatui` and `alacritty_terminal`.

`htui` (`herdr-tui` v`0.1.0`) connects to a running `hc` daemon over its Unix domain socket (or auto-starts `hc` on interactive launch), organizes live and remote terminals into **Spaces**, isolated **Panels** (tabs), and **Split Panes**, surfaces AI coding **Agents** sorted by status priority, and supports concurrent **Observing** vs **Active** (`[Take over]`) sessions with full SGR mouse passthrough, scrollback, and clipboard integration.

---

## Highlights

- **Workspace Hierarchy (`Spaces` → `Panels` → `Split Panes`):** Group terminals by `space` label, keep multiple independent split layouts per space as tab pills (`(+N)`), and drag split dividers smoothly with mouse or resize with `Ctrl+B` prefix bindings.
- **Agent-Aware Sidebar:** Automatically detects AI agents (`agy`, `claude`, `codex`, `opencode`, `gemini`, `aider`, `goose`, `cursor`, `pi`, templates, or reported statuses) and sorts them by urgency (`⚠`/`✖` → `◐` → `✔` → `●` → `◈`/`·` → `○`).
- **Observing & Active Takeover:** Attach passively (`[observing]`) or interactively; promote an observing pane to active control at its exact dimensions via `[Take over]` or `Ctrl+B t`.
- **Rich Terminal Emulation:** Powered by `alacritty_terminal` (`2000`-line scrollback), truecolor RGB / colon-SGR / undercurl mapping, Arabic harakat and UTF-8 grapheme preservation, `SyncCrosstermBackend` (`?2026h`/`l`), and DEC Mode 2048 in-band resize notifications (e.g., Neovim `0.12+`).
- **Scriptable CLI Subcommands:** Inspect terminals (`htui ls`), daemon status (`htui status`), and live event streams (`htui events`) in human-readable or `--json` formats alongside interactive TUI entry (`htui`, `htui open`).

---

## TUI Workspace Layout

```text
┌─ Spaces ─────────────────┬─ ● zsh (+1) │ ◐ claude │ · bash ── [+H] [+V] [+ New Terminal] ┐
│   [+ New Space]          ├─ [observing]  [Take over] ──────┬────────────────────────────┤
│ ▾ local (connected)      │                                 │                            │
│   • api (2)              │  Focused / Observing Pane       │  Split Pane                │
│   • (unassigned) (1)     │  (live PTY output or            │  (or empty pane showing    │
│                          │   [connecting / unattached])    │  [+ New Terminal (Ctrl+B n)])
├─ Agents ─────────────────┤                                 │                            │
│  ⚠ codex [api]           │                                 │                            │
│  ◐ claude [api]          │                                 │                            │
└──────────────────────────┴─────────────────────────────────┴────────────────────────────┘
 htui | local | space: api [PREFIX]  —  Ctrl+B ? Help | Ctrl+B d Quit
```

---

## Prerequisites

- **Rust Toolchain (for building from source):** Rust `1.85` or newer (MSRV declared in [`Cargo.toml`](Cargo.toml)).
- **Operating System:** Linux or macOS (communicates with `hc` over Unix domain sockets).
- **`herdr-core` (`hc`):** Required at runtime (`hc` binary in `$PATH` or specified via `HTUI_HC_BIN`). Verified against `hc 0.1.21` (protocol generation `1`, OpenRPC `1.3.2`).

---

## Installation

Install the `htui` binary directly from the repository:

```bash
cargo install --path . --locked
```

Or compile a release binary without installing globally:

```bash
cargo build --release
# Binary is at ./target/release/htui
```

---

## Quick Start

Launch the interactive TUI (auto-starting `hc server start` on the default socket if needed):

```bash
htui
```

Open directly into a specific space, creating an empty space if it does not yet have any terminals:

```bash
htui open my-space --create
```

Inspect running terminals or daemon status from the command line:

```bash
htui ls
htui ls --all --json
htui status
```

---

## CLI Reference

```text
htui [--socket <PATH>] [COMMAND]
```

Running `htui` with no subcommand starts the interactive TUI.

### Global Flags

| Flag | Description |
|---|---|
| `--socket <PATH>` | Path to the `hc` Unix domain socket (overrides `HTUI_HC_SOCKET` and `config.toml`). |
| `-h`, `--help` | Print CLI help information. |
| `-V`, `--version` | Print version information (`htui 0.1.0`). |

### Subcommands

#### `htui ls`
List machines, spaces, and terminals known to `hc`.

```bash
htui ls [--json] [--all] [--space <SPACE>] [--label <K=V>]...
```

- `--json` — Emit raw JSON (`terminals` and `machines`) instead of a formatted table.
- `--all` — Include finished terminals (by default `hc` returns live terminals).
- `--space <SPACE>` — Filter terminals to those belonging to `<SPACE>` (including `(unassigned)`).
- `--label <K=V>` — Filter terminals matching `labels[K] == V` (repeatable).

Table columns in plain output: `MACHINE`, `SPACE`, `ADDRESS`, `NAME`, `STATUS`, `STATE`, `AGE` (plus `stderr` warnings for any machine whose state is not `connected`).

#### `htui status`
Show `hc` core identity, version, PID, uptime, live/finished counts, and connected machines.

```bash
htui status [--json]
```

#### `htui events`
Subscribe to `events.subscribe` and stream the initial snapshot followed by live events (`event`, `heartbeat`, `missed`).

```bash
htui events [--json]
```

- Plain output prints `snapshot seq=... terminals=...`, `event seq=... type=... terminal=...`, `heartbeat seq=...`, and `missed seq=... terminals=...`.
- `--json` outputs one JSON object per line (`{"type":"snapshot",...}`, event payloads, `{"type":"heartbeat",...}`, `{"type":"missed",...}`).

#### `htui open`
Open the interactive TUI pre-focused on `<SPACE>`.

```bash
htui open <SPACE> [--create]
```

- Without `--create`, verifies that `<SPACE>` has at least one terminal (live or finished) and exits with code `4` if missing.
- With `--create`, skips the existence check and opens the TUI with an active empty space ready to spawn terminals.

### Exit Codes

| Code | Meaning |
|---|---|
| `0` | Success, or clean exit on `BrokenPipe` when piping `ls` / `status` / `events` output. |
| `1` | General runtime error (RPC error, I/O error, TUI error, or non-unreachable connection failure). |
| `2` | Invalid CLI usage or malformed `--label` filter (must be `K=V` with non-empty key and value). |
| `3` | `SocketUnreachable` — failed to connect to the `hc` Unix domain socket. |
| `4` | Space not found when running `htui open <SPACE>` without `--create`. |

---

## Socket Discovery, Environment Variables & `hc` Auto-Start

### Socket Discovery Precedence

`htui` resolves the `hc` Unix domain socket in strict priority order:

1. CLI flag: `--socket <PATH>`
2. Environment variable: `HTUI_HC_SOCKET` (when non-empty after trimming)
3. Configuration file: `[hc].socket` in `config.toml`
4. Default core socket path:
   - `$XDG_RUNTIME_DIR/herdr-core/${HC_SESSION:-default}.sock` (when `XDG_RUNTIME_DIR` is non-empty)
   - On Linux when `XDG_RUNTIME_DIR` is unset: `/run/user/<uid>/herdr-core/${HC_SESSION:-default}.sock` if `/run/user/<uid>` exists, otherwise `/tmp/herdr-core-<uid>/${HC_SESSION:-default}.sock`
   - Generic Unix/macOS fallback when `XDG_RUNTIME_DIR` is unset: `<temp_dir>/herdr-core/${HC_SESSION:-default}.sock`

### Environment Variables

| Variable | Default | Purpose |
|---|---|---|
| `HTUI_HC_SOCKET` | *(unset)* | Overrides the `hc` Unix domain socket path (lower precedence than `--socket`). |
| `HC_SESSION` | `"default"` | Session name used to construct `<session>.sock` in the default socket path. |
| `HTUI_HC_BIN` | `"hc"` | Executable name or path invoked when auto-starting `hc server start`. |
| `SHELL` | `"sh"` | Default command pre-filled in the Spawn Terminal dialog (basename extracted from `$SHELL`, falling back to `"sh"` if unset/empty). |
| `HTUI_LIVE_HC` | *(unset)* | Opt-in flag (`HTUI_LIVE_HC=1`) enabling live daemon integration tests in `tests/live.rs`. |

### `hc` Auto-Start Behavior

- **Interactive startup only:** Auto-start runs on `htui` and `htui open <SPACE>` (prior to space validation). Non-interactive subcommands (`ls`, `status`, `events`) never auto-start `hc`.
- **Skipped for custom sockets:** If a custom socket is supplied via `--socket`, non-empty `HTUI_HC_SOCKET`, or `[hc].socket`, auto-start is skipped.
- **Spawn & readiness polling:** If the default socket is not yet reachable, `htui` spawns `<HTUI_HC_BIN> server start` detached (`stdin`/`stdout`/`stderr` redirected to null) and polls socket connectivity every `50 ms` for up to `5 s` (fast-failing if the child exits non-zero before the socket appears). If auto-start fails, a `warning:` is printed to `stderr`.

---

## Configuration & Layout Persistence

### `config.toml`

`htui` reads configuration from `~/.config/herdr-tui/config.toml` (resolved via `$XDG_CONFIG_HOME/herdr-tui/config.toml` or platform equivalent from `directories::ProjectDirs`). Missing files or omitted fields fall back to these defaults:

```toml
[hc]
# Optional custom hc Unix domain socket path.
# socket = "/run/user/1000/herdr-core/default.sock"

[ui]
show_finished = true
show_unassigned = true
prefix_timeout_ms = 1500
sidebar_width = 30

[attach]
# "auto" | "active" | "observe"
# When set to "observe", panes attach in passive observing mode ([observing]).
# When set to "auto" or "active", panes attach in active mode (observe: false).
mode = "auto"
```

### Layout Persistence (`htui.layout` & `layout.json`)

Workspace layouts (each space's panels, `SplitTree` ratios, active pane/panel, and terminal assignments) are persisted automatically:

- **Primary store (`hc` document API):** When `hc` advertises both `doc.get` and `doc.set` in `hello.methods`, layouts are saved to the shared `htui.layout` document using compare-and-swap (CAS) with per-space merging and up to `3` retries on version conflicts.
- **Local mirror & fallback:** Every successful remote save mirrors locally, and if `hc` lacks `doc.get`/`doc.set` or CAS retries are exhausted, `htui` falls back to `~/.config/herdr-tui/layout.json` (or `/tmp/htui-layout.json` if the config directory cannot be resolved). Legacy single-tree space entries are automatically upgraded to multi-panel layouts on load.

---

## Workspaces, Agents & Interactive Controls

### Workspace Hierarchy

`htui` uses the Kanagawa Wave palette and organizes terminals into a four-level hierarchy:

```text
Machines ──▶ Spaces ──▶ Isolated Panels (tabs) ──▶ Split Panes (SplitTree)
```

- **Spaces:** Terminals are grouped by their `space` label; terminals without a `space` label appear under `(unassigned)` (configurable via `ui.show_unassigned`). Empty spaces created interactively are preserved while active and automatically pruned when switching away, closing their last terminal, or quitting `htui`.
- **Isolated Panels (Tabs):** Each space maintains one or more independent panels shown as pills in the top tab bar. Multi-pane panels show their extra pane count as `(+N)`. Unattached live terminals in the active space also appear as pills so you can open them into their own panel with a click.
- **Split Panes:** Every panel owns a binary `SplitTree` supporting nested horizontal and vertical splits.

### `Agents` Panel Detection & Status Glyphs

The sidebar `Agents` panel surfaces terminals identified as AI coding agents. A terminal is classified as an agent if any of the following hold:

- It was launched from an `hc` `template`, has `reported: true`, or has a structured `status` payload.
- Its `foreground.name` or `argv[0]` basename matches a known agent binary: `agy`, `claude`, `codex`, `opencode`, `gemini`, `aider`, `goose`, `cursor`, or `pi`.

Agents are sorted by urgency: **Blocked / Error** (`0`) → **Working** (`1`) → **Done** (`2`) → **Idle** (`3`) → **Other / no status** (`4`) → **Finished** (`5`), breaking ties by terminal address.

| Glyph | State | Meaning |
|---|---|---|
| `◐` | `Working` | Live agent actively working |
| `⚠` | `Blocked` | Live agent waiting on input / blocked |
| `✖` | `Error` | Live terminal / agent in error state |
| `✔` | `Done` | Live agent completed its task |
| `●` | `Idle` | Live agent idle |
| `◈` | `Other` | Live terminal reporting a custom status |
| `·` | Live (no status) | Live plain shell or terminal without status |
| `○` | Finished | Exited / finished terminal |

### Observing vs Active Takeover

Each pane attaches to `hc` in one of two interactive states:

- **Observing (`[observing]`):** Passive view (`observe: true`). Keystrokes are suppressed and trigger a status bar toast: `Observing mode: press Ctrl+B t or click [Take over]`.
- **Active:** Full PTY input and resize ownership (`observe: false`).

Promote any observing pane to active control at its exact pane inner dimensions by pressing `Ctrl+B t` or clicking `[Take over]` in the pane title bar.

### Keybindings

#### `Ctrl+B` Prefix Bindings

Press `Ctrl+B` to arm prefix mode (`[PREFIX]` appears in the status bar for `ui.prefix_timeout_ms`, default `1500` ms), then press any command key:

| Keybinding | Action |
|---|---|
| `Ctrl+B %` or `Ctrl+B \|` | Split focused pane horizontally |
| `Ctrl+B "` or `Ctrl+B -` | Split focused pane vertically |
| `Ctrl+B H` | Split horizontally and open Spawn dialog |
| `Ctrl+B V` | Split vertically and open Spawn dialog |
| `Ctrl+B [` | Switch to previous panel (tab) in current space |
| `Ctrl+B ]` | Switch to next panel (tab) in current space |
| `Ctrl+B x` or `Ctrl+B q` | Close focused pane (and prune emptied splits/spaces) |
| `Ctrl+B h` / `Left` | Focus pane to the left |
| `Ctrl+B j` / `Down` | Focus pane below |
| `Ctrl+B k` / `Up` | Focus pane above |
| `Ctrl+B l` / `Right` | Focus pane to the right |
| `Ctrl+B t` | Take over observing pane |
| `Ctrl+B n` | Open Spawn Agent / Shell dialog |
| `Ctrl+B c` | Open Create New Space dialog |
| `Ctrl+B K` | Confirm and kill focused live terminal |
| `Ctrl+B R` | Confirm and remove focused terminal (prompts to kill first if still live) |
| `Ctrl+B r` | Rename current space |
| `Ctrl+B b` | Toggle sidebar visibility |
| `Ctrl+B Tab` | Toggle keyboard focus between sidebar and panes |
| `Ctrl+B ?` | Open keybindings help modal |
| `Ctrl+B d` | Detach and quit `htui` |

#### Direct Keys & Sidebar Navigation

- **`Ctrl+Shift+C`:** Copy the active text selection in the focused pane to the clipboard.
- **Sidebar Navigation (when sidebar is focused, or `Up`/`Down` on an empty pane):**
  - `Up` / `k`, `Down` / `j` — Move selection across `Spaces` and `Agents` panels.
  - `Left` / `Right` — Collapse or expand the selected machine/space in `Spaces`.
  - `Tab` — Switch focus between `Spaces` and `Agents` panels.
  - `Enter` — Switch to space, open terminal, toggle machine collapse, or open the New Space dialog (`[+ New Space]`).
  - `Esc` — Return focus to the active pane.

### Mouse Controls, Passthrough, Scrollback & Clipboard

- **Tab Bar Clicks:** Click panel pills to switch panels, unattached terminal pills to open them in a new isolated panel, or `[+H]`, `[+V]`, and `[+ New Terminal]` action buttons.
- **Sidebar Clicks:** Click any inner row of `Spaces` or `Agents` to select and activate it (clicks on panel borders are ignored).
- **Split Divider Dragging:** Left-drag any split divider (`±1` cell hit tolerance, ratio clamped to `0.1..=0.9`). PTY resize requests are debounced every `50 ms` during dragging, and releasing the mouse immediately flushes any pending resize and persists the updated layout.
- **Pane Buttons:** Click `[Take over]` in an observing pane's title bar to promote it, or click `[+ New Terminal (Ctrl+B n)]` inside an empty pane to spawn directly into that pane.
- **Child Mouse Passthrough & `Shift` Override:** When the focused pane is `Active` and the child application enables mouse tracking, mouse presses, drags, releases, movement, and wheel events are encoded and forwarded using the active protocol mode (`Press`, `PressRelease`, `ButtonMotion`, `AnyMotion`) and encoding (Default/X10, UTF-8 `?1005`, or SGR `?1006`). Hold `Shift` while clicking, dragging, or scrolling to bypass child mouse passthrough and use `htui` selection or scrollback instead.
- **Wheel Scrollback:** Scrolling on a pane without active mouse passthrough (or with `Shift` held) scrolls the `2000`-line terminal buffer by `3` lines per tick.
- **Selection & Clipboard:** Left-dragging across a pane selects text and automatically copies on release (and `Ctrl+Shift+C` copies the current selection). Copying emits an `OSC 52` sequence (`\x1b]52;c;<base64>\x07`) to `stdout` and also pipes to the first available native clipboard utility: `wl-copy`, `xclip -selection clipboard`, or `xsel --clipboard --input`.

### Modal Dialogs

- **Spawn Agent / Shell (`Ctrl+B n`, `H`, `V`, or tab/pane spawn buttons):** Shows the spawn target (`New panel`, `Selected pane`, `Horizontal split`, or `Vertical split`), available `hc` templates, and three `Tab`-navigated fields (`Command/Template` defaulting to the first template or `$SHELL` basename/`sh`, `Task`, and `Space`). Submits `terminal.run` with predicted `InitialSize` (`cols`, `rows`) for the target pane.
- **Create New Space (`Ctrl+B c` or `[+ New Space]`):** Creates and activates a named space.
- **Rename Space (`Ctrl+B r`):** Updates the `space` label across terminals in the space and migrates its layout entry.
- **Confirm Kill / Remove (`Ctrl+B K` / `Ctrl+B R`):** Confirms destructive terminal actions (`y`/`Y`/`Enter` to proceed, `n`/`N`/`Esc` to cancel).
- **Keybindings Help (`Ctrl+B ?`):** Displays all `Ctrl+B` bindings (`Esc`, `q`, `?`, or `Enter` to close).

---

## Terminal Emulation & Rendering Highlights

- **`alacritty_terminal` (`0.26`) Engine:** `AlacrittyScreen` implements `ScreenModel` with a `2000`-line scrollback buffer, truecolor RGB and indexed color resolution, and SGR attribute / `ALL_UNDERLINES` (including colon-SGR / undercurl) mapping.
- **Grapheme & Arabic Script Safety:** Preserves full UTF-8 grapheme clusters via `unicode-segmentation` (keeping Arabic harakat and combining diacritics intact) while `sanitize_cell_symbol` strips control characters, soft hyphens, invisible `Cf` format codepoints, and variation selectors to prevent width drift between the emulated grid and `ratatui`.
- **`SyncCrosstermBackend`:** Wraps every `draw()` call in Synchronized Output (`\x1b[?2026h` / `\x1b[?2026l`) to prevent mid-frame tearing, and emits an explicit `MoveTo(x, y)` after any non-ASCII cell so physical terminal width quirks (e.g., in VTE / GNOME Terminal) cannot misalign subsequent cells or pane borders.
- **DEC Mode 2048 In-Band Resize:** Tracks `?2048h` / `?2048l` independently for primary and alternate screens; when a session is `Active` and Mode 2048 is enabled, `htui` synthesizes `\x1b[48;{rows};{cols};0;0t` (deduplicated per `(cols, rows)`) for apps such as Neovim `0.12+`.
- **Resize & Spawn Hygiene:**
  - `120 ms` outer terminal resize settle window (`OUTER_RESIZE_SETTLE`) to coalesce rapid window resizes.
  - `50 ms` resize debounce (`RESIZE_DEBOUNCE`) during interactive divider drags, with immediate flush on release.
  - Startup `last_panes_area` priming prior to initial terminal attachment.
  - Predicted `InitialSize` (`RunParams.size`) computed from target pane/split geometry at spawn time.
  - Primary-screen shrink scrollback reclaim in `AlacrittyScreen::resize` to avoid trailing blank rows pushing visible content into history.

---

## Architecture & Project Layout

```text
src/
├── main.rs            # CLI entrypoint, socket resolution, hc auto-start, TUI dispatch
├── lib.rs             # Library root exposing modules below
├── config.rs          # ~/.config/herdr-tui/config.toml & layout fallback path resolution
├── cli/               # Subcommands: ls, status, events, open, label filter parsing
├── hc/                # JSON-RPC 2.0 client, binary framed multiplexer, events, transport
├── model/             # Terminal address parsing, reducer state, machine/space/agent tree
├── layout/            # PanelLayout, SpaceLayout, SplitTree, presence & htui.layout CAS store
├── term/              # AlacrittyScreen (ScreenModel), AttachSession, key/mouse encoding
└── ui/                # TuiApp, event loop, SyncCrosstermBackend, sidebar, tabs, panes,
                       # statusbar, mouse, selection, clipboard, Kanagawa Wave theme, dialogs/
```

For wire-level details on the binary framed protocol, `hello` negotiation, `terminal.attach` channels, `CREDIT` flow control, and `events.subscribe` payloads, see [`docs/protocol-notes.md`](docs/protocol-notes.md).

---

## Development & Testing

Run the unit and offline integration test suite (backed by `FakeHcServer` in `tests/support/fake_hc.rs` and `insta` UI snapshot tests in `tests/ui_snapshots.rs`):

```bash
cargo test
```

Run formatting and lint checks:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
```

Run live `hc` daemon smoke tests (requires a running or reachable `hc` daemon on the default socket):

```bash
HTUI_LIVE_HC=1 cargo test --test live
```

---

## License

Licensed under the **Apache-2.0** license as declared in [`Cargo.toml`](Cargo.toml).
