# Task List: Isolated Multi-Panel Workspaces & Per-Panel Splits

## Phase 1: Layout Model, Spawn Targets & Keymap Foundation

### Task 1: Add `PanelId`, `PanelLayout`, and `SpaceLayout` in `src/layout/panel.rs` and export from `src/layout/mod.rs`
**Description:** Create `src/layout/panel.rs` defining `PanelId` (`u64`), `PanelLayout` (`id`, `tree: SplitTree`, `focused_pane: Option<PaneId>`), and `SpaceLayout` (`panels: Vec<PanelLayout>`, `active_panel: Option<PanelId>`, `next_panel_id: PanelId`) with helper methods for panel creation, switching, `from_tree` migration, `find_panel_by_terminal`, `has_terminals`, and per-panel pruning/collapsing. Re-export `PanelId`, `PanelLayout`, and `SpaceLayout` from `src/layout/mod.rs`. Add thorough unit tests in `src/layout/panel.rs`.

**Acceptance criteria:**
- [x] `PanelLayout` owns an isolated `SplitTree` and `focused_pane: Option<PaneId>`, with `new`, `from_tree`, `has_terminals`, `terminal_addresses`, `contains_terminal`, and `ensure_valid_focus`.
- [x] `SpaceLayout` supports `new`, `with_single`, `from_tree`, `add_panel`, `remove_panel`, `set_active_panel`, `active_panel` / `active_panel_mut`, `next_panel_id` / `prev_panel_id` cycling, `find_panel_by_terminal`, `has_terminals`, `prune_removed`, and `prune_terminal`.
- [x] Removing a panel updates `active_panel` to an adjacent remaining panel; pruning a terminal removes an emptied panel when other panels remain or collapses empty splits when it is the last panel.
- [x] Unit tests in `src/layout/panel.rs` verify panel isolation, splitting within one panel without affecting another, panel cycling, and pruning behavior.

**Verification:**
- [x] Tests pass: `cargo test --lib layout::panel::tests`
- [x] Build succeeds: `cargo check --all-targets`

**Dependencies:** None

**Files likely touched:**
- `src/layout/panel.rs`
- `src/layout/mod.rs`

**Estimated scope:** Small (2 files)

---

### Task 2: Add `SpawnTarget` to `SpawnDialog` / `DialogOutcome` and add panel/split-spawn commands to `PrefixRouter` (`src/ui/keymap.rs`)
**Description:** Introduce `SpawnTarget` (`NewPanel`, `IntoPane { panel_id, pane_id }`, `SplitPane { panel_id, pane_id, axis }`) in `src/ui/dialogs/spawn.rs` and include it in `SpawnDialog` and `DialogOutcome::SubmitSpawn`. Keep `SpawnDialog::new` defaulting to `SpawnTarget::NewPanel` and add `SpawnDialog::with_target`. In `src/ui/keymap.rs`, add `Command::SplitSpawnHorizontal` (`Ctrl+B H`), `Command::SplitSpawnVertical` (`Ctrl+B V`), `Command::PrevPanel` (`Ctrl+B [`), and `Command::NextPanel` (`Ctrl+B ]`), updating `bindings_help()` (keeping total lines <= 18) and adding unit tests. Wire stub match arms in `src/ui/app.rs` (`AppAction::Spawn` still builds cleanly).

**Acceptance criteria:**
- [x] `SpawnTarget` is defined and carried through `SpawnDialog` and `DialogOutcome::SubmitSpawn`.
- [x] `SpawnDialog::render` clearly indicates when spawning into a new panel vs horizontal/vertical split.
- [x] `PrefixRouter` routes `Ctrl+B H` → `SplitSpawnHorizontal`, `Ctrl+B V` → `SplitSpawnVertical`, `Ctrl+B [` → `PrevPanel`, and `Ctrl+B ]` → `NextPanel`, while preserving `%`/`|` and `"`/`-`.
- [x] `PrefixRouter::bindings_help()` lists all new bindings and stays <= 18 entries so `dialogs_snapshot` passes without clipping.

