#[path = "support/fake_hc.rs"]
mod fake_hc;

use std::time::{Duration, Instant};

use fake_hc::FakeHcServer;
use herdr_tui::hc::framed::FramedClient;
use herdr_tui::hc::rpc::RpcClient;
use herdr_tui::hc::types::RunParams;
use herdr_tui::term::screen::{ScreenModel, Vt100Screen};
use herdr_tui::term::session::{AttachSession, InputDisposition, SessionState};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

#[test]
fn vt100_screen_arabic() {
    let mut screen = Vt100Screen::new(40, 10, 100);
    // Arabic with harakat: Fatha, Damma, Kasra, Shadda, Sukun, Tanwin
    let arabic = "بِسْمِ اللَّهِ الرَّحْمَٰنِ الرَّحِيمِ — شُكْرًا";
    screen.process(arabic.as_bytes());
    screen.process(b"\r\n");

    let row0 = screen.row_text(0);
    assert!(
        row0.contains("بِسْمِ"),
        "expected Arabic with harakat intact, got: {row0:?}"
    );
    assert!(
        row0.contains("شُكْرًا"),
        "expected Tanwin intact, got: {row0:?}"
    );

    // Render into Ratatui Buffer and ensure no panics or byte corruption
    let area = Rect::new(0, 0, 40, 10);
    let mut buf = Buffer::empty(area);
    screen.render_to_buffer(area, &mut buf);

    let first_sym = buf.cell((0, 0)).unwrap().symbol();
    assert_eq!(first_sym, "بِ");

    // Test resize before replacement paint (TV-4)
    screen.resize(20, 5);
    assert_eq!(screen.size(), (20, 5));
    screen.process(b"\x1b[2J\x1b[HResized OK");
    assert_eq!(screen.row_text(0), "Resized OK");
}

#[tokio::test]
async fn session_state_machine() {
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

    let mut session = AttachSession::new(term.address.clone(), 80, 24);
    session.attach_observe(&framed).await.expect("observe");
    assert_eq!(session.state, SessionState::Observing);

    // TV-2: input suppressed in Observing, takeover hint set
    let disp = session.send_input(b"ls\r").expect("send_input");
    assert_eq!(disp, InputDisposition::SuppressedObserving);
    assert!(session.takeover_hint);

    // TV-3: take over promotes to Active and forwards input
    session.take_over(&framed).await.expect("take_over");
    assert_eq!(session.state, SessionState::Active);
    assert!(!session.takeover_hint);
    let disp2 = session.send_input(b"hi").expect("send_input active");
    assert_eq!(disp2, InputDisposition::Sent);

    // Verify echo comes back on channel
    let frame = session.recv_next().await.expect("recv echo");
    session.handle_frame(frame);
    assert_eq!(session.screen.row_text(0), "hi");

    // TV-8: resize debounced to <= 1 per 50ms
    let t0 = Instant::now();
    assert!(session.request_resize(90, 30, t0).expect("r1"));
    assert!(!session
        .request_resize(95, 32, t0 + Duration::from_millis(10))
        .expect("r2 debounced"));
    assert!(session
        .flush_pending_resize(t0 + Duration::from_millis(60))
        .expect("r2 flushed"));
}

#[tokio::test]
async fn switch_back_reuses_existing_active_session() {
    use herdr_tui::config::Config;
    use herdr_tui::hc::framed::Frame;
    use herdr_tui::layout::store::LayoutStore;
    use herdr_tui::ui::app::{AppAction, TuiApp};
    use herdr_tui::ui::dialogs::SpawnTarget;
    use herdr_tui::ui::event_loop::dispatch_action;

    let srv = FakeHcServer::start().await.expect("start fake server");
    let rpc = RpcClient::connect(srv.socket_path()).await.expect("rpc");
    let framed = FramedClient::connect(srv.socket_path())
        .await
        .expect("framed");
    let tmp = tempfile::tempdir().expect("tempdir");
    let store = LayoutStore::new(tmp.path().join("layouts.toml"), &[]);
    let mut app = TuiApp::new(Config::default(), None);

    dispatch_action(
        AppAction::Spawn {
            command_or_template: "sh".to_string(),
            task: String::new(),
            space: "api".to_string(),
            target: SpawnTarget::NewPanel,
        },
        &mut app,
        Some(&rpc),
        Some(&framed),
        &store,
    )
    .await;

    let addr = app
        .model
        .terminals
        .keys()
        .next()
        .expect("spawned terminal")
        .clone();
    let sess = app.sessions.get_mut(&addr).expect("session created");
    assert_eq!(sess.state, SessionState::Active);
    sess.handle_frame(Frame::data(1, b"hello-screen".to_vec()));
    assert_eq!(sess.screen.row_text(0), "hello-screen");

    // Simulate switching back to the same terminal via AttachTerminal
    let pid = app.focused_pane.expect("focused pane");
    dispatch_action(
        AppAction::AttachTerminal {
            pane_id: pid,
            terminal: addr.clone(),
        },
        &mut app,
        Some(&rpc),
        Some(&framed),
        &store,
    )
    .await;

    let reused = app.sessions.get(&addr).expect("session still exists");
    assert_eq!(reused.state, SessionState::Active);
    assert_eq!(reused.screen.row_text(0), "hello-screen");
}

