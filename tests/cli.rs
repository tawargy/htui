#[path = "support/fake_hc.rs"]
mod fake_hc;

use std::collections::HashMap;
use std::io::Write;
use std::path::Path;

use fake_hc::FakeHcServer;
use herdr_tui::cli::events::run_events;
use herdr_tui::cli::ls::run_ls;
use herdr_tui::cli::open::validate_space_exists;
use herdr_tui::cli::status::run_status;
use herdr_tui::hc::rpc::RpcClient;
use herdr_tui::hc::types::RunParams;

#[tokio::test]
async fn ls_subcommand() {
    let srv = FakeHcServer::start().await.expect("start fake server");
    let rpc = RpcClient::connect(srv.socket_path()).await.expect("rpc");

    let mut labels = HashMap::new();
    labels.insert("space".to_string(), "api".to_string());
    rpc.terminal_run(RunParams {
        argv: vec!["claude".to_string()],
        labels,
        ..Default::default()
    })
    .await
    .expect("run");

    // CL-1: JSON output
    let mut out = Vec::new();
    let mut err = Vec::new();
    let code = run_ls(
        srv.socket_path(),
        true,
        false,
        None,
        &[],
        &mut out,
        &mut err,
    )
    .await;
    assert_eq!(code, 0);
    let parsed: serde_json::Value = serde_json::from_slice(&out).expect("valid json");
    assert_eq!(parsed["terminals"].as_array().unwrap().len(), 1);

    // Table output
    let mut out2 = Vec::new();
    let mut err2 = Vec::new();
    let code2 = run_ls(
        srv.socket_path(),
        false,
        false,
        None,
        &[],
        &mut out2,
        &mut err2,
    )
    .await;
    assert_eq!(code2, 0);
    let table = String::from_utf8(out2).unwrap();
    assert!(table.contains("MACHINE"));
    assert!(table.contains("api"));
    assert!(table.contains("t1"));

    // CL-5: Malformed --label exits 2
    let mut out3 = Vec::new();
    let mut err3 = Vec::new();
    let bad = vec!["not_kv".to_string()];
    let code3 = run_ls(
        srv.socket_path(),
        false,
        false,
        None,
        &bad,
        &mut out3,
        &mut err3,
    )
    .await;
    assert_eq!(code3, 2);

    // CL-4: Unreachable socket exits 3
    let mut out4 = Vec::new();
    let mut err4 = Vec::new();
    let code4 = run_ls(
        Path::new("/tmp/missing_htui_cli_test.sock"),
        false,
        false,
        None,
        &[],
        &mut out4,
        &mut err4,
    )
    .await;
    assert_eq!(code4, 3);
}

struct BrokenPipeWriter;
impl Write for BrokenPipeWriter {
    fn write(&mut self, _buf: &[u8]) -> std::io::Result<usize> {
        Err(std::io::Error::new(std::io::ErrorKind::BrokenPipe, "pipe"))
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[tokio::test]
async fn status_and_events() {
    let srv = FakeHcServer::start().await.expect("start fake server");

    let mut out = Vec::new();
    let mut err = Vec::new();
    let code = run_status(srv.socket_path(), false, &mut out, &mut err).await;
    assert_eq!(code, 0);
    let s = String::from_utf8(out).unwrap();
    assert!(s.contains("core: local"));
    assert!(s.contains("v0.1.21"));

    // CL-2: BrokenPipe on events --json returns exit code 0 cleanly
    let mut bp = BrokenPipeWriter;
    let mut err2 = Vec::new();
    let code2 = run_events(srv.socket_path(), true, &mut bp, &mut err2).await;
    assert_eq!(code2, 0);

    // CL-4: Unreachable socket returns 3
    let mut out3 = Vec::new();
    let mut err3 = Vec::new();
    let code3 = run_status(
        Path::new("/tmp/missing_htui_status_test.sock"),
        false,
        &mut out3,
        &mut err3,
    )
    .await;
    assert_eq!(code3, 3);
}

#[tokio::test]
async fn open_subcommand() {
    let srv = FakeHcServer::start().await.expect("start fake server");
    // CL-3: missing space returns exit code 4
    let res = validate_space_exists(srv.socket_path(), "nosuch", false).await;
    assert_eq!(res, Err(4));

    // --create succeeds
    let res_create = validate_space_exists(srv.socket_path(), "nosuch", true).await;
    assert_eq!(res_create, Ok(()));
}

#[tokio::test]
async fn open_subcommand_unreachable_socket_exits_3() {
    // Verifies CL-4 for open subcommand when socket is unreachable
    let tmp = tempfile::tempdir().expect("tempdir");
    let missing_socket = tmp.path().join("missing_open_test.sock");
    let res = validate_space_exists(&missing_socket, "any_space", false).await;
    assert_eq!(res, Err(3));
}

#[cfg(unix)]
#[test]
fn open_subcommand_triggers_auto_start_on_default_socket_before_validate() {
    use std::os::unix::fs::PermissionsExt;
    use std::process::Command;

    let tmp = tempfile::tempdir().expect("tempdir");
    let marker = tmp.path().join("hc_invoked.txt");
    let script = tmp.path().join("mock_hc.sh");
    let script_body = format!(
        "#!/bin/sh\necho \"$@\" > \"{}\"\nexit 2\n",
        marker.display()
    );
    std::fs::write(&script, script_body).expect("write script");
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).expect("chmod");

    let output = Command::new(env!("CARGO_BIN_EXE_htui"))
        .args(["open", "any_space"])
        .env("XDG_RUNTIME_DIR", tmp.path())
        .env("XDG_CONFIG_HOME", tmp.path())
        .env("HOME", tmp.path())
        .env("HC_SESSION", "test_autostart")
        .env_remove("HTUI_HC_SOCKET")
        .env("HTUI_HC_BIN", &script)
        .output()
        .expect("run htui");

    assert_eq!(output.status.code(), Some(3));
    assert!(
        marker.exists(),
        "expected auto-start to invoke HTUI_HC_BIN before validate_space_exists"
    );
    let recorded = std::fs::read_to_string(&marker).expect("read marker");
    assert_eq!(recorded.trim(), "server start");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("warning:"),
        "expected warning on stderr, got: {stderr}"
    );
}