**Verification:**
- [x] Tests pass: `cargo test --lib ui::keymap::tests`
- [x] Tests pass: `cargo test --lib ui::dialogs::spawn::tests`
- [x] Tests pass: `cargo test --test ui_snapshots dialogs_snapshot`
- [x] Build succeeds: `cargo check --all-targets`

**Dependencies:** Task 1

**Files likely touched:**
- `src/ui/dialogs/spawn.rs`
- `src/ui/dialogs/mod.rs`
- `src/ui/keymap.rs`
- `src/ui/app.rs` (minimal compile-keeping match arms)

**Estimated scope:** Medium (3-4 files)

---

## Checkpoint 1: Foundation Unit Tests
- [x] `cargo test --lib` passes with 0 failures
- [x] `cargo check --all-targets` builds cleanly

---

## Phase 2: Backward-Compatible Storage & Core `SpaceLayout` Adoption

### Task 3: Migrate `LayoutDoc` & `LayoutStore` in `src/layout/store.rs` to `HashMap<String, SpaceLayout>` with legacy `SplitTree` deserialization
**Description:** Update `LayoutDoc.spaces` in `src/layout/store.rs` to `HashMap<String, SpaceLayout>` using an internal `#[serde(untagged)] enum SpaceLayoutWire` (`MultiPanel(SpaceLayout)` | `LegacyTree(SplitTree)`) so legacy single-tree documents deserialize cleanly into 1-panel `SpaceLayout`s while saving persists the multi-panel format. Update `LayoutStore::save_space` and `prune_empty_spaces` for `SpaceLayout`. Adapt `src/ui/app.rs`, `src/ui/event_loop.rs`, `tests/layout.rs`, and existing direct `layouts.spaces` / `SplitTree` references in `tests/ui_snapshots.rs` to use `SpaceLayout`, and add legacy migration + multi-panel persistence tests in `tests/layout.rs`.

**Acceptance criteria:**
- [x] `LayoutDoc` deserializes legacy JSON (`{"spaces": {"api": {"root": ..., "next_id": ...}}}`) into a 1-panel `SpaceLayout` preserving all split nodes, ratios, and `TermRef`s, and serializes as multi-panel `SpaceLayout`.
- [x] `LayoutStore::save_space`, `load`, and `prune_empty_spaces` work via both `doc.get`/`doc.set` CAS API and local `layout.json` fallback.
- [x] `tests/layout.rs` includes dedicated tests for legacy `SplitTree` migration (Doc API + local fallback) and multi-panel round-trip persistence.
- [x] Existing `TuiApp`, `event_loop`, and `ui_snapshots` callers compile and pass with `SpaceLayout`.

**Verification:**
- [x] Tests pass: `cargo test --test layout`
- [x] Full suite passes: `cargo test`

**Dependencies:** Task 1, Task 2

**Files likely touched:**
- `src/layout/store.rs`
- `src/ui/app.rs`
- `src/ui/event_loop.rs`
- `tests/layout.rs`
- `tests/ui_snapshots.rs`

**Estimated scope:** Medium (5 files)

---

## Checkpoint 2: Compilation & Migration
- [x] `cargo test` passes completely across all unit and integration tests
- [x] Legacy layout deserialization verified in `tests/layout.rs`

---

## Phase 3: Panel UI, Isolation, Lifecycle & Reconciliation

