# Implementation Plan: Isolated Multi-Panel Workspaces & Per-Panel Splits

## Overview
Refactor `htui`'s workspace layout model and TUI shell so every space holds **multiple isolated panels** (`SpaceLayout` containing `Vec<PanelLayout>`), where each individual panel owns its own `SplitTree`, focused pane, and terminals. Opening a new terminal (from `[+ New Terminal]` in the tab bar, `Ctrl+B n` on an occupied pane, or sidebar/external `hc` terminals) creates a new individual panel instead of overwriting the focused pane. Inside any panel, users can split and open new terminals horizontally or vertically (`Ctrl+B %`/`|` + `Ctrl+B n`, `Ctrl+B "`/`-` + `Ctrl+B n`, or direct `Ctrl+B H` / `Ctrl+B V` and tab-bar `[+H]` / `[+V]` controls) with full layout and terminal isolation between panels.

---

## Architecture Decisions

### 1. Multi-Panel Data Model (`src/layout/panel.rs`, `src/layout/mod.rs`)
- Introduce `pub type PanelId = u64;`, `PanelLayout`, and `SpaceLayout`:
  - `PanelLayout`: `{ id: PanelId, tree: SplitTree, focused_pane: Option<PaneId> }`
  - `SpaceLayout`: `{ panels: Vec<PanelLayout>, active_panel: Option<PanelId>, next_panel_id: PanelId }`
- `SpaceLayout::new()` initializes with one empty placeholder panel (`id: 1`, `SplitTree::with_single(None)`, `active_panel: Some(1)`, `next_panel_id: 2`).
- `SpaceLayout::from_tree(tree: SplitTree)` migrates a legacy single-`SplitTree` space into a single-panel `SpaceLayout` with `id: 1` and valid `focused_pane`.
- `SpaceLayout::add_panel(terminal)` appends a new `PanelLayout`, sets `active_panel` to it, and returns `(panel_id, pane_id)`.
- `SpaceLayout::remove_panel(id)` removes the panel and selects an adjacent remaining panel as `active_panel`.
- Per-panel pruning (`prune_removed`, `prune_terminal`, `collapse_empty_splits_or_prune`) removes panels that become empty when other panels remain in the space, or collapses empty splits in the sole remaining panel.

### 2. Backward-Compatible Persistence (`src/layout/store.rs`)
- Update `LayoutDoc.spaces` from `HashMap<String, SplitTree>` to `HashMap<String, SpaceLayout>`.
- Use an internal `#[serde(untagged)] enum SpaceLayoutWire { MultiPanel(SpaceLayout), LegacyTree(SplitTree) }` with a custom deserializer on `LayoutDoc.spaces`:
  - Existing `htui.layout` docs and `layout.json` files containing `{ "root": ..., "next_id": ... }` deserialize as `LegacyTree` and automatically migrate via `SpaceLayout::from_tree`.
  - Note: `SpaceLayout.panels` must NOT use `#[serde(default)]` during deserialization so `MultiPanel` and `LegacyTree` have disjoint required fields (`panels` vs `root` + `next_id`).
- `LayoutStore::save_space(&self, rpc, space_key, layout: &SpaceLayout)` and `prune_empty_spaces` operate on `SpaceLayout` via existing 3-retry CAS (`doc.get`/`doc.set`) and local fallback.

### 3. Spawn Targets & Keymap (`src/ui/dialogs/spawn.rs`, `src/ui/dialogs/mod.rs`, `src/ui/keymap.rs`)
- Introduce `SpawnTarget`:
  - `NewPanel` — creates a new individual panel in the target space (or populates the sole empty placeholder panel if the space has only 1 empty, unsplit pane).
  - `IntoPane { panel_id: PanelId, pane_id: PaneId }` — attaches into a specific empty pane inside a panel (used by `Ctrl+B n` on an empty split pane and `MouseAction::SpawnInPane`).
  - `SplitPane { panel_id: PanelId, pane_id: PaneId, axis: SplitAxis }` — splits `pane_id` along `axis` inside `panel_id` and attaches the spawned terminal into the newly created split pane (only when submitted; `Esc` leaves the tree untouched).
