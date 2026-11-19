# Graph Report - htui  (2026-11-19)

## Corpus Check
- 88 files · ~93,197 words
- Verdict: corpus is large enough that graph structure adds value.
- Unclassified: 2 file(s) not represented in the graph (top: (none) 1, .jsonl 1)

## Summary
- 1599 nodes · 3187 edges · 96 communities (71 shown, 25 thin omitted)
- Extraction: 96% EXTRACTED · 4% INFERRED · 0% AMBIGUOUS · INFERRED: 124 edges (avg confidence: 0.85)
- Token cost: 0 input · 0 output

## Graph Freshness
- Built from commit: `52030359`
- Run `git rev-parse HEAD` and compare to check if the graph is stale.
- Run `graphify update .` after code changes (no API cost).

## Community Hubs (Navigation)
- PaneId
- TuiApp
- screen.rs
- term/mouse.rs
- backend.rs
- sidebar.rs
- fake_hc.rs
- RpcClient
- Command
- ui/mouse.rs
- types.rs
- Task List: Show Active Empty Named Space in Sidebar & Harden New-Space Flow
- Theme
- HcErrorKind
- HcResult
- ui_snapshots.rs
- Implementation Plan: Fix `nvim` Rendering & Resize Corruption in `htui`
- cli.rs
- transport.rs
- Task List
- reducer.rs
- 2. Handshake (`hello`) & Mode Negotiation
- spawn.rs
- tabs.rs
- Implementation Plan: Unified Action Button Styling (New Space & New Terminal)
- Task Breakdown: Fix Terminal Text Corruption on Outer Window & Pane Resize
- hc_client.rs
- term.rs
- path
- Task List: Isolated Multi-Panel Workspaces & Per-Panel Splits
- Spec: herdr-tui (`htui`)
- hc/events.rs
- ActiveDialog
- Implementation Plan: Fix Remaining Side-by-Side Divider Breakage & Cross-Pane Text Bleeding During Drag
- event_loop.rs
- confirm.rs
- Implementation Plan: Replace `vt100` with `alacritty_terminal` Behind `ScreenModel`
- Config
- Implementation Plan: `Ctrl+b q` ClosePane Rebinding & Empty Space / Empty Split Cleanup
- Implementation Plan: Terminal Mouse Pass-Through in `htui`
- Terminal
- presence.rs
- store.rs
- LayoutStore
- SessionState
- Implementation Plan: `Command::ClosePane` (`Ctrl+b q` / `Ctrl+b x`) Kills and Removes Terminal in `hc`
- Task List: Unified Action Button Styling (New Space & New Terminal)
- HcError
- Task List: `Ctrl+b q` ClosePane Rebinding & Empty Space / Empty Split Cleanup
- Implementation Plan: `hc server start` Auto-Start on `htui` Startup
- Task List: Terminal Mouse Pass-Through in `htui`
- pathbuf
- htui (`herdr-tui`)
- Task List: `hc server start` Auto-Start on `htui` Startup
- Implementation Plan: Fix Side-by-Side Terminal Resize Text Corruption
- Task Breakdown: Fix Side-by-Side Terminal Resize Text Corruption
- Implementation Plan: Inherit App Background for Panel Tab Bar & Footer Status Bar
- new_space.rs
- help.rs
- Task Breakdown: Replace `vt100` with `alacritty_terminal` Behind `ScreenModel`
- Task List: `Command::ClosePane` (`Ctrl+b q` / `Ctrl+b x`) Kills and Removes Terminal in `hc`
- Task Breakdown: Fix Remaining Side-by-Side Divider Breakage & Cross-Pane Text Bleeding During Drag
- StatusKind
- rename.rs
- super
- DialogOutcome
- SharedVecWriter
- side_by_side_zsh_rprompt_outer_resize_storm_and_divider_drag_with_sync_backend
- Task List: Inherit App Background for Panel Tab Bar & Footer Status Bar
- widget
- Workspaces, Agents & Interactive Controls
- herdr-tui
- Subcommands
- TerminalEntry
- dispatch_action
- 4. `events.subscribe` & Notification Payloads
- 3. Binary Frame Wire Layout (Framed Mode)
- Task List: Add Comprehensive Root `README.md` for `htui` (`herdr-tui`)
- Socket Discovery, Environment Variables & `hc` Auto-Start
- Configuration & Layout Persistence

## God Nodes (most connected - your core abstractions)
1. `TuiApp` - 78 edges
2. `HcResult` - 63 edges
3. `PaneId` - 52 edges
4. `HcErrorKind` - 45 edges
5. `Theme` - 42 edges
6. `RpcClient` - 38 edges
7. `SpaceLayout` - 37 edges
8. `AppAction` - 37 edges
9. `SplitTree` - 31 edges
10. `AttachSession` - 29 edges