### Task 4: Update `src/ui/tabs.rs` for isolated panel tabs + split actions, and implement panel switching & target-aware spawn/sidebar routing in `src/ui/app.rs`
**Description:** Update `render_tab_bar` and `TabHitbox` in `src/ui/tabs.rs` to render one tab pill per `PanelLayout` in `active_space` (showing focused/primary terminal glyph + name, and ` (+N)` when a panel holds multiple panes/terminals), plus any unattached space terminals as `TabHitbox::UnattachedTerminal`, plus `[+H]` (`TabHitbox::SplitHorizontal`), `[+V]` (`TabHitbox::SplitVertical`), and `[+ New Terminal]` (`TabHitbox::NewTerminal`). In `src/ui/app.rs`, implement `switch_panel`, `next_panel`, `prev_panel`, `attach_unattached_in_active_panel`, and update `handle_key` / `handle_mouse_event` so:
- `[+ New Terminal]` and `Ctrl+B n` (on an occupied pane) target `SpawnTarget::NewPanel`.
- `Ctrl+B n` (on an empty pane) and `MouseAction::SpawnInPane` target `SpawnTarget::IntoPane`.
- `Ctrl+B H` / `Ctrl+B V` and tab-bar `[+H]` / `[+V]` target `SpawnTarget::SplitPane`.
- Clicking a panel tab or pressing `Ctrl+B [` / `]` switches the active panel, restores its `focused_pane`, and attaches any unattached terminals in that panel.
- Clicking or pressing Enter on a terminal in the sidebar (`SidebarItem::Terminal`) focuses its existing panel or creates a new individual panel rather than overwriting an occupied pane.

**Acceptance criteria:**
- [ ] `render_tab_bar` renders isolated panel pills (including multi-pane count indicator ` (+N)`), unattached terminal pills, `[+H]`, `[+V]`, and `[+ New Terminal]` with `Theme::tab_new_terminal()` styling and `Color::Reset` spacing.
- [ ] Clicking a panel tab or pressing `Ctrl+B [` / `]` switches `active_panel`, updates `focused_pane` to that panel's focused pane, and preserves other panels' layouts.
- [ ] Opening a spawn dialog from `[+ New Terminal]` or an occupied pane sets `SpawnTarget::NewPanel`; from an empty split pane sets `IntoPane`; from `Ctrl+B H/V` or `[+H]`/`[+V]` sets `SplitPane`.
- [ ] Sidebar terminal selection never overwrites an occupied pane; it focuses the terminal's panel or opens a new isolated panel.

**Verification:**
- [ ] Tests pass: `cargo test --test ui_snapshots tab_bar`
- [ ] Tests pass: `cargo test --test ui_snapshots sidebar`
- [ ] Build succeeds: `cargo check --all-targets`

**Dependencies:** Task 3

**Files likely touched:**
- `src/ui/tabs.rs`
- `src/ui/app.rs`
- `tests/ui_snapshots.rs` (update `tab_bar_and_footer_inherit_app_background` for `[+H]`/`[+V]` + `TabHitbox` variants)

**Estimated scope:** Medium (3 files)

---

### Task 5: Implement multi-panel lifecycle cleanup, `AppAction::Spawn` target execution, and external terminal reconciliation in `src/ui/app.rs` and `src/ui/event_loop.rs`
**Description:** Update `AppAction::Spawn` in `src/ui/app.rs` and `src/ui/event_loop.rs` to carry `target: SpawnTarget` and execute `NewPanel`, `IntoPane`, and `SplitPane` cleanly (reusing a sole 1-pane empty placeholder panel on `NewPanel`, creating a new isolated panel otherwise, and saving `SpaceLayout`). Update `Command::ClosePane`, `prune_empty_spaces`, `prune_terminal_from_spaces`, and `cleanup_empty_spaces_on_quit` in `src/ui/app.rs` to operate per-panel (closing a panel when its last pane closes while other panels exist, and removing the space only when its last panel closes). Add `TuiApp::reconcile_live_terminals()` and invoke it in `reconcile_startup_layouts`, `handle_core_event`, `Missed`, and reconnect in `src/ui/event_loop.rs` so external/pre-existing `hc` terminals are deterministically placed into individual panels.

**Acceptance criteria:**
- [ ] `AppAction::Spawn` with `SpawnTarget::NewPanel` creates a new isolated panel and focuses it without modifying existing panels; `IntoPane` fills the targeted empty pane; `SplitPane` splits the targeted pane along `axis` inside its panel and attaches the new terminal.
- [ ] `Command::ClosePane` on a multi-pane panel closes only that pane; on a single-pane panel when other panels exist in the space, removes only that panel and switches to an adjacent panel; on the last panel in the space, cleans up the space as before.
- [ ] `prune_empty_spaces`, `prune_terminal_from_spaces`, and `cleanup_empty_spaces_on_quit` remove emptied panels when other panels remain and remove emptied spaces when no panels have terminals.
- [ ] `reconcile_startup_layouts` and core event reconciliation deterministically place unassigned live terminals (sorted by `(created_ms, address)`) into individual panels without merging them into one panel or overwriting existing panels.

