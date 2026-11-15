use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use tokio::net::UnixStream;

use crate::hc::error::{HcError, HcErrorKind, HcResult};

/// Resolves the `hc` Unix domain socket path with precedence:
/// 1. CLI `--socket`
/// 2. Env `HTUI_HC_SOCKET`
/// 3. Config `config.hc.socket`
/// 4. Default core socket (`$XDG_RUNTIME_DIR/herdr-core/${HC_SESSION:-default}.sock`,
///    or `/run/user/<uid>/herdr-core/...` / `/tmp/herdr-core-<uid>/...`).
pub fn resolve_socket_path(
    cli_socket: Option<&Path>,
    env_socket: Option<&str>,
    config_socket: Option<&Path>,
) -> PathBuf {
    if let Some(p) = cli_socket {
        return p.to_path_buf();
    }
    if let Some(e) = env_socket.filter(|s| !s.trim().is_empty()) {
        return PathBuf::from(e);
    }
    if let Some(c) = config_socket {
        return c.to_path_buf();
    }
    default_core_socket_path()
}

pub fn resolve_socket_from_env(cli_socket: Option<&Path>, config_socket: Option<&Path>) -> PathBuf {
    let env_val = std::env::var("HTUI_HC_SOCKET").ok();
    resolve_socket_path(cli_socket, env_val.as_deref(), config_socket)
}

/// Returns `true` if a custom socket override was provided via CLI flag,
/// non-empty environment variable, or configuration file.
pub fn is_custom_socket(
    cli_socket: Option<&Path>,
    env_socket: Option<&str>,
    config_socket: Option<&Path>,
) -> bool {
    cli_socket.is_some()
        || env_socket.map(|s| !s.trim().is_empty()).unwrap_or(false)
        || config_socket.is_some()
}

/// Helper to determine if a custom socket is active using `HTUI_HC_SOCKET`.
pub fn is_custom_socket_from_env(cli_socket: Option<&Path>, config_socket: Option<&Path>) -> bool {
    let env_val = std::env::var("HTUI_HC_SOCKET").ok();
    is_custom_socket(cli_socket, env_val.as_deref(), config_socket)
}

pub fn default_core_socket_path() -> PathBuf {
    let session = std::env::var("HC_SESSION")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "default".to_string());
    let sock_name = format!("{session}.sock");

    if let Ok(xdg) = std::env::var("XDG_RUNTIME_DIR") {
        if !xdg.trim().is_empty() {
            return PathBuf::from(xdg).join("herdr-core").join(sock_name);
        }
    }

    #[cfg(target_os = "linux")]
    {
        // Safety-free uid lookup via /proc/self/status or nix-free fallback:
        if let Some(uid) = current_uid_linux() {
            let run_user = PathBuf::from(format!("/run/user/{uid}"));
            if run_user.is_dir() {
                return run_user.join("herdr-core").join(sock_name);
            }
            return PathBuf::from(format!("/tmp/herdr-core-{uid}")).join(sock_name);
        }
    }

    std::env::temp_dir().join("herdr-core").join(sock_name)
}

#[cfg(target_os = "linux")]
fn current_uid_linux() -> Option<u32> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    for line in status.lines() {
        if let Some(rest) = line.strip_prefix("Uid:") {
            let first = rest.split_whitespace().next()?;
            return first.parse::<u32>().ok();
        }
    }
    None
}

pub async fn connect_unix(path: &Path) -> HcResult<UnixStream> {
    UnixStream::connect(path)
        .await
        .map_err(|source| HcError::SocketUnreachable {
            path: path.display().to_string(),
            source,
        })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutoStartOutcome {
    AlreadyRunning,
    Started,
    SkippedCustomSocket,
}

/// Ensures `herdr-core` (`hc`) is running on `socket_path`.
///
/// Uses default 5s timeout and `HTUI_HC_BIN` (defaulting to `"hc"`).
pub async fn ensure_server_running(
    socket_path: &Path,
    is_custom_socket: bool,
) -> HcResult<AutoStartOutcome> {
    let hc_bin = std::env::var("HTUI_HC_BIN")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "hc".to_string());

    ensure_server_running_with(
        socket_path,
        is_custom_socket,
        &hc_bin,
        Duration::from_secs(5),
    )
    .await
}

