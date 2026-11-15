#[path = "support/fake_hc.rs"]
mod fake_hc;

use fake_hc::FakeHcServer;
use herdr_tui::hc::error::HcErrorKind;
use herdr_tui::hc::events::{EventMessage, EventStream};
use herdr_tui::hc::framed::{FrameKind, FramedClient};
use herdr_tui::hc::rpc::RpcClient;
use herdr_tui::hc::transport::{ensure_server_running_with, AutoStartOutcome};
use herdr_tui::hc::types::RunParams;
use serde_json::json;
use std::path::Path;
use std::time::Duration;

#[tokio::test]
async fn rpc_correlation() {
    let srv = FakeHcServer::start().await.expect("start fake server");
    let client = RpcClient::connect(srv.socket_path())
        .await
        .expect("connect client");

    assert_eq!(client.hello().version, "0.1.21");

    // Fire 10 concurrent requests and verify all correlate cleanly
    let mut handles = Vec::new();
    for i in 0..10 {
        let fut = client.call(
            "terminal.run",
            json!({ "argv": ["sh", "-c", format!("echo {i}")] }),
        );
        handles.push((i, fut));
    }
    let results = futures::future::join_all(handles.into_iter().map(|(i, f)| async move {
        let res = f.await.expect("rpc call");
        (i, res)
    }))
    .await;

    for (i, res) in results {
        let argv = res["argv"].as_array().expect("argv array");
        assert_eq!(argv[2].as_str().unwrap(), format!("echo {i}"));
    }
}

#[tokio::test]
async fn missing_mandatory_methods_returns_error() {
    let srv = FakeHcServer::start().await.expect("start fake server");
    {
        let mut st = srv.state.lock().await;
        st.methods = vec!["hello".to_string(), "server.status".to_string()];
    }
    let err = match RpcClient::connect(srv.socket_path()).await {
        Ok(_) => panic!("expected missing methods error"),
        Err(e) => e,
    };
    assert_eq!(err.kind(), HcErrorKind::MissingMandatoryMethods);
}

#[tokio::test]
async fn unreachable_socket_returns_socket_unreachable() {
    let err = match RpcClient::connect(Path::new("/tmp/definitely_missing_htui_hc_test.sock")).await
    {
        Ok(_) => panic!("expected socket unreachable"),
        Err(e) => e,
    };
    assert_eq!(err.kind(), HcErrorKind::SocketUnreachable);
}

#[tokio::test]
async fn framed_multiplexer() {
    let srv = FakeHcServer::start().await.expect("start fake server");
    let client = FramedClient::connect(srv.socket_path())
        .await
        .expect("connect framed");

    // Open 8 concurrent attach channels (HC-7)
    let mut handles = Vec::new();
    for i in 0..8 {
        let term_name = format!("t{}", i + 1);
        let (res, handle) = client
            .attach(&term_name, 80, 24, false, true)
            .await
            .expect("attach");
        assert!(res.channel >= 1);
        handles.push((i, handle));
    }

    // Send distinct data on each channel and verify independent echo
    for (i, handle) in &mut handles {
        let msg = format!("msg-{i}");
        handle.send_data(msg.as_bytes()).expect("send data");
        let frame = handle.rx.recv().await.expect("recv echo");
        assert_eq!(frame.channel, handle.channel);
        assert_eq!(frame.kind, FrameKind::Data);
        assert_eq!(frame.payload, msg.as_bytes());
    }

    // Close channel 0 and verify channel 1 still works
    let (_, h0) = handles.remove(0);
    h0.close().await.expect("close ch 0");

    let (_, h1) = &mut handles[0];
    h1.send_data(b"still-alive").expect("send on ch 1");
    let frame = h1.rx.recv().await.expect("recv on ch 1");
    assert_eq!(frame.payload, b"still-alive");
}