**Verification:**
- [ ] Tests pass: `cargo test --test ui_snapshots`
- [ ] Tests pass: `cargo test --test layout`
- [ ] Full suite passes: `cargo test`

**Dependencies:** Task 4

**Files likely touched:**
- `src/ui/app.rs`
- `src/ui/event_loop.rs`

**Estimated scope:** Small-Medium (2 files)

---

## Checkpoint 3: Feature Complete
- [ ] `cargo test` passes across all unit and integration tests
- [ ] All panel isolation, splitting, closing, and reconciliation behaviors implemented

---

## Phase 4: End-to-End Snapshot & Integration Verification

### Task 6: Add comprehensive multi-panel isolation, split, lifecycle, and startup reconciliation tests
**Description:** Add thorough integration and UI snapshot tests in `tests/ui_snapshots.rs` and `tests/layout.rs` covering every acceptance criterion:
1. Opening a new terminal (`[+ New Terminal]`, `Ctrl+B n` from occupied pane, sidebar selection) creates a new individual panel without overwriting existing panes.
2. Splitting horizontally and vertically within a panel (`Ctrl+B %`/`|` + `Ctrl+B n`, `Ctrl+B "`/`-` + `Ctrl+B n`, `Ctrl+B H`/`V`, `[+H]`/`[+V]` tab-bar buttons) splits only that panel's `SplitTree`.
3. Panel isolation & switching (`Ctrl+B [`/`]`, tab click) preserves independent `SplitTree`s and `focused_pane` per panel.
4. Tab bar rendering clearly distinguishes isolated panels (including multi-pane counts) and action buttons.
5. Closing panes/panels (`Ctrl+B x`), `finished`/`removed` core events, `prune_empty_spaces`, `prune_terminal_from_spaces`, and `cleanup_empty_spaces_on_quit` behave cleanly per-panel.
6. Startup reconciliation (`reconcile_startup_layouts`) places multiple pre-existing/external `hc` terminals into individual panels deterministically and migrates legacy `SplitTree` `LayoutDoc`s cleanly.
Run `cargo fmt --all -- --check` and `cargo clippy --all-targets -- -D warnings`.

**Acceptance criteria:**
- [ ] Dedicated tests in `tests/ui_snapshots.rs` verify end-to-end `AppAction::Spawn` (`NewPanel`, `IntoPane`, `SplitPane`), panel isolation & switching, multi-panel `ClosePane`, tab-bar `[+H]`/`[+V]`/`[+ New Terminal]` clicks, sidebar terminal click isolation, and `reconcile_startup_layouts` external terminal placement.
- [ ] Dedicated tests in `tests/layout.rs` verify legacy `SplitTree` JSON migration and multi-panel persistence under CAS conflicts and local fallback.
- [ ] `cargo test` passes with 0 failures and 0 skipped tests.
- [ ] `cargo fmt --all -- --check` and `cargo clippy --all-targets -- -D warnings` exit with code 0.

**Verification:**
- [ ] Tests pass: `cargo test`
- [ ] Formatter check: `cargo fmt --all -- --check`
- [ ] Linter check: `cargo clippy --all-targets -- -D warnings`

**Dependencies:** Task 5

**Files likely touched:**
- `tests/ui_snapshots.rs`
- `tests/layout.rs`

**Estimated scope:** Small (2 files)

---

## Final Checkpoint: Full Regression & Quality Gates
- [ ] Full test suite passes: `cargo test`
- [ ] Formatting clean: `cargo fmt --all -- --check`
- [ ] Clippy clean with `-D warnings`: `cargo clippy --all-targets -- -D warnings`
- [ ] All 7 acceptance criteria verified