/// Parameter-injected implementation of auto-start check with bounded readiness polling.
pub async fn ensure_server_running_with(
    socket_path: &Path,
    is_custom_socket: bool,
    hc_bin: &str,
    timeout: Duration,
) -> HcResult<AutoStartOutcome> {
    // 1. Probe connectivity first (no process spawned if already up)
    if connect_unix(socket_path).await.is_ok() {
        return Ok(AutoStartOutcome::AlreadyRunning);
    }

    // 2. Custom sockets never trigger auto-start
    if is_custom_socket {
        return Ok(AutoStartOutcome::SkippedCustomSocket);
    }

    // 3. Spawn `<hc_bin> server start` detached with null stdio
    let mut child = match std::process::Command::new(hc_bin)
        .args(["server", "start"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(HcError::AutoStartFailed {
                kind: HcErrorKind::HcMissing,
                message: format!("'{hc_bin}' binary not found in PATH: {e}"),
            });
        }
        Err(e) => {
            return Err(HcError::AutoStartFailed {
                kind: HcErrorKind::SpawnFailed,
                message: format!("failed to spawn '{hc_bin} server start': {e}"),
            });
        }
    };

    // 4. Poll every 50ms up to timeout
    let poll_interval = Duration::from_millis(50);
    let start = std::time::Instant::now();

    loop {
        // Fast-fail: check if child process exited non-zero
        match child.try_wait() {
            Ok(Some(status)) if !status.success() => {
                if connect_unix(socket_path).await.is_ok() {
                    return Ok(AutoStartOutcome::Started);
                }
                return Err(HcError::AutoStartFailed {
                    kind: HcErrorKind::SpawnFailed,
                    message: format!(
                        "'{hc_bin} server start' exited with unsuccessful status: {status}"
                    ),
                });
            }
            Ok(_) => {}
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(HcError::AutoStartFailed {
                    kind: HcErrorKind::SpawnFailed,
                    message: format!("failed to wait on child process: {e}"),
                });
            }
        }

        if connect_unix(socket_path).await.is_ok() {
            return Ok(AutoStartOutcome::Started);
        }

        if start.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            return Err(HcError::AutoStartFailed {
                kind: HcErrorKind::SpawnFailed,
                message: format!(
                    "timed out after {timeout:?} waiting for '{hc_bin}' to start on {}",
                    socket_path.display()
                ),
            });
        }

        tokio::time::sleep(poll_interval).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn socket_precedence_order() {
        let cli = Path::new("/tmp/cli.sock");
        let cfg = Path::new("/tmp/cfg.sock");

        assert_eq!(
            resolve_socket_path(Some(cli), Some("/tmp/env.sock"), Some(cfg)),
            PathBuf::from("/tmp/cli.sock")
        );
        assert_eq!(
            resolve_socket_path(None, Some("/tmp/env.sock"), Some(cfg)),
            PathBuf::from("/tmp/env.sock")
        );
        assert_eq!(
            resolve_socket_path(None, Some(""), Some(cfg)),
            PathBuf::from("/tmp/cfg.sock")
        );
        assert_eq!(
            resolve_socket_path(None, None, None),
            default_core_socket_path()
        );
    }

    #[test]
    fn custom_socket_detection() {
        let p = Path::new("/custom/socket.sock");
        assert!(is_custom_socket(Some(p), None, None));
        assert!(is_custom_socket(None, Some("/custom/env.sock"), None));
        assert!(is_custom_socket(None, None, Some(p)));
        assert!(is_custom_socket(Some(p), Some("/custom/env.sock"), Some(p)));

        assert!(!is_custom_socket(None, None, None));
        assert!(!is_custom_socket(None, Some(""), None));
        assert!(!is_custom_socket(None, Some("   "), None));
    }

    #[test]
    fn auto_start_error_kinds() {
        use crate::hc::error::HcErrorKind;

        let missing = HcError::AutoStartFailed {
            kind: HcErrorKind::HcMissing,
            message: "binary not found".to_string(),
        };
        assert_eq!(missing.kind(), HcErrorKind::HcMissing);

        let timeout = HcError::AutoStartFailed {
            kind: HcErrorKind::SpawnFailed,
            message: "timed out".to_string(),
        };
        assert_eq!(timeout.kind(), HcErrorKind::SpawnFailed);
    }
}