## Surprising Connections (you probably didn't know these)
- `Risks and Mitigations` --references--> `new_space_shortcut_and_spawn_unassigned()`  [EXTRACTED]
  .orca/arch/active-empty-space-plan.md → tests/ui_snapshots.rs
- `Architecture Decisions` --references--> `new_space_shortcut_and_spawn_unassigned()`  [EXTRACTED]
  .orca/arch/closepane-empty-spaces-plan.md → tests/ui_snapshots.rs
- `Risks and Mitigations` --references--> `new_space_shortcut_and_spawn_unassigned()`  [EXTRACTED]
  .orca/arch/closepane-empty-spaces-plan.md → tests/ui_snapshots.rs
- `Task 1: Hide Empty Spaces in `SidebarState::rebuild` and Verify Sidebar / Model Invariants` --references--> `new_space_shortcut_and_spawn_unassigned()`  [EXTRACTED]
  .orca/arch/closepane-empty-spaces-todo.md → tests/ui_snapshots.rs
- `dispatch_spawn_passes_predicted_initial_size_to_terminal_run()` --calls--> `dispatch_action()`  [INFERRED]
  tests/hc_client.rs → src/ui/event_loop.rs

## Import Cycles
- 2-file cycle: `src/ui/dialogs/confirm.rs -> src/ui/dialogs/mod.rs -> src/ui/dialogs/confirm.rs`
- 2-file cycle: `src/ui/dialogs/mod.rs -> src/ui/dialogs/rename.rs -> src/ui/dialogs/mod.rs`
- 2-file cycle: `src/ui/dialogs/help.rs -> src/ui/dialogs/mod.rs -> src/ui/dialogs/help.rs`
- 2-file cycle: `src/ui/dialogs/mod.rs -> src/ui/dialogs/spawn.rs -> src/ui/dialogs/mod.rs`
- 2-file cycle: `src/ui/dialogs/mod.rs -> src/ui/dialogs/new_space.rs -> src/ui/dialogs/mod.rs`

## Communities (96 total, 25 thin omitted)

### Community 0 - "PaneId"
Cohesion: 0.07
Nodes (36): collect_terminal_addresses(), ensure_valid_focus_recovers_after_pane_close(), find_pane_by_terminal_in(), first_terminal_in(), from_tree_normalizes_empty_legacy_tree(), panel_cycling_and_removal_selects_adjacent_panel(), panel_isolation_and_independent_splits(), PanelId (+28 more)

### Community 1 - "TuiApp"
Cohesion: 0.07
Nodes (30): AppAction, AttachTerminal, CopyToClipboard, Kill, None, Quit, Remove, RemoveLayout (+22 more)

### Community 2 - "screen.rs"
Cohesion: 0.06
Nodes (33): AlacrittyScreen, arabic_harakat_and_cf_vs_zwj_sanitization(), clear_from_col_zero_breaks_prev_wrapline_including_scrollback(), clear_prev_wrapline(), clear_screen_below_after_shrink_prevents_old_and_new_prompts_merging_on_grow(), ClearWrapHandler, colon_sgr_truecolor_and_undercurl_map_correctly(), cols_one_wide_char_safety_and_logical_size() (+25 more)

### Community 3 - "term/mouse.rs"
Cohesion: 0.10
Nodes (37): app_cursor_and_normal_arrows(), bracketed_paste_wrapping(), encode_cursor_key(), encode_key(), encode_paste(), encode_ss3_or_mod(), encode_tilde_key(), utf8_arabic_and_ctrl_keys() (+29 more)

### Community 4 - "backend.rs"
Cohesion: 0.06
Nodes (10): ADD_ATTRS, cell_with(), draw_emits_moveto_after_non_ascii_and_elides_for_consecutive_ascii(), draw_handles_empty_diff_with_balanced_sync_update(), draw_handles_modifier_diff_including_bold_dim_and_underline_color(), draw_wraps_frame_in_synchronized_update_sequences(), queue_modifier_diff(), REMOVE_ATTRS (+2 more)

### Community 5 - "sidebar.rs"
Cohesion: 0.11
Nodes (14): agent_priority(), render_agents_panel(), render_sidebar(), render_spaces_panel(), SidebarItem, Machine, NewSpaceAction, Space (+6 more)

### Community 6 - "fake_hc.rs"
Cohesion: 0.10
Nodes (14): dispatch_rpc(), encode_frame(), FakeHcServer, FakeState, FRAME_AUTO_INPUT, FRAME_CLOSE, FRAME_CREDIT, FRAME_DATA (+6 more)

### Community 7 - "RpcClient"
Cohesion: 0.15
Nodes (4): extract_rpc_result(), MANDATORY_METHODS, RpcClient, HelloResult