#[tokio::test]
async fn attach_terminal_defaults_to_active_and_respects_observe_mode() {
    use herdr_tui::config::{AttachMode, Config};
    use herdr_tui::layout::store::LayoutStore;
    use herdr_tui::ui::app::{AppAction, TuiApp};
    use herdr_tui::ui::event_loop::dispatch_action;

    let srv = FakeHcServer::start().await.expect("start fake server");
    let rpc = RpcClient::connect(srv.socket_path()).await.expect("rpc");
    let framed = FramedClient::connect(srv.socket_path())
        .await
        .expect("framed");
    let term = rpc
        .terminal_run(RunParams {
            argv: vec!["sh".to_string()],
            ..Default::default()
        })
        .await
        .expect("run");

    let tmp = tempfile::tempdir().expect("tempdir");
    let store = LayoutStore::new(tmp.path().join("layouts.toml"), &[]);

    // Default (Auto) attaches in Active mode and focuses pane
    let mut app = TuiApp::new(Config::default(), None);
    app.sidebar_focused = true;
    let pid = app.focused_pane.expect("focused pane");
    dispatch_action(
        AppAction::AttachTerminal {
            pane_id: pid,
            terminal: term.address.clone(),
        },
        &mut app,
        Some(&rpc),
        Some(&framed),
        &store,
    )
    .await;
    assert!(!app.sidebar_focused);
    assert_eq!(
        app.sessions.get(&term.address).expect("sess").state,
        SessionState::Active
    );

    // Explicit Observe mode attaches in Observing mode
    let mut obs_cfg = Config::default();
    obs_cfg.attach.mode = AttachMode::Observe;
    let mut obs_app = TuiApp::new(obs_cfg, None);
    let obs_pid = obs_app.focused_pane.expect("focused pane");
    dispatch_action(
        AppAction::AttachTerminal {
            pane_id: obs_pid,
            terminal: term.address.clone(),
        },
        &mut obs_app,
        Some(&rpc),
        Some(&framed),
        &store,
    )
    .await;
    assert_eq!(
        obs_app.sessions.get(&term.address).expect("sess").state,
        SessionState::Observing
    );
}

#[tokio::test]
async fn sync_pane_sizes_resizes_session_to_wide_area_and_sends_resize() {
    use fake_hc::FRAME_RESIZE;
    use herdr_tui::config::Config;
    use herdr_tui::layout::tree::TermRef;
    use herdr_tui::ui::app::TuiApp;

    let srv = FakeHcServer::start().await.expect("start fake server");
    let rpc = RpcClient::connect(srv.socket_path()).await.expect("rpc");
    let framed = FramedClient::connect(srv.socket_path())
        .await
        .expect("framed");
    let term = rpc
        .terminal_run(RunParams {
            argv: vec!["sh".to_string()],
            ..Default::default()
        })
        .await
        .expect("run");

    let mut app = TuiApp::new(Config::default(), Some("api".into()));
    let pid = app.focused_pane.expect("focused pane");
    app.current_tree_mut().set_terminal(
        pid,
        Some(TermRef {
            address: term.address.clone(),
        }),
    );

    let mut sess = AttachSession::new(term.address.clone(), 80, 24);
    sess.attach_active(&framed).await.expect("attach_active");
    app.sessions.insert(term.address.clone(), sess);

    let now = Instant::now();
    let area = Rect::new(0, 0, 140, 40);
    let mut buf = Buffer::empty(area);
    app.render(area, &mut buf, now);
    app.sync_pane_sizes(now);

    let expected_size = app.pane_inner_size(pid);
    assert!(
        expected_size.0 > 80,
        "expected wide inner cols > 80, got {:?}",
        expected_size
    );
    assert_eq!(
        app.sessions.get(&term.address).unwrap().screen.size(),
        expected_size
    );

    tokio::time::sleep(Duration::from_millis(50)).await;
    let frames = srv.recorded_frames().await;
    let has_resize = frames.iter().any(|f| {
        if f.kind != FRAME_RESIZE || f.payload.len() < 4 {
            return false;
        }
        let cols = u16::from_le_bytes([f.payload[0], f.payload[1]]);
        let rows = u16::from_le_bytes([f.payload[2], f.payload[3]]);
        (cols, rows) == expected_size
    });
    assert!(
        has_resize,
        "expected FRAME_RESIZE with {:?}, got {:?}",
        expected_size, frames
    );
}