- `SpawnDialog::new(space, templates)` defaults to `SpawnTarget::NewPanel`; `SpawnDialog::with_target(space, templates, target)` sets an explicit target.
- Add prefix commands in `PrefixRouter` (`src/ui/keymap.rs`):
  - `Ctrl+B H` → `Command::SplitSpawnHorizontal` (opens `SpawnDialog` with `SplitPane { ..., Horizontal }`)
  - `Ctrl+B V` → `Command::SplitSpawnVertical` (opens `SpawnDialog` with `SplitPane { ..., Vertical }`)
  - `Ctrl+B [` → `Command::PrevPanel`
  - `Ctrl+B ]` → `Command::NextPanel`
  - Preserve `Ctrl+B % / |` (`SplitHorizontal`) and `Ctrl+B " / -` (`SplitVertical`) for split-then-spawn workflows inside the active panel.
  - Keep `PrefixRouter::bindings_help()` at ≤ 18 lines so all bindings fit in `HelpDialog` at `80x40`.

### 4. Isolated Panel Tab Bar & Mouse/Keyboard UX (`src/ui/tabs.rs`, `src/ui/app.rs`)
- Update `render_tab_bar` & `TabHitbox` (`src/ui/tabs.rs`):
  - Render one tab pill per `PanelLayout` in the active space's `SpaceLayout.panels`:
    - Label displays the panel's focused (or first) terminal's `status_glyph` + `display_name`, plus ` (+N)` when the panel has additional panes/terminals, or `(empty)` if empty.
    - Active panel uses `Theme::tab_active()`; inactive panels use `Theme::tab_inactive()`.
  - Also render any unattached terminals in `model.terminals` matching `active_space` (not yet present in `SpaceLayout`) as `TabHitbox::UnattachedTerminal { address, rect }` so they remain reachable and open into a new isolated panel on click.
  - Right-aligned action buttons: `[+H]` (`TabHitbox::SplitHorizontal`), `[+V]` (`TabHitbox::SplitVertical`), and `[+ New Terminal]` (`TabHitbox::NewTerminal`), styled with `Theme::tab_new_terminal()` (`OLD_WHITE` on `SUMI_INK3`, no bold) with `Color::Reset` 1-char spacing. On narrow widths, prioritize `[+ New Terminal]`.
- `TuiApp` (`src/ui/app.rs`):
  - `current_tree()` / `current_tree_mut()` return the active panel's `SplitTree` in `active_space`.
  - Switching panels (`switch_panel`, `next_panel`, `prev_panel`, tab click) persists the outgoing panel's `focused_pane`, switches `active_panel`, restores the incoming panel's `focused_pane`, clears `selection`, and emits `AttachTerminal` for any unattached terminals in the newly active panel.
  - `Ctrl+B n`: targets `IntoPane` if the focused pane is empty, or `NewPanel` if the focused pane is occupied.
  - Sidebar `Enter` / click on `SidebarItem::Terminal`: focuses the existing panel containing that terminal if already placed in `SpaceLayout`, or opens it in a new individual panel (reusing a sole empty placeholder pane if present) without overwriting occupied panes.

### 5. Lifecycle, Cleanup & External Terminal Reconciliation (`src/ui/app.rs`, `src/ui/event_loop.rs`)
- `Command::ClosePane` (`Ctrl+B x / q`):
  - Multi-pane active panel: kills/removes focused pane's terminal and closes that pane inside the active panel; if the panel becomes empty and other panels exist in the space, removes the empty panel and switches to an adjacent panel.
  - Single-pane active panel with other panels in the space: kills/removes its terminal, removes the panel, switches to an adjacent panel, and emits `SaveLayout`.
  - Last panel and last pane in the space: preserves existing space-closing & cleanup semantics (`close_space_terminals`, `remove_empty_space`, `RemoveLayout`, fallback to `next_named_space()` or `UNASSIGNED_SPACE`).
- `prune_empty_spaces`, `prune_terminal_from_spaces`, and `cleanup_empty_spaces_on_quit` operate across all panels in each `SpaceLayout`, pruning redundant empty panels while preserving single-panel invariants.
- `TuiApp::reconcile_live_terminals()` (called from `reconcile_startup_layouts`, `handle_core_event`, `Missed`, and reconnect in `src/ui/event_loop.rs`, NOT from `refresh_sidebar()` or `render()`):
  - Deterministically sorts unassigned live terminals by `(created_ms, address)` and places each into its own individual panel in its space (reusing a sole empty placeholder panel if present).
  - Does not steal `active_panel` on background events if the current panel is already occupied.

---

## Dependency Graph