### Community 8 - "Command"
Cohesion: 0.05
Nodes (41): NavDir, Down, Left, Right, Up, B64_TABLE, base64_encode(), copy_to_clipboard() (+33 more)

### Community 9 - "ui/mouse.rs"
Cohesion: 0.11
Nodes (37): clamp_rel(), dividers_sidebar_and_buttons_take_precedence_over_passthrough(), find_passthrough_pane(), handle_mouse(), handle_mouse_with_passthrough(), hit_divider(), make_mouse(), MouseAction (+29 more)

### Community 10 - "types.rs"
Cohesion: 0.16
Nodes (14): DocGetResult, DocSetResult, EventPayload, HolderInfo, InitialSize, ListResult, MachineListResult, MachineView (+6 more)

### Community 11 - "Task List: Show Active Empty Named Space in Sidebar & Harden New-Space Flow"
Cohesion: 0.05
Nodes (37): Architecture Decisions, Checkpoint: Complete, Checkpoint: Sidebar Visibility Verified, Dependency Graph, Implementation Plan: Show Active Empty Named Space in Sidebar & Harden New-Space Flow, Overview, Phase 1: Core Sidebar Active-Space Visibility, Phase 2: `[+ New Space]` Click, `SubmitNewSpace`, and Lifecycle Hardening (+29 more)

### Community 12 - "Theme"
Cohesion: 0.10
Nodes (14): Theme, .CARP_YELLOW, .CRYSTAL_BLUE, .FUJI_WHITE, .OLD_WHITE, .ONI_VIOLET, .SPRING_GREEN, .SUMI_INK0 (+6 more)

### Community 13 - "HcErrorKind"
Cohesion: 0.06
Nodes (33): HcErrorKind, AlreadyAdded, Blocked, Cancelled, ChannelClosed, Config, Conflict, EditFailed (+25 more)

### Community 14 - "HcResult"
Cohesion: 0.06
Nodes (18): HcResult, ChannelHandle, ChannelRegistry, Frame, frame_roundtrip_9_byte_le_header(), FramedClient, FrameKind, AutomaticInput (+10 more)

### Community 15 - "ui_snapshots.rs"
Cohesion: 0.06
Nodes (12): render_panes(), buffer_to_string(), dialogs_snapshot(), mouse_hit_testing(), multipanel_per_panel_splits_and_panel_switching(), panes_arabic_snapshot(), render_panes_unattached_placed_terminal_omits_spawn_button(), side_by_side_panes_boundary_wide_and_zero_width_preserve_divider_and_diff() (+4 more)

### Community 16 - "Implementation Plan: Fix `nvim` Rendering & Resize Corruption in `htui`"
Cohesion: 0.15
Nodes (12): Architecture & Content Decisions, Checkpoint: After Tasks 1–2, Checkpoint: Complete, Dependency Graph, Implementation Plan: Add Comprehensive Root `README.md` for `htui` (`herdr-tui`), Open Questions, Overview, Phase 1: Overview, Setup, CLI & Configuration (+4 more)

### Community 17 - "cli.rs"
Cohesion: 0.09
Nodes (8): run_ls(), parse_label_filter(), validate_space_exists(), main(), BrokenPipeWriter, ls_subcommand(), open_subcommand(), open_subcommand_unreachable_socket_exits_3()

### Community 18 - "transport.rs"
Cohesion: 0.17
Nodes (11): connect_unix(), current_uid_linux(), custom_socket_detection(), default_core_socket_path(), is_custom_socket(), is_custom_socket_from_env(), resolve_socket_from_env(), resolve_socket_path() (+3 more)

### Community 19 - "Task List"
Cohesion: 0.10
Nodes (19): 1. Multi-Panel Data Model (`src/layout/panel.rs`, `src/layout/mod.rs`), 2. Backward-Compatible Persistence (`src/layout/store.rs`), 3. Spawn Targets & Keymap (`src/ui/dialogs/spawn.rs`, `src/ui/dialogs/mod.rs`, `src/ui/keymap.rs`), 4. Isolated Panel Tab Bar & Mouse/Keyboard UX (`src/ui/tabs.rs`, `src/ui/app.rs`), 5. Lifecycle, Cleanup & External Terminal Reconciliation (`src/ui/app.rs`, `src/ui/event_loop.rs`), Architecture Decisions, Checkpoint 1: Foundation Unit Tests, Checkpoint 2: Compilation & Migration (+11 more)

### Community 20 - "reducer.rs"
Cohesion: 0.21
Nodes (8): CoreInfo, apply(), AppModelState, MachineNode, SpaceNode, UNASSIGNED_SPACE, foreground_event_updates_display_name(), state_reducer()

