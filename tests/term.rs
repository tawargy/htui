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

#[test]
fn vt100_screen_boundary_wide_zero_width_and_clearing_invariants() {
    use unicode_width::UnicodeWidthStr;

    // 1. Wide character at rightmost column when area is narrower than screen,
    //    and orphaned wide character after vt100 shrink resize.
    let mut screen = Vt100Screen::new(10, 4, 100);
    // "abcd" (cols 0..4) + "漢" (cols 4..6) + "ef" (cols 6..8) + "字" (cols 8..10)
    screen.process("abcd漢ef字".as_bytes());

    // Render into a 5-col area: col 4 has "漢" (width 2, 4+2=6 > 5) -> must be replaced with " "
    let narrow_area = Rect::new(0, 0, 5, 2);
    let mut buf = Buffer::empty(narrow_area);
    screen.render_to_buffer(narrow_area, &mut buf);
    let col4 = buf.cell((4, 0)).unwrap().symbol();
    assert_eq!(col4, " ", "boundary wide char must be clamped to space");
    assert!(UnicodeWidthStr::width(col4) <= 1);

    // Shrink screen to 5 cols; vt100 Row::resize may leave wide char at col 4
    screen.resize(5, 4);
    let mut buf_shrunk = Buffer::empty(narrow_area);
    screen.render_to_buffer(narrow_area, &mut buf_shrunk);
    let shrunk_col4 = buf_shrunk.cell((4, 0)).unwrap().symbol();
    assert_eq!(
        shrunk_col4, " ",
        "shrunk boundary wide char must not bleed past area.width"
    );

    // 2. Continuation cell in buffer must be reset/empty, even if buffer had stale text
    let mut wide_screen = Vt100Screen::new(6, 2, 100);
    wide_screen.process("漢a".as_bytes());
    let wide_area = Rect::new(0, 0, 6, 2);
    let mut stale_buf = Buffer::empty(wide_area);
    for x in 0..6 {
        stale_buf.cell_mut((x, 0)).unwrap().set_symbol("X");
    }
    wide_screen.render_to_buffer(wide_area, &mut stale_buf);
    assert_eq!(stale_buf.cell((0, 0)).unwrap().symbol(), "漢");
    assert_eq!(
        stale_buf.cell((1, 0)).unwrap().symbol(),
        " ",
        "continuation cell must be reset instead of keeping stale 'X'"
    );
    assert_eq!(stale_buf.cell((2, 0)).unwrap().symbol(), "a");

    // 3. Area larger than screen_cols/screen_rows must reset stale cells outside screen bounds
    let big_area = Rect::new(0, 0, 10, 4);
    let mut big_buf = Buffer::empty(big_area);
    for y in 0..4 {
        for x in 0..10 {
            big_buf.cell_mut((x, y)).unwrap().set_symbol("Z");
        }
    }
    wide_screen.render_to_buffer(big_area, &mut big_buf);
    assert_eq!(big_buf.cell((7, 0)).unwrap().symbol(), " ");
    assert_eq!(big_buf.cell((0, 3)).unwrap().symbol(), " ");

    // 4. Zero-width graphemes (ZWSP, FEFF, lone combining mark) normalize to " ",
    //    while Arabic harakat clusters remain intact.
    let mut zw_screen = Vt100Screen::new(20, 3, 100);
    // Standalone ZWSP / BOM / Arabic harakat
    zw_screen.process("\u{200B}\u{FEFF}\r\nبِ لَّ شُ".as_bytes());
    let mut zw_buf = Buffer::empty(Rect::new(0, 0, 20, 3));
    zw_screen.render_to_buffer(Rect::new(0, 0, 20, 3), &mut zw_buf);
    for x in 0..20 {
        let sym = zw_buf.cell((x, 0)).unwrap().symbol();
        assert!(
            UnicodeWidthStr::width(sym) >= 1,
            "zero-width symbol {sym:?} leaked into buffer at col {x}"
        );
        assert!(
            !sym.chars().any(char::is_control),
            "control char {sym:?} leaked into buffer at col {x}"
        );
    }
    let row1 = zw_screen.row_text(1);
    assert!(row1.contains("بِ"));
    assert!(row1.contains("لَّ"));
    assert!(row1.contains("شُ"));
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

#[tokio::test]
async fn attach_session_resize_lifecycle_observing_debounce_and_take_over_at() {
    use fake_hc::{make_terminal, FRAME_RESIZE};

    let srv = FakeHcServer::start().await.expect("start fake server");
    let mut custom_term = make_terminal("t-obs", "sh");
    custom_term["cols"] = serde_json::json!(112);
    custom_term["rows"] = serde_json::json!(33);
    srv.add_terminal(custom_term).await;

    let framed = FramedClient::connect(srv.socket_path())
        .await
        .expect("framed");

    // 1. attach_observe initializes screen from res.terminal.cols/rows when non-zero
    let mut session = AttachSession::new("t-obs".to_string(), 80, 24);
    session.attach_observe(&framed).await.expect("observe");
    assert_eq!(session.state, SessionState::Observing);
    assert_eq!(session.screen.size(), (112, 33));

    // 2. request_resize in Observing must NOT resize screen or send FRAME_RESIZE
    let t0 = Instant::now();
    assert!(!session.request_resize(60, 20, t0).expect("obs resize"));
    assert_eq!(session.screen.size(), (112, 33));
    tokio::time::sleep(Duration::from_millis(20)).await;
    let obs_frames = srv.recorded_frames().await;
    assert!(
        !obs_frames.iter().any(|f| f.kind == FRAME_RESIZE),
        "Observing session must not send FRAME_RESIZE: {obs_frames:?}"
    );

    // 3. take_over_at promotes to Active at the target pane size and syncs screen
    session
        .take_over_at(&framed, 60, 20)
        .await
        .expect("take_over_at");
    assert_eq!(session.state, SessionState::Active);
    assert_eq!(session.screen.size(), (60, 20));

    // 4. Duplicate request_resize at current screen size is a no-op
    assert!(!session.request_resize(60, 20, t0).expect("dup resize"));
    assert_eq!(session.screen.size(), (60, 20));

    // 5. Un-debounced request_resize sends RESIZE and resizes screen
    assert!(session.request_resize(70, 22, t0).expect("r1"));
    assert_eq!(session.screen.size(), (70, 22));

    // 6. Debounced request_resize keeps screen at 70x22 until flushed
    assert!(!session
        .request_resize(75, 25, t0 + Duration::from_millis(10))
        .expect("r2 debounced"));
    assert_eq!(
        session.screen.size(),
        (70, 22),
        "screen must stay at old PTY size while RESIZE is debounced"
    );

    // 7. Reverting back to current size clears pending_resize
    assert!(!session
        .request_resize(70, 22, t0 + Duration::from_millis(15))
        .expect("revert clears pending"));
    assert!(!session
        .flush_pending_resize(t0 + Duration::from_millis(100))
        .expect("nothing pending"));

    // 8. Debounced resize followed by flush_pending_resize_now sends immediately (< 50ms)
    assert!(!session
        .request_resize(82, 26, t0 + Duration::from_millis(20))
        .expect("r3 debounced"));
    assert_eq!(session.screen.size(), (70, 22));
    assert!(session
        .flush_pending_resize_now(t0 + Duration::from_millis(25))
        .expect("flush now"));
    assert_eq!(session.screen.size(), (82, 26));

    tokio::time::sleep(Duration::from_millis(30)).await;
    let resize_frames: Vec<(u16, u16)> = srv
        .recorded_frames()
        .await
        .into_iter()
        .filter(|f| f.kind == FRAME_RESIZE && f.payload.len() >= 4)
        .map(|f| {
            (
                u16::from_le_bytes([f.payload[0], f.payload[1]]),
                u16::from_le_bytes([f.payload[2], f.payload[3]]),
            )
        })
        .collect();
    assert_eq!(resize_frames, vec![(70, 22), (82, 26)]);
}

#[tokio::test]
async fn app_and_event_loop_sync_end_drag_split_spawn_take_over_and_size_event() {
    use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
    use fake_hc::FRAME_RESIZE;
    use herdr_tui::config::Config;
    use herdr_tui::hc::types::EventPayload;
    use herdr_tui::layout::panel::PanelId;
    use herdr_tui::layout::store::LayoutStore;
    use herdr_tui::layout::tree::{SplitAxis, TermRef};
    use herdr_tui::ui::app::{AppAction, TuiApp};
    use herdr_tui::ui::dialogs::SpawnTarget;
    use herdr_tui::ui::event_loop::{dispatch_action, handle_core_event_with_framed};

    let srv = FakeHcServer::start().await.expect("start fake server");
    let rpc = RpcClient::connect(srv.socket_path()).await.expect("rpc");
    let framed = FramedClient::connect(srv.socket_path())
        .await
        .expect("framed");
    let tmp = tempfile::tempdir().expect("tempdir");
    let store = LayoutStore::new(tmp.path().join("layouts.toml"), &[]);

    let mut api_labels = std::collections::HashMap::new();
    api_labels.insert("space".to_string(), "api".to_string());
    let t1 = rpc
        .terminal_run(RunParams {
            argv: vec!["sh".to_string()],
            labels: api_labels.clone(),
            ..Default::default()
        })
        .await
        .expect("run t1");
    let t_obs = rpc
        .terminal_run(RunParams {
            argv: vec!["sh".to_string()],
            labels: api_labels,
            ..Default::default()
        })
        .await
        .expect("run t_obs");

    let mut app = TuiApp::new(Config::default(), Some("api".into()));
    let panel_id: PanelId = app.active_panel_id().expect("panel");
    let p1 = app.focused_pane.expect("p1");
    app.model.terminals.insert(t1.address.clone(), t1.clone());
    app.model
        .terminals
        .insert(t_obs.address.clone(), t_obs.clone());
    app.current_tree_mut().set_terminal(
        p1,
        Some(TermRef {
            address: t1.address.clone(),
        }),
    );

    let mut s1 = AttachSession::new(t1.address.clone(), 80, 24);
    s1.attach_active(&framed).await.expect("attach t1");
    app.sessions.insert(t1.address.clone(), s1);

    // Initial render at 120x30 (backdate t0 by 100ms so subsequent Spawn is not debounced)
    let t0 = Instant::now() - Duration::from_millis(100);
    let area = Rect::new(0, 0, 120, 30);
    let mut buf = Buffer::empty(area);
    app.render(area, &mut buf, t0);
    // render sets dirty = false; sync_pane_sizes should resize t1, return true, and set dirty = true
    let did_resize: bool = app.sync_pane_sizes(t0);
    assert!(did_resize);
    assert!(app.dirty);
    let p1_full_size = app.pane_inner_size(p1);
    assert_eq!(
        app.sessions.get(&t1.address).unwrap().screen.size(),
        p1_full_size
    );

    // 1. AppAction::Spawn with SplitPane immediately shrinks sibling pane p1 before rerender
    dispatch_action(
        AppAction::Spawn {
            command_or_template: "sh".to_string(),
            task: String::new(),
            space: "api".to_string(),
            target: SpawnTarget::SplitPane {
                panel_id,
                pane_id: p1,
                axis: SplitAxis::Horizontal,
            },
        },
        &mut app,
        Some(&rpc),
        Some(&framed),
        &store,
    )
    .await;
    let p1_split_size = app.pane_inner_size(p1);
    assert!(p1_split_size.0 < p1_full_size.0);
    assert_eq!(
        app.sessions.get(&t1.address).unwrap().screen.size(),
        p1_split_size,
        "sibling pane t1 must be resized immediately on SplitPane Spawn"
    );

    // 2. DragDivider (debounced) + EndDrag via mouse events flushes immediately and marks dirty
    app.render(area, &mut buf, t0);
    let divider = app.last_geom.dividers[0];
    let drag_t = Instant::now();
    app.handle_mouse_event(
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: divider.line_pos,
            row: divider.container.y + 2,
            modifiers: KeyModifiers::NONE,
        },
        drag_t,
    );
    app.handle_mouse_event(
        MouseEvent {
            kind: MouseEventKind::Drag(MouseButton::Left),
            column: divider.line_pos.saturating_sub(12),
            row: divider.container.y + 2,
            modifiers: KeyModifiers::NONE,
        },
        drag_t + Duration::from_millis(2),
    );
    assert_eq!(
        app.sessions.get(&t1.address).unwrap().screen.size(),
        p1_split_size,
        "debounced DragDivider must not prematurely resize screen before EndDrag"
    );
    app.dirty = false;
    let end_actions = app.handle_mouse_event(
        MouseEvent {
            kind: MouseEventKind::Up(MouseButton::Left),
            column: divider.line_pos.saturating_sub(12),
            row: divider.container.y + 2,
            modifiers: KeyModifiers::NONE,
        },
        drag_t + Duration::from_millis(5),
    );
    assert!(end_actions.contains(&AppAction::SaveLayout("api".to_string())));
    assert!(app.dirty, "EndDrag flush must mark app.dirty = true");
    let expected_drag_size = app.pane_inner_size(p1);
    assert_eq!(
        app.sessions.get(&t1.address).unwrap().screen.size(),
        expected_drag_size
    );

    tokio::time::sleep(Duration::from_millis(20)).await;
    let frames = srv.recorded_frames().await;
    assert!(frames.iter().any(|f| {
        f.kind == FRAME_RESIZE
            && f.payload.len() >= 4
            && (
                u16::from_le_bytes([f.payload[0], f.payload[1]]),
                u16::from_le_bytes([f.payload[2], f.payload[3]]),
            ) == expected_drag_size
    }));

    // 3. Observing pane is skipped by sync_pane_sizes, updated by "size" fallback when !resize_notifications,
    //    and TakeOver attaches at pane_inner_size!
    let p2 = app.focused_pane.expect("p2");
    app.current_tree_mut().set_terminal(
        p2,
        Some(TermRef {
            address: t_obs.address.clone(),
        }),
    );
    let mut s_obs = AttachSession::new(t_obs.address.clone(), 40, 10);
    s_obs.attach_observe(&framed).await.expect("observe t_obs");
    s_obs.resize_notifications = false;
    app.sessions.insert(t_obs.address.clone(), s_obs);

    let p2_size = app.pane_inner_size(p2);
    assert_ne!(
        app.sessions.get(&t_obs.address).unwrap().screen.size(),
        p2_size
    );
    app.dirty = false;
    assert!(
        !app.sync_pane_sizes(drag_t + Duration::from_millis(100)),
        "sync_pane_sizes must skip Observing sessions"
    );
    assert_ne!(
        app.sessions.get(&t_obs.address).unwrap().screen.size(),
        p2_size
    );

    // "size" event fallback updates Observing session when !resize_notifications
    handle_core_event_with_framed(
        EventPayload {
            seq: 10,
            terminal: Some(t_obs.address.clone()),
            event_type: "size".to_string(),
            data: serde_json::json!({ "cols": 55, "rows": 18 }),
        },
        &mut app,
        Some(&rpc),
        Some(&framed),
        &store,
    )
    .await;
    assert_eq!(
        app.sessions.get(&t_obs.address).unwrap().screen.size(),
        (55, 18)
    );
    assert!(app.dirty);

    // TakeOver adopts p2's pane_inner_size
    dispatch_action(
        AppAction::TakeOver(t_obs.address.clone()),
        &mut app,
        Some(&rpc),
        Some(&framed),
        &store,
    )
    .await;
    let s_taken = app.sessions.get(&t_obs.address).unwrap();
    assert_eq!(s_taken.state, SessionState::Active);
    assert_eq!(s_taken.screen.size(), p2_size);
}

