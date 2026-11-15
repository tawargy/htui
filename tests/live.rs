#[path = "support/fake_hc.rs"]
mod fake_hc;

use std::collections::HashMap;

use fake_hc::FakeHcServer;
use herdr_tui::hc::framed::FramedClient;
use herdr_tui::hc::rpc::RpcClient;
use herdr_tui::hc::transport::default_core_socket_path;
use herdr_tui::hc::types::RunParams;
use herdr_tui::term::session::{AttachSession, SessionState};

#[tokio::test]
async fn reconnection_simulation() {
    let srv = FakeHcServer::start().await.expect("start fake server");
    let rpc = RpcClient::connect(srv.socket_path()).await.expect("rpc");
    let term = rpc
        .terminal_run(RunParams {
            argv: vec!["sh".to_string()],
            ..Default::default()
        })
        .await
        .expect("run");

    let framed = FramedClient::connect(srv.socket_path())
        .await
        .expect("framed");
    let mut sess = AttachSession::new(term.address.clone(), 80, 24);
    sess.attach_observe(&framed).await.expect("observe");
    assert_eq!(sess.state, SessionState::Observing);

    // Simulate reconnect with a fresh FramedClient and reattach
    let framed2 = FramedClient::connect(srv.socket_path())
        .await
        .expect("framed2");
    sess.attach_observe(&framed2).await.expect("reattach");
    assert_eq!(sess.state, SessionState::Observing);
}

#[tokio::test]
async fn live_hc_smoke() {
    if std::env::var("HTUI_LIVE_HC").as_deref() != Ok("1") {
        return;
    }

    let sock = default_core_socket_path();
    let rpc = RpcClient::connect(&sock).await.expect("connect live hc");
    assert_eq!(rpc.hello().generation, 1);

    let mut labels = HashMap::new();
    labels.insert("space".to_string(), "htui-smoke".to_string());
    let term = rpc
        .terminal_run(RunParams {
            argv: vec![
                "sh".to_string(),
                "-c".to_string(),
                "printf 'السَّلَامُ عَلَيْكُمْ\\n'; sleep 3".to_string(),
            ],
            labels,
            ..Default::default()
        })
        .await
        .expect("run live terminal");

    let framed = FramedClient::connect(&sock).await.expect("framed live");
    let mut sess = AttachSession::new(term.address.clone(), 80, 24);
    sess.attach_observe(&framed).await.expect("observe live");

    // Read initial paint
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    sess.drain_frames();

    // Take over
    sess.take_over(&framed).await.expect("take_over live");
    assert_eq!(sess.state, SessionState::Active);

    // Clean up temporary terminal
    let _ = rpc.terminal_kill(&term.address).await;
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    let _ = rpc.terminal_rm(&term.address).await;
}

#[tokio::test]
async fn live_hc_active_typing() {
    if std::env::var("HTUI_LIVE_HC").as_deref() != Ok("1") {
        return;
    }

    use herdr_tui::term::session::InputDisposition;

    let sock = default_core_socket_path();
    let rpc = RpcClient::connect(&sock).await.expect("connect live hc");

    let mut labels = HashMap::new();
    labels.insert("space".to_string(), "htui-smoke".to_string());
    let term = rpc
        .terminal_run(RunParams {
            argv: vec!["sh".to_string()],
            labels,
            ..Default::default()
        })
        .await
        .expect("run live sh");

    let framed = FramedClient::connect(&sock).await.expect("framed live");
    let mut sess = AttachSession::new(term.address.clone(), 80, 24);
    sess.attach_active(&framed).await.expect("attach_active");
    assert_eq!(sess.state, SessionState::Active);

    // Give shell a moment to start, then send command
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    sess.drain_frames();

    let disp = sess
        .send_input(b"echo HTUI_INPUT_OK\r")
        .expect("send_input");
    assert_eq!(disp, InputDisposition::Sent);

    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(2);
    let mut found = false;
    while tokio::time::Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if let Ok(Some(frame)) = tokio::time::timeout(
            remaining.min(std::time::Duration::from_millis(200)),
            sess.recv_next(),
        )
        .await
        {
            sess.handle_frame(frame);
            sess.drain_frames();
        }
        for row in 0..24 {
            if sess.screen.row_text(row).contains("HTUI_INPUT_OK") {
                found = true;
                break;
            }
        }
        if found {
            break;
        }
    }

    // Clean up temporary terminal
    let _ = rpc.terminal_kill(&term.address).await;
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    let _ = rpc.terminal_rm(&term.address).await;

    assert!(found, "expected HTUI_INPUT_OK on live terminal screen");
}