### Community 21 - "2. Handshake (`hello`) & Mode Negotiation"
Cohesion: 0.18
Nodes (9): 1. Socket Path Discovery, 2. Handshake (`hello`) & Mode Negotiation, Core Default Socket Resolution (Observed on Linux), Framed Handshake Request, `hello` Response (Always Plain `\n`-Delimited JSON), `herdr-core` (`hc`) Wire Protocol Notes (Spike HC-S0), `htui` Client Discovery Precedence, Plain Handshake Request (+1 more)

### Community 22 - "spawn.rs"
Cohesion: 0.24
Nodes (5): default_shell(), default_shell_detection_from_env(), default_shell_from_env(), spawn_target_defaults_and_with_target_propagates_to_outcome_and_render(), SpawnDialog

### Community 23 - "tabs.rs"
Cohesion: 0.19
Nodes (13): make_term(), narrow_tab_bar_prioritizes_new_terminal_button(), NEW_TERMINAL_TAB_LABEL, render_tab_bar(), renders_panel_pills_multipane_count_unattached_and_action_buttons(), SPLIT_H_LABEL, SPLIT_V_LABEL, TabHitbox (+5 more)

### Community 24 - "Implementation Plan: Unified Action Button Styling (New Space & New Terminal)"
Cohesion: 0.11
Nodes (17): 1. Theme Module (`src/ui/theme.rs`), 2. Tab Bar (`src/ui/tabs.rs` & `tests/ui_snapshots.rs`), 3. Empty-Pane Spawn Button (`src/ui/panes.rs` & `tests/ui_snapshots.rs`), 4. Sidebar New Space Action (`src/ui/sidebar.rs` & `tests/ui_snapshots.rs`), Checkpoint: Full Regression & Build, Checkpoint: Tab Bar & Panes, Checkpoint: Theme Foundation, Implementation Plan: Unified Action Button Styling (New Space & New Terminal) (+9 more)

### Community 25 - "Task Breakdown: Fix Terminal Text Corruption on Outer Window & Pane Resize"
Cohesion: 0.08
Nodes (24): Architecture Decisions, Checkpoint: After Tasks 1–2, Checkpoint: After Tasks 3–4, Checkpoint: Complete, Dependency Graph, Implementation Plan: Fix Terminal Text Corruption on Outer Window & Pane Resize, Open Questions, Overview (+16 more)

### Community 26 - "hc_client.rs"
Cohesion: 0.12
Nodes (13): AutoStartOutcome, AlreadyRunning, SkippedCustomSocket, Started, ensure_server_running(), ensure_server_running_with(), auto_start_already_running_does_not_spawn(), auto_start_fails_fast_on_nonzero_exit() (+5 more)

### Community 27 - "term.rs"
Cohesion: 0.13
Nodes (13): make_terminal(), assert_clean_zsh_screen(), attach_session_drains_buffered_old_width_frames_before_resize(), attach_session_emits_in_band_resize_on_active_snapshot_resize_flush_and_takeover(), attach_session_resize_lifecycle_observing_debounce_and_take_over_at(), attach_session_suppresses_in_band_resize_when_observing_or_mode_2048_disabled(), nvim_cargo_toml_frame(), nvim_cargo_toml_mode_2048_attach_shrink_grow_and_exit_to_zsh() (+5 more)

### Community 28 - "path"
Cohesion: 0.36
Nodes (3): run_events(), run_status(), status_and_events()

### Community 29 - "Task List: Isolated Multi-Panel Workspaces & Per-Panel Splits"
Cohesion: 0.12
Nodes (15): Checkpoint 1: Foundation Unit Tests, Checkpoint 2: Compilation & Migration, Checkpoint 3: Feature Complete, Final Checkpoint: Full Regression & Quality Gates, Phase 1: Layout Model, Spawn Targets & Keymap Foundation, Phase 2: Backward-Compatible Storage & Core `SpaceLayout` Adoption, Phase 3: Panel UI, Isolation, Lifecycle & Reconciliation, Phase 4: End-to-End Snapshot & Integration Verification (+7 more)

### Community 30 - "Spec: herdr-tui (`htui`)"
Cohesion: 0.12
Nodes (15): 0. Fundamental Architectural Assumptions & Core Constraints, 1. Objective, 2. Tech Stack & Dependencies, 3. Project Structure, 4. Module Specifications, 5. Acceptance Criteria, 6. Boundaries & Quality Constraints, Module: `cli` (+7 more)

### Community 31 - "hc/events.rs"
Cohesion: 0.22
Nodes (8): DEFAULT_HEARTBEAT_TIMEOUT, EventMessage, Event, Heartbeat, Missed, EventStream, HeartbeatPayload, SubscribeResult

### Community 32 - "ActiveDialog"
Cohesion: 0.20
Nodes (7): ActiveDialog, Confirm, Help, NewSpace, RenameSpace, Spawn, centered_rect()

