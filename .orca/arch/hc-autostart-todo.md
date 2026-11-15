# Task List: `hc server start` Auto-Start on `htui` Startup

---

## Phase 1: Transport Foundation & Readiness Polling

## Task 1: Custom Socket Detection & Auto-Start Error Mapping

**Description:** Add helper functions in `src/hc/transport.rs` (`is_custom_socket` and `is_custom_socket_from_env`) to determine whether the resolved socket path came from an explicit override (`--socket`, `HTUI_HC_SOCKET`, or `config.hc.socket`) versus the default session socket. Add an `HcError::AutoStartFailed` variant in `src/hc/error.rs` mapping missing `hc` binaries to `HcErrorKind::HcMissing` and startup/timeout failures to `HcErrorKind::SpawnFailed`.

**Acceptance criteria:**
- [x] `is_custom_socket(cli, env, cfg)` returns `true` when any of `cli`, non-empty `env`, or `cfg` is provided, and `false` when falling back to `default_core_socket_path()`.
- [x] `HcError::AutoStartFailed { kind, message }` reports `HcErrorKind::HcMissing` or `HcErrorKind::SpawnFailed` via `HcError::kind()`.
- [x] Unit tests in `src/hc/transport.rs` verify `is_custom_socket` across all precedence combinations (including empty/whitespace `HTUI_HC_SOCKET`).

**Verification:**
- [x] Tests pass: `cargo test --lib hc::transport`
- [x] Build succeeds: `cargo check --all-targets`

**Dependencies:** None

**Files likely touched:**
- `src/hc/error.rs`
- `src/hc/transport.rs`

**Estimated scope:** Small (2 files)

---

## Task 2: `ensure_server_running` & Bounded Readiness Polling

**Description:** Implement `AutoStartOutcome` (`AlreadyRunning`, `Started`, `SkippedCustomSocket`), `ensure_server_running_with(socket_path, is_custom_socket, hc_bin, timeout)`, and `ensure_server_running(socket_path, is_custom_socket)` in `src/hc/transport.rs`. When `socket_path` is unreachable and `is_custom_socket` is `false`, spawn `<hc_bin> server start` with `stdin`/`stdout`/`stderr` set to `Stdio::null()`, poll `connect_unix(socket_path)` every 50ms up to `timeout`, fail fast if `child.try_wait()` reports a non-zero exit while the socket remains unreachable, and kill/reap the child if the timeout expires.

**Acceptance criteria:**
- [x] Returns `Ok(AutoStartOutcome::AlreadyRunning)` immediately without spawning a process when `connect_unix(socket_path)` succeeds.
- [x] Returns `Ok(AutoStartOutcome::SkippedCustomSocket)` without spawning a process when `is_custom_socket` is `true` and the socket is unreachable.
- [x] Returns `Err(HcError)` with `kind() == HcErrorKind::HcMissing` when `hc_bin` does not exist, and `HcErrorKind::SpawnFailed` when the process exits non-zero or times out before the socket becomes reachable.
- [x] Returns `Ok(AutoStartOutcome::Started)` once the spawned process creates and listens on `socket_path` within the timeout window.

**Verification:**
- [x] Tests pass: `cargo test --test hc_client`
- [x] Build succeeds: `cargo check --all-targets`

**Dependencies:** Task 1

**Files likely touched:**
- `src/hc/transport.rs`
- `tests/hc_client.rs`

**Estimated scope:** Small (2 files)

---

## Checkpoint: After Tasks 1-2
- [x] `cargo test --lib hc::transport` and `cargo test --test hc_client` pass
- [x] Application builds without warnings (`cargo clippy --all-targets -- -D warnings`)

---

## Phase 2: Entrypoint Wiring & End-to-End Verification

## Task 3: Wire Auto-Start Check into `src/main.rs` Startup Paths

**Description:** Update `src/main.rs` so that TUI startup commands (`Some(Commands::Open { .. })` and `None`) call `ensure_server_running(&socket_path, is_custom)` before `validate_space_exists` and before `run_tui` enters raw mode. If `ensure_server_running` returns an error, log a concise warning to `stderr` (`eprintln!("warning: {e}");`) and allow `validate_space_exists` (which exits with code 3 on unreachable socket) or `run_tui` (which enters disconnected retry state) to proceed cleanly.

**Acceptance criteria:**
- [x] `htui` (no subcommand) and `htui open <space>` invoke `ensure_server_running` before `validate_space_exists` and before `enable_raw_mode()`.
- [x] `htui ls`, `htui status`, and `htui events` do not invoke auto-start and continue to exit with code 3 when the socket is unreachable (`CL-4`).
- [x] All unit and integration tests pass, and `cargo fmt --all -- --check` and `cargo clippy --all-targets -- -D warnings` succeed.

**Verification:**
- [x] Tests pass: `cargo test --all-targets`
- [x] Build & lint succeed: `cargo fmt --all -- --check && cargo clippy --all-targets -- -D warnings`

**Dependencies:** Task 2

**Files likely touched:**
- `src/main.rs`
- `tests/cli.rs`

**Estimated scope:** Small (2 files)

---

## Checkpoint: Complete
- [x] All acceptance criteria met
- [x] Full test suite, rustfmt, and clippy pass cleanly
- [x] Ready for human review