#[test]
fn vt100_screen_exposes_mouse_protocol_modes_and_encodings() {
    use herdr_tui::term::screen::{MouseProtocolEncoding, MouseProtocolMode, ScreenModes};

    let default_modes = ScreenModes::default();
    assert_eq!(default_modes.mouse_mode, MouseProtocolMode::None);
    assert_eq!(default_modes.mouse_encoding, MouseProtocolEncoding::Default);

    let mut screen = Vt100Screen::new(80, 24, 100);
    assert_eq!(screen.modes().mouse_mode, MouseProtocolMode::None);
    assert_eq!(
        screen.modes().mouse_encoding,
        MouseProtocolEncoding::Default
    );

    // DECSET ?9h -> Press, DECRST ?9l -> None
    screen.process(b"\x1b[?9h");
    assert_eq!(screen.modes().mouse_mode, MouseProtocolMode::Press);
    screen.process(b"\x1b[?9l");
    assert_eq!(screen.modes().mouse_mode, MouseProtocolMode::None);

    // DECSET ?1000h -> PressRelease, DECRST ?1000l -> None
    screen.process(b"\x1b[?1000h");
    assert_eq!(screen.modes().mouse_mode, MouseProtocolMode::PressRelease);
    screen.process(b"\x1b[?1000l");
    assert_eq!(screen.modes().mouse_mode, MouseProtocolMode::None);

    // DECSET ?1002h -> ButtonMotion, DECRST ?1002l -> None
    screen.process(b"\x1b[?1002h");
    assert_eq!(screen.modes().mouse_mode, MouseProtocolMode::ButtonMotion);
    screen.process(b"\x1b[?1002l");
    assert_eq!(screen.modes().mouse_mode, MouseProtocolMode::None);

    // DECSET ?1003h -> AnyMotion, DECRST ?1003l -> None
    screen.process(b"\x1b[?1003h");
    assert_eq!(screen.modes().mouse_mode, MouseProtocolMode::AnyMotion);
    screen.process(b"\x1b[?1003l");
    assert_eq!(screen.modes().mouse_mode, MouseProtocolMode::None);

    // DECSET ?1005h -> Utf8, DECRST ?1005l -> Default
    screen.process(b"\x1b[?1005h");
    assert_eq!(screen.modes().mouse_encoding, MouseProtocolEncoding::Utf8);
    screen.process(b"\x1b[?1005l");
    assert_eq!(
        screen.modes().mouse_encoding,
        MouseProtocolEncoding::Default
    );

    // DECSET ?1006h -> Sgr, DECRST ?1006l -> Default
    screen.process(b"\x1b[?1006h");
    assert_eq!(screen.modes().mouse_encoding, MouseProtocolEncoding::Sgr);
    screen.process(b"\x1b[?1006l");
    assert_eq!(
        screen.modes().mouse_encoding,
        MouseProtocolEncoding::Default
    );
}

#[tokio::test]
async fn dispatch_action_send_input_forwards_bytes_over_framed_channel() {
    use fake_hc::FRAME_DATA;
    use herdr_tui::config::Config;
    use herdr_tui::layout::store::LayoutStore;
    use herdr_tui::layout::tree::TermRef;
    use herdr_tui::ui::app::{AppAction, TuiApp};
    use herdr_tui::ui::event_loop::dispatch_action;

    let srv = FakeHcServer::start().await.expect("start fake server");
    let rpc = RpcClient::connect(srv.socket_path()).await.expect("rpc");
    let framed = FramedClient::connect(srv.socket_path())
        .await
        .expect("framed");
    let term = rpc
        .terminal_run(RunParams {
            argv: vec!["sh".to_string()],
            ..Default::default()
        })
        .await
        .expect("run");

    let tmp = tempfile::tempdir().expect("tempdir");
    let store = LayoutStore::new(tmp.path().join("layouts.toml"), &[]);
    let mut app = TuiApp::new(Config::default(), Some("api".into()));
    let pid = app.focused_pane.expect("focused pane");
    app.current_tree_mut().set_terminal(
        pid,
        Some(TermRef {
            address: term.address.clone(),
        }),
    );

    let mut sess = AttachSession::new(term.address.clone(), 80, 24);
    sess.attach_active(&framed).await.expect("attach_active");
    app.sessions.insert(term.address.clone(), sess);

    let payload = b"\x1b[<0;3;2M".to_vec();
    dispatch_action(
        AppAction::SendInput {
            terminal: term.address.clone(),
            bytes: payload.clone(),
        },
        &mut app,
        Some(&rpc),
        Some(&framed),
        &store,
    )
    .await;

    tokio::time::sleep(Duration::from_millis(50)).await;
    let frames = srv.recorded_frames().await;
    assert!(
        frames
            .iter()
            .any(|f| f.kind == FRAME_DATA && f.payload == payload),
        "expected FRAME_DATA with {payload:?}, got {frames:?}"
    );
}