### Community 33 - "Implementation Plan: Fix Remaining Side-by-Side Divider Breakage & Cross-Pane Text Bleeding During Drag"
Cohesion: 0.14
Nodes (13): Architecture Decisions, Checkpoint 1: Core Implementation, Checkpoint 2: Complete, Dependency Graph, Implementation Plan: Fix Remaining Side-by-Side Divider Breakage & Cross-Pane Text Bleeding During Drag, Open Questions, Overview, Phase 1: Terminal Cell Sanitization (+5 more)

### Community 34 - "event_loop.rs"
Cohesion: 0.09
Nodes (19): Task 3: Update & Verify `htui open --create` Startup, `switch_space`, `prune_empty_spaces`, `ClosePane`, and `Quit` Lifecycle, attach_active_panel_terminals(), attach_terminal_in_pane(), handle_core_event_with_framed(), init_disconnected_startup(), install_panic_hook(), now_epoch_ms(), prune_and_sync_empty_spaces() (+11 more)

### Community 35 - "confirm.rs"
Cohesion: 0.22
Nodes (5): ConfirmAction, KillBeforeRemove, KillTerminal, RemoveTerminal, ConfirmDialog

### Community 36 - "Implementation Plan: Replace `vt100` with `alacritty_terminal` Behind `ScreenModel`"
Cohesion: 0.15
Nodes (12): Architecture Decisions, Checkpoint 1: Core Migration, Checkpoint 2: Complete, Dependency Graph, Implementation Plan: Replace `vt100` with `alacritty_terminal` Behind `ScreenModel`, Open Questions, Overview, Phase 1: `AlacrittyScreen` Implementation & Wiring (+4 more)

### Community 37 - "Config"
Cohesion: 0.19
Nodes (8): AttachConfig, AttachMode, Active, Auto, Observe, Config, HcConfig, UiConfig

### Community 38 - "Implementation Plan: `Ctrl+b q` ClosePane Rebinding & Empty Space / Empty Split Cleanup"
Cohesion: 0.18
Nodes (11): Architecture Decisions, Checkpoint: Complete, Checkpoint: Keymap & Layout Primitives Verified, Dependency Graph, Implementation Plan: `Ctrl+b q` ClosePane Rebinding & Empty Space / Empty Split Cleanup, Overview, Phase 1: Keymap & Layout Foundations, Phase 2: Application & Event Loop Lifecycle Cleanup (+3 more)

### Community 39 - "Implementation Plan: Terminal Mouse Pass-Through in `htui`"
Cohesion: 0.17
Nodes (11): Architecture Decisions, Checkpoint: Complete, Checkpoint: Terminal Layer Verified, Dependency Graph, Implementation Plan: Terminal Mouse Pass-Through in `htui`, Overview, Phase 1: Terminal Protocol & Mouse Encoding, Phase 2: UI Hit-Testing, Routing & Dispatch (+3 more)

### Community 40 - "Terminal"
Cohesion: 0.17
Nodes (11): AttachResult, FirstMessage, Foreground, MissedPayload, Terminal, ModelInput, DeclareSpace, Event (+3 more)

### Community 41 - "presence.rs"
Cohesion: 0.18
Nodes (6): FOREIGN_RESIZE_INHIBIT_MS, LEASE_STALE_MS, PRESENCE_DOC_NAME, PresenceDoc, PresenceLease, PresenceManager

### Community 42 - "store.rs"
Cohesion: 0.18
Nodes (7): deserialize_spaces_map(), LAYOUT_DOC_NAME, MAX_CAS_RETRIES, SpaceLayout, SpaceLayoutWire, LegacyTree, MultiPanel

### Community 44 - "SessionState"
Cohesion: 0.13
Nodes (11): InputDisposition, IgnoredNotLive, Sent, SuppressedObserving, RESIZE_DEBOUNCE, SessionState, Active, Connecting (+3 more)

### Community 45 - "Implementation Plan: `Command::ClosePane` (`Ctrl+b q` / `Ctrl+b x`) Kills and Removes Terminal in `hc`"
Cohesion: 0.18
Nodes (10): Architecture Decisions, Checkpoint: Complete, Checkpoint: Core `ClosePane` Semantics Verified, Dependency Graph, Implementation Plan: `Command::ClosePane` (`Ctrl+b q` / `Ctrl+b x`) Kills and Removes Terminal in `hc`, Overview, Phase 1: `Command::ClosePane` Action & State Cleanup, Phase 2: End-to-End `FakeHcServer` Verification (+2 more)

### Community 46 - "Task List: Unified Action Button Styling (New Space & New Terminal)"
Cohesion: 0.18
Nodes (10): Checkpoint: Tab Bar & Panes, Checkpoint: Theme Foundation, Final Checkpoint: Full Regression & Build, Phase 1: Theme Style Contract & Unit Tests, Phase 2: Tab Bar & Empty Pane Button Styling, Phase 3: Sidebar New Space Action & Selection Highlight, Task 1: Update `Theme::new_space_action()` and `Theme::tab_new_terminal()` in `src/ui/theme.rs` and update unit tests, Task 2: Switch empty-pane spawn label in `src/ui/panes.rs` to `Theme::tab_new_terminal()`, update tab bar snapshot assertions and add empty-pane style snapshot test in `tests/ui_snapshots.rs` (+2 more)