#[tokio::test]
async fn event_stream() {
    let srv = FakeHcServer::start().await.expect("start fake server");
    let rpc = RpcClient::connect(srv.socket_path()).await.expect("rpc");

    let term = rpc
        .terminal_run(RunParams {
            argv: vec!["sh".to_string()],
            ..Default::default()
        })
        .await
        .expect("run");
    assert_eq!(term.address, "t1");

    let (snap, mut events) =
        EventStream::subscribe_with_timeout(srv.socket_path(), Duration::from_millis(250))
            .await
            .expect("subscribe");
    assert_eq!(snap.terminals.len(), 1);
    assert_eq!(snap.seq, 1);

    // Push missed snapshot with seq 10, then stale event with seq 5, then valid event with seq 11
    srv.push_event(json!({
        "jsonrpc": "2.0",
        "method": "missed",
        "params": { "seq": 10, "terminals": [] }
    }))
    .await;
    srv.push_event(json!({
        "jsonrpc": "2.0",
        "method": "event",
        "params": { "seq": 5, "terminal": "t1", "type": "title", "data": { "title": "stale" } }
    }))
    .await;
    srv.push_event(json!({
        "jsonrpc": "2.0",
        "method": "event",
        "params": { "seq": 11, "terminal": "t1", "type": "title", "data": { "title": "fresh" } }
    }))
    .await;

    match events.next_event().await.expect("missed") {
        Some(EventMessage::Missed(m)) => assert_eq!(m.seq, 10),
        other => panic!("expected Missed, got {other:?}"),
    }
    // Stale seq=5 must be dropped, so next must be seq=11
    match events.next_event().await.expect("fresh") {
        Some(EventMessage::Event(e)) => {
            assert_eq!(e.seq, 11);
            assert_eq!(e.data["title"], "fresh");
        }
        other => panic!("expected Event(11), got {other:?}"),
    }

    // Wait for heartbeat timeout (HC-6)
    let err = events.next_event().await.expect_err("expected timeout");
    assert_eq!(err.kind(), HcErrorKind::HeartbeatTimeout);
}

#[tokio::test]
async fn early_channel_frames_buffered_before_attach_registers() {
    use herdr_tui::hc::framed::{Frame, FrameKind};
    use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
    use tokio::net::UnixListener;

    let tmp = tempfile::tempdir().expect("tempdir");
    let sock_path = tmp.path().join("early.sock");
    let listener = UnixListener::bind(&sock_path).expect("bind");

    let srv_task = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept");
        let (read_half, mut write_half) = stream.into_split();
        let mut reader = BufReader::new(read_half);
        let mut line = String::new();
        reader.read_line(&mut line).await.expect("hello line");

        let hello_resp = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "result": {
                "protocol": 1,
                "generation": 1,
                "server_version": "0.1.21",
                "framed": true,
                "features": ["resize_notifications"]
            }
        });
        let mut out = serde_json::to_vec(&hello_resp).unwrap();
        out.push(b'\n');
        write_half.write_all(&out).await.unwrap();
        write_half.flush().await.unwrap();

        // Read terminal.attach frame on channel 0
        let mut hdr = [0u8; 9];
        reader.read_exact(&mut hdr).await.expect("hdr");
        let len = u32::from_le_bytes([hdr[5], hdr[6], hdr[7], hdr[8]]) as usize;
        let mut payload = vec![0u8; len];
        reader.read_exact(&mut payload).await.expect("payload");
        let req: serde_json::Value = serde_json::from_slice(&payload).unwrap();
        assert_eq!(req["method"], "terminal.attach");
        assert_eq!(req["params"]["history"], 1000);
        let id = req["id"].clone();

        // Send early DATA frame on channel 7 BEFORE sending the attach RPC response on channel 0
        let early = Frame::data(7, b"early-history-bytes".to_vec());
        write_half.write_all(&early.encode()).await.unwrap();

        let attach_resp = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "channel": 7,
                "terminal": {
                    "address": "t1",
                    "pid": 100,
                    "argv": ["sh"],
                    "cwd": "/tmp",
                    "cols": 80,
                    "rows": 24,
                    "state": "running",
                    "labels": {}
                },
                "resize_notifications": true,
                "automatic_input": true
            }
        });
        let resp_frame = Frame::data(0, serde_json::to_vec(&attach_resp).unwrap());
        write_half.write_all(&resp_frame.encode()).await.unwrap();
        write_half.flush().await.unwrap();

        // Keep connection open briefly
        tokio::time::sleep(Duration::from_millis(100)).await;
    });

    let framed = FramedClient::connect(&sock_path).await.expect("connect");
    let (res, mut handle) = framed
        .attach("t1", 80, 24, false, true)
        .await
        .expect("attach");
    assert_eq!(res.channel, 7);

    let frame = tokio::time::timeout(Duration::from_millis(200), handle.rx.recv())
        .await
        .expect("no timeout waiting for early frame")
        .expect("early frame present");
    assert_eq!(frame.channel, 7);
    assert_eq!(frame.kind, FrameKind::Data);
    assert_eq!(frame.payload, b"early-history-bytes");

    srv_task.await.expect("srv_task");
}