```
src/layout/panel.rs (PanelId, PanelLayout, SpaceLayout)
    │
    ├──► src/layout/store.rs (LayoutDoc + legacy SplitTree migration)
    │        │
    │        ├──► src/ui/app.rs (multi-panel TuiApp state, lifecycle, reconciliation)
    │        │        │
    │        │        └──► src/ui/event_loop.rs (AppAction::Spawn target, startup/event reconciliation)
    │        │
    │        └──► tests/layout.rs (migration & multi-panel CAS/local tests)
    │
    ├──► src/ui/tabs.rs (panel tabs, [+H], [+V], [+ New Terminal])
    │
    └──► src/ui/dialogs/spawn.rs + src/ui/keymap.rs (SpawnTarget, SplitSpawnH/V, Prev/NextPanel)
```

---

## Task List

### Phase 1: Layout Model, Spawn Targets & Keymap Foundation
- [x] Task 1: Add `PanelId`, `PanelLayout`, and `SpaceLayout` in `src/layout/panel.rs` and export from `src/layout/mod.rs`
- [x] Task 2: Add `SpawnTarget` to `SpawnDialog` / `DialogOutcome` and add panel/split-spawn commands to `PrefixRouter` (`src/ui/keymap.rs`)

### Checkpoint 1: Foundation Unit Tests
- [x] `cargo test --lib` passes cleanly

### Phase 2: Backward-Compatible Storage & Core `SpaceLayout` Adoption
- [x] Task 3: Migrate `LayoutDoc` & `LayoutStore` in `src/layout/store.rs` to `HashMap<String, SpaceLayout>` with legacy `SplitTree` deserialization, update callers/tests to compile, and add migration tests in `tests/layout.rs`

### Checkpoint 2: Compilation & Migration
- [x] `cargo test` passes across all unit and integration suites

### Phase 3: Panel UI, Isolation, Lifecycle & Reconciliation
- [x] Task 4: Update `src/ui/tabs.rs` for isolated panel tabs + `[+H]`/`[+V]`/`[+ New Terminal]`, and implement panel switching, split-isolation, and target-aware spawn/sidebar routing in `src/ui/app.rs`
- [x] Task 5: Implement multi-panel `ClosePane`, per-panel pruning/cleanup, `AppAction::Spawn` target handling, and deterministic external terminal placement in `src/ui/app.rs` and `src/ui/event_loop.rs`

### Checkpoint 3: Feature Complete
- [x] `cargo test` passes with zero failures

### Phase 4: End-to-End Snapshot & Integration Verification
- [x] Task 6: Add comprehensive multi-panel isolation, splitting, tab bar, lifecycle, and startup reconciliation tests in `tests/ui_snapshots.rs` and `tests/layout.rs`, and run `fmt` + `clippy`

### Final Checkpoint: Ready for Review
- [x] All 7 acceptance criteria verified by automated tests
- [x] `cargo fmt --all -- --check` and `cargo clippy --all-targets -- -D warnings` pass

---

## Risks and Mitigations

| Risk | Impact | Mitigation |
|---|---|---|
| Legacy `htui.layout` / `layout.json` files fail to load after schema change | High | Use `#[serde(untagged)] enum SpaceLayoutWire` with disjoint required fields (`panels` vs `root` + `next_id`) and test both Doc API and local JSON migration in `tests/layout.rs`. |
| Existing unit/snapshot tests expect `refresh_sidebar()` not to auto-create panels for manually inserted `model.terminals` | High | Run `reconcile_live_terminals()` only in `event_loop.rs` (`reconcile_startup_layouts`, `handle_core_event`, `Missed`, reconnect) and render unattached terminals in `render_tab_bar` as clickable tabs opening a new panel. |
| Adding `[+H]` / `[+V]` in `render_tab_bar` overflows narrow terminals or breaks existing `tab_bar_and_footer_inherit_app_background` | Medium | Prioritize `[+ New Terminal]` when `area.width` is constrained; update `tab_bar_and_footer_inherit_app_background` assertions to cover `[+H]`, `[+V]`, and `[+ New Terminal]` spacing. |
| `HelpDialog` truncates keybindings if `bindings_help()` exceeds modal height | Low | Keep `PrefixRouter::bindings_help()` at ≤ 18 entries (fits inside `80x40` `HelpDialog` inner height) and verify via `dialogs_snapshot` test. |
| `unwrap()` / `expect()` introduced in library code (forbidden by `SPEC.md`) | Medium | Avoid `unwrap()`/`expect()` in `src/` outside `#[cfg(test)]`; enforce via `cargo clippy --all-targets -- -D warnings`. |