#[cfg(unix)]
#[test]
fn open_subcommand_skips_auto_start_with_custom_socket_flag() {
    use std::os::unix::fs::PermissionsExt;
    use std::process::Command;

    let tmp = tempfile::tempdir().expect("tempdir");
    let marker = tmp.path().join("hc_invoked.txt");
    let script = tmp.path().join("mock_hc.sh");
    let script_body = format!(
        "#!/bin/sh\necho \"$@\" > \"{}\"\nexit 2\n",
        marker.display()
    );
    std::fs::write(&script, script_body).expect("write script");
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).expect("chmod");

    let custom_sock = tmp.path().join("custom_missing.sock");
    let output = Command::new(env!("CARGO_BIN_EXE_htui"))
        .args([
            "--socket",
            custom_sock.to_str().unwrap(),
            "open",
            "any_space",
        ])
        .env("XDG_RUNTIME_DIR", tmp.path())
        .env("XDG_CONFIG_HOME", tmp.path())
        .env("HOME", tmp.path())
        .env_remove("HTUI_HC_SOCKET")
        .env("HTUI_HC_BIN", &script)
        .output()
        .expect("run htui");

    assert_eq!(output.status.code(), Some(3));
    assert!(
        !marker.exists(),
        "expected custom --socket to skip auto-start"
    );
}

#[cfg(unix)]
#[test]
fn cli_subcommands_do_not_auto_start_and_exit_3_on_unreachable_socket() {
    use std::os::unix::fs::PermissionsExt;
    use std::process::Command;

    let tmp = tempfile::tempdir().expect("tempdir");
    let marker = tmp.path().join("hc_invoked.txt");
    let script = tmp.path().join("mock_hc.sh");
    let script_body = format!(
        "#!/bin/sh\necho \"$@\" > \"{}\"\nexit 2\n",
        marker.display()
    );
    std::fs::write(&script, script_body).expect("write script");
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).expect("chmod");

    for subcmd in ["ls", "status", "events"] {
        let output = Command::new(env!("CARGO_BIN_EXE_htui"))
            .arg(subcmd)
            .env("XDG_RUNTIME_DIR", tmp.path())
            .env("XDG_CONFIG_HOME", tmp.path())
            .env("HOME", tmp.path())
            .env("HC_SESSION", "test_cli_no_autostart")
            .env_remove("HTUI_HC_SOCKET")
            .env("HTUI_HC_BIN", &script)
            .output()
            .expect("run htui");

        assert_eq!(
            output.status.code(),
            Some(3),
            "subcommand {subcmd} should exit 3 on unreachable socket"
        );
        assert!(
            !marker.exists(),
            "subcommand {subcmd} must not trigger auto-start"
        );
    }
}