### Community 47 - "HcError"
Cohesion: 0.16
Nodes (10): HcError, AutoStartFailed, ChannelClosed, HeartbeatTimeout, Io, Json, MissingMethods, Protocol (+2 more)

### Community 48 - "Task List: `Ctrl+b q` ClosePane Rebinding & Empty Space / Empty Split Cleanup"
Cohesion: 0.20
Nodes (9): Checkpoint: After Tasks 1-2, Checkpoint: Complete, Phase 1: Keymap & Layout Foundations, Phase 2: Application & Event Loop Lifecycle Cleanup, Task 1: Rebind `Ctrl+b q` to `Command::ClosePane` and Update `bindings_help()`, Task 2: Add `SplitTree::has_terminals`, `SplitTree::collapse_empty_splits`, and `LayoutStore::prune_empty_spaces`, Task 3: Enforce Empty-Space & Empty-Split Cleanup in `TuiApp`, Task 4: Wire Persistence Cleanup in `event_loop.rs` (`AppAction::Quit`, Startup, Core Events, `Missed`, Reconnect, `Spawn`) (+1 more)

### Community 49 - "Implementation Plan: `hc server start` Auto-Start on `htui` Startup"
Cohesion: 0.20
Nodes (9): Architecture Decisions, Checkpoint: Complete, Checkpoint: Transport Auto-Start Verified, Dependency Graph, Implementation Plan: `hc server start` Auto-Start on `htui` Startup, Overview, Phase 1: Transport Foundation & Readiness Polling, Phase 2: Entrypoint Wiring & End-to-End Verification (+1 more)

### Community 50 - "Task List: Terminal Mouse Pass-Through in `htui`"
Cohesion: 0.20
Nodes (9): Checkpoint: After Tasks 1-2, Checkpoint: Complete, Phase 1: Terminal Protocol & Mouse Encoding, Phase 2: UI Hit-Testing, Routing & Dispatch, Task 1: Expose Mouse Protocol Mode and Encoding on `ScreenModes`, Task 2: Implement Terminal Mouse Sequence Encoder (`encode_mouse`), Task 3: Add Passthrough Hit-Testing and Active Drag Tracking in `src/ui/mouse.rs`, Task 4: Wire `AppAction::SendInput`, Routing Priority, and End-to-End Dispatch (+1 more)

### Community 51 - "pathbuf"
Cohesion: 0.22
Nodes (6): Cli, Commands, Events, Ls, Open, Status

### Community 52 - "htui (`herdr-tui`)"
Cohesion: 0.20
Nodes (10): Architecture & Project Layout, Development & Testing, Highlights, htui (`herdr-tui`), Installation, License, Prerequisites, Quick Start (+2 more)

### Community 53 - "Task List: `hc server start` Auto-Start on `htui` Startup"
Cohesion: 0.22
Nodes (8): Checkpoint: After Tasks 1-2, Checkpoint: Complete, Phase 1: Transport Foundation & Readiness Polling, Phase 2: Entrypoint Wiring & End-to-End Verification, Task 1: Custom Socket Detection & Auto-Start Error Mapping, Task 2: `ensure_server_running` & Bounded Readiness Polling, Task 3: Wire Auto-Start Check into `src/main.rs` Startup Paths, Task List: `hc server start` Auto-Start on `htui` Startup

### Community 54 - "Implementation Plan: Fix Side-by-Side Terminal Resize Text Corruption"
Cohesion: 0.12
Nodes (15): Architecture Decisions, Checkpoint 1: Rendering Layer, Checkpoint 2: Session & Event Loop Synchronization, Checkpoint 3: Complete, Dependency Graph, Implementation Plan: Fix Side-by-Side Terminal Resize Text Corruption, Open Questions, Overview (+7 more)

### Community 55 - "Task Breakdown: Fix Side-by-Side Terminal Resize Text Corruption"
Cohesion: 0.22
Nodes (8): Checkpoint: After Task 1, Checkpoint: After Tasks 2-3, Checkpoint: Complete, Task 1: Harden `Vt100Screen::cell` and `Vt100Screen::render_to_buffer` against boundary, wide, zero-width, and stale-cell corruption, Task 2: Synchronize `AttachSession` screen resizing with actual PTY `RESIZE` transmission, Task 3: Coordinate `EndDrag`, `sync_pane_sizes`, `tick`, `Spawn`, `TakeOver`, and `"size"` events in `TuiApp` and `event_loop`, Task 4: Add comprehensive side-by-side resize regression tests and pass all quality gates, Task Breakdown: Fix Side-by-Side Terminal Resize Text Corruption