#[tokio::test]
async fn auto_start_already_running_does_not_spawn() {
    let srv = FakeHcServer::start().await.expect("fake server");
    let outcome = ensure_server_running_with(
        srv.socket_path(),
        false,
        "nonexistent_hc_binary",
        Duration::from_millis(100),
    )
    .await
    .expect("outcome");

    assert_eq!(outcome, AutoStartOutcome::AlreadyRunning);
}

#[tokio::test]
async fn auto_start_skipped_on_custom_socket() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let missing_socket = tmp.path().join("missing_custom_socket_test.sock");
    let outcome = ensure_server_running_with(
        &missing_socket,
        true,
        "nonexistent_hc_binary",
        Duration::from_millis(100),
    )
    .await
    .expect("outcome");

    assert_eq!(outcome, AutoStartOutcome::SkippedCustomSocket);
}

#[tokio::test]
async fn auto_start_fails_missing_binary() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let missing_socket = tmp.path().join("missing_auto_start_binary_test.sock");
    let err = ensure_server_running_with(
        &missing_socket,
        false,
        "definitely_missing_hc_bin_12345",
        Duration::from_millis(100),
    )
    .await
    .expect_err("should fail missing binary");

    assert_eq!(err.kind(), HcErrorKind::HcMissing);
}

#[cfg(unix)]
#[tokio::test]
async fn auto_start_fails_fast_on_nonzero_exit() {
    use std::os::unix::fs::PermissionsExt;

    let tmp = tempfile::tempdir().expect("tempdir");
    let script = tmp.path().join("fail_hc.sh");
    std::fs::write(&script, "#!/bin/sh\nexit 2\n").expect("write script");
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).expect("chmod");

    let sock = tmp.path().join("fail.sock");
    let err = ensure_server_running_with(
        &sock,
        false,
        script.to_str().unwrap(),
        Duration::from_millis(2000),
    )
    .await
    .expect_err("should fail fast");

    assert_eq!(err.kind(), HcErrorKind::SpawnFailed);
}

#[cfg(unix)]
#[tokio::test]
async fn auto_start_fails_on_timeout() {
    use std::os::unix::fs::PermissionsExt;

    let tmp = tempfile::tempdir().expect("tempdir");
    let script = tmp.path().join("sleep_hc.sh");
    std::fs::write(&script, "#!/bin/sh\nsleep 10\n").expect("write script");
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).expect("chmod");

    let sock = tmp.path().join("timeout.sock");
    let err = ensure_server_running_with(
        &sock,
        false,
        script.to_str().unwrap(),
        Duration::from_millis(150),
    )
    .await
    .expect_err("should timeout");

    assert_eq!(err.kind(), HcErrorKind::SpawnFailed);
}

#[cfg(unix)]
#[tokio::test]
async fn auto_start_succeeds_when_socket_binds() {
    use std::os::unix::fs::PermissionsExt;

    let tmp = tempfile::tempdir().expect("tempdir");
    let sock = tmp.path().join("bound.sock");
    let script = tmp.path().join("mock_hc.sh");

    // Mock script verifies `server start` args, sleeps 100ms, then binds socket using python3
    let py_cmd = format!(
        "#!/bin/sh\nif [ \"$1\" != \"server\" ] || [ \"$2\" != \"start\" ]; then exit 3; fi\nsleep 0.1\npython3 -c 'import socket, time; s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM); s.bind(\"{}\"); s.listen(1); time.sleep(0.5)'\n",
        sock.display()
    );
    std::fs::write(&script, py_cmd).expect("write script");
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).expect("chmod");

    let outcome = ensure_server_running_with(
        &sock,
        false,
        script.to_str().unwrap(),
        Duration::from_secs(2),
    )
    .await
    .expect("should succeed");

    assert_eq!(outcome, AutoStartOutcome::Started);
}