#[tokio::test]
async fn observing_frame_resize_vs_size_event_and_control_char_sanitization() {
    use herdr_tui::config::Config;
    use herdr_tui::hc::framed::Frame;
    use herdr_tui::hc::types::EventPayload;
    use herdr_tui::layout::store::LayoutStore;
    use herdr_tui::ui::app::TuiApp;
    use herdr_tui::ui::event_loop::handle_core_event_with_framed;

    let srv = FakeHcServer::start().await.expect("start fake server");
    let rpc = RpcClient::connect(srv.socket_path()).await.expect("rpc");
    let framed = FramedClient::connect(srv.socket_path())
        .await
        .expect("framed");
    let tmp = tempfile::tempdir().expect("tempdir");
    let store = LayoutStore::new(tmp.path().join("layouts.toml"), &[]);

    let term = rpc
        .terminal_run(RunParams {
            argv: vec!["sh".to_string()],
            ..Default::default()
        })
        .await
        .expect("run");

    let mut app = TuiApp::new(Config::default(), None);
    let mut sess = AttachSession::new(term.address.clone(), 80, 24);
    sess.attach_observe(&framed).await.expect("observe");
    assert!(sess.resize_notifications);

    // FrameKind::Resize updates Observing screen when resize_notifications == true
    sess.handle_frame(Frame::resize(1, 96, 28));
    assert_eq!(sess.screen.size(), (96, 28));

    // Control characters inside screen bytes never leak into Vt100Screen::cell
    sess.handle_frame(Frame::data(1, b"ok\x00\x07\x1b[0mend".to_vec()));
    for col in 0..10 {
        let cell = sess.screen.cell(0, col).expect("cell");
        assert!(
            !cell.symbol.chars().any(char::is_control),
            "control char leaked at col {col}: {:?}",
            cell.symbol
        );
    }
    app.sessions.insert(term.address.clone(), sess);

    // Core "size" event is ignored when resize_notifications == true (FrameKind::Resize is authoritative)
    app.dirty = false;
    handle_core_event_with_framed(
        EventPayload {
            seq: 20,
            terminal: Some(term.address.clone()),
            event_type: "size".to_string(),
            data: serde_json::json!({ "cols": 50, "rows": 15 }),
        },
        &mut app,
        Some(&rpc),
        Some(&framed),
        &store,
    )
    .await;
    assert_eq!(
        app.sessions.get(&term.address).unwrap().screen.size(),
        (96, 28)
    );
}