### Community 56 - "Implementation Plan: Inherit App Background for Panel Tab Bar & Footer Status Bar"
Cohesion: 0.22
Nodes (8): Architecture Decisions, Checkpoint: Complete, Dependency Graph, Implementation Plan: Inherit App Background for Panel Tab Bar & Footer Status Bar, Overview, Phase 1: Theme Styles & Rendered Verification, Risks and Mitigations, Task List

### Community 59 - "Task Breakdown: Replace `vt100` with `alacritty_terminal` Behind `ScreenModel`"
Cohesion: 0.25
Nodes (7): Checkpoint: After Tasks 1-2, Checkpoint: Complete, Task 1: Add `alacritty_terminal` dependency and implement `AlacrittyScreen` in `src/term/screen.rs`, Task 2: Wire `AlacrittyScreen` into `AttachSession::new`, migrate tests, and remove `vt100`, Task 3: Add `nvim`/`xterm-256color`, reflow, Arabic, scrollback, and mode regression tests, Task 4: Run full quality gates and reinstall `htui`, Task Breakdown: Replace `vt100` with `alacritty_terminal` Behind `ScreenModel`

### Community 60 - "Task List: `Command::ClosePane` (`Ctrl+b q` / `Ctrl+b x`) Kills and Removes Terminal in `hc`"
Cohesion: 0.25
Nodes (7): Checkpoint: After Task 1, Checkpoint: Complete, Phase 1: `Command::ClosePane` Action & State Cleanup, Phase 2: End-to-End `FakeHcServer` Verification, Task 1: Update `Command::ClosePane` to Emit `Kill`/`Remove`, Clean `model.terminals`, and Remove `AppAction::UnassignSpace`, Task 2: Enforce `live` Guard in `FakeHcServer` `terminal.rm` and Add Async `ClosePane` Dispatch Integration Test, Task List: `Command::ClosePane` (`Ctrl+b q` / `Ctrl+b x`) Kills and Removes Terminal in `hc`

### Community 61 - "Task Breakdown: Fix Remaining Side-by-Side Divider Breakage & Cross-Pane Text Bleeding During Drag"
Cohesion: 0.25
Nodes (7): Checkpoint: After Tasks 1-2, Checkpoint: Complete, Task 1: Harden `Vt100Screen::cell` against `Cf`, `U+00AD`, variation selectors, and ZWJ clusters, Task 2: Implement `SyncCrosstermBackend<W>` and wire it into `run_tui`, Task 3: Add ANSI byte-stream & side-by-side divider-drag integration tests, Task 4: Run full quality gates and reinstall `htui`, Task Breakdown: Fix Remaining Side-by-Side Divider Breakage & Cross-Pane Text Bleeding During Drag

### Community 62 - "StatusKind"
Cohesion: 0.24
Nodes (8): Status, StatusKind, Blocked, Done, Error, Idle, Other, Working

### Community 65 - "DialogOutcome"
Cohesion: 0.29
Nodes (7): DialogOutcome, Close, None, SubmitConfirm, SubmitNewSpace, SubmitRename, SubmitSpawn

### Community 67 - "side_by_side_zsh_rprompt_outer_resize_storm_and_divider_drag_with_sync_backend"
Cohesion: 0.21
Nodes (9): assert_moveto_follows_non_ascii(), assert_sync_framed(), nvim_mode_2048_payload(), SharedVecWriter, side_by_side_nvim_mode_2048_resize_storm_split_and_drag_with_sync_backend(), side_by_side_nvim_rep_and_colon_sgr_panes_with_sync_backend(), side_by_side_zsh_rprompt_outer_resize_storm_and_divider_drag_with_sync_backend(), sync_backend_side_by_side_divider_drag_emits_sync_and_post_non_ascii_moveto() (+1 more)

### Community 68 - "Task List: Inherit App Background for Panel Tab Bar & Footer Status Bar"
Cohesion: 0.33
Nodes (5): Checkpoint: Complete, Phase 1: Theme Styles & Rendered Verification, Task 1: Update `Theme::status_bar()`, `Theme::tab_bar_bg()`, `Theme::tab_new_terminal()`, and Theme Unit Tests, Task 2: Verify Rendered Tab Bar & Footer Background Inheritance in `tests/ui_snapshots.rs`, Task List: Inherit App Background for Panel Tab Bar & Footer Status Bar

### Community 71 - "Workspaces, Agents & Interactive Controls"
Cohesion: 0.22
Nodes (9): `Agents` Panel Detection & Status Glyphs, `Ctrl+B` Prefix Bindings, Direct Keys & Sidebar Navigation, Keybindings, Modal Dialogs, Mouse Controls, Passthrough, Scrollback & Clipboard, Observing vs Active Takeover, Workspace Hierarchy (+1 more)

### Community 79 - "Subcommands"
Cohesion: 0.25
Nodes (8): CLI Reference, Exit Codes, Global Flags, `htui events`, `htui ls`, `htui open`, `htui status`, Subcommands

### Community 81 - "dispatch_action"
Cohesion: 0.17
Nodes (13): dispatch_action(), handle_core_event(), attach_terminal_defaults_to_active_and_respects_observe_mode(), dispatch_action_send_input_forwards_bytes_over_framed_channel(), switch_back_reuses_existing_active_session(), close_pane_dispatches_kill_and_remove_against_fake_hc_and_store(), close_pane_empty_last_pane_with_unattached_dispatches_against_fake_hc_and_store(), core_labels_and_exit_events_prune_empty_spaces_from_store() (+5 more)

### Community 82 - "4. `events.subscribe` & Notification Payloads"
Cohesion: 0.40
Nodes (5): 4. `events.subscribe` & Notification Payloads, `event` Notification, `heartbeat` Notification (Every 15s), Initial Snapshot (RPC Response to `events.subscribe`), `missed` Notification

### Community 88 - "3. Binary Frame Wire Layout (Framed Mode)"
Cohesion: 0.50
Nodes (4): 3. Binary Frame Wire Layout (Framed Mode), Empirically Verified Frame Kinds, Frame Header (9 Bytes, Little-Endian), JSON-RPC in Framed Mode

### Community 89 - "Task List: Add Comprehensive Root `README.md` for `htui` (`herdr-tui`)"
Cohesion: 0.20
Nodes (9): Checkpoint: After Tasks 1–2, Checkpoint: Complete, Phase 1: Overview, Setup, CLI & Configuration, Phase 2: Interactive Controls, Terminal Emulation & Developer Guide, Task 1: Author `README.md` Overview, UI Layout Diagram, Prerequisites, Installation & Quick Start, Task 2: Document CLI Reference, Exit Codes, Socket Discovery, `hc` Auto-Start & Configuration, Task 3: Document Workspaces, Agents, Takeover, Keybindings, Mouse Controls & Dialogs, Task 4: Document Terminal Emulation Highlights, Architecture, Testing & License (+1 more)

### Community 94 - "Socket Discovery, Environment Variables & `hc` Auto-Start"
Cohesion: 0.50
Nodes (4): Environment Variables, `hc` Auto-Start Behavior, Socket Discovery, Environment Variables & `hc` Auto-Start, Socket Discovery Precedence

### Community 95 - "Configuration & Layout Persistence"
Cohesion: 0.67
Nodes (3): `config.toml`, Configuration & Layout Persistence, Layout Persistence (`htui.layout` & `layout.json`)

## Knowledge Gaps
- **149 isolated node(s):** `Down`, `Left`, `Right`, `Up`, `Horizontal` (+144 more)
  These have ≤1 connection - possible missing edges or undocumented components. (Counts symbols only; 678 node(s) total have ≤1 connection when file, concept and rationale nodes are included.)
- **25 thin communities (<3 nodes) omitted from report** — run `graphify query` to explore isolated nodes.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `HcErrorKind` connect `HcErrorKind` to `presence.rs`, `types.rs`, `store.rs`, `HcError`, `cli.rs`, `transport.rs`, `hc_client.rs`, `path`?**
  _High betweenness centrality (0.069) - this node is a cross-community bridge._
- **What connects `Down`, `Left`, `Right` to the rest of the system?**
  _149 weakly-connected nodes found - possible documentation gaps or missing edges._
- **Should `PaneId` be split into smaller, more focused modules?**
  _Cohesion score 0.0658835546475996 - nodes in this community are weakly interconnected._
- **Why does `SpaceLayout` connect `PaneId` to `TuiApp`, `event_loop.rs`, `layout.rs`, `store.rs`, `ui_snapshots.rs`, `tabs.rs`?**
  _High betweenness centrality (0.053) - this node is a cross-community bridge._
- **Should `TuiApp` be split into smaller, more focused modules?**
  _Cohesion score 0.06715506715506715 - nodes in this community are weakly interconnected._
- **Why does `TuiApp` connect `TuiApp` to `PaneId`, `ActiveDialog`, `event_loop.rs`, `Config`, `sidebar.rs`, `Command`, `ui/mouse.rs`, `LayoutStore`, `HcResult`, `ui_snapshots.rs`, `dispatch_action`, `reducer.rs`, `tabs.rs`?**
  _High betweenness centrality (0.051) - this node is a cross-community bridge._
- **Should `screen.rs` be split into smaller, more focused modules?**
  _Cohesion score 0.05760905760905761 - nodes in this community are weakly interconnected._