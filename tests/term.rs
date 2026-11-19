#[path = "support/fake_hc.rs"]
mod fake_hc;

use std::time::{Duration, Instant};

use fake_hc::FakeHcServer;
use herdr_tui::hc::framed::FramedClient;
use herdr_tui::hc::rpc::RpcClient;
use herdr_tui::hc::types::RunParams;
use herdr_tui::term::screen::{AlacrittyScreen, ScreenModel};
use herdr_tui::term::session::{AttachSession, InputDisposition, SessionState};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

#[test]
fn alacritty_screen_arabic() {
    let mut screen = AlacrittyScreen::new(40, 10, 100);
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
fn alacritty_screen_boundary_wide_zero_width_and_clearing_invariants() {
    use unicode_width::UnicodeWidthStr;

    // 1. Wide character at rightmost column when area is narrower than screen,
    //    and boundary behavior after shrink resize.
    let mut screen = AlacrittyScreen::new(10, 4, 100);
    // "abcd" (cols 0..4) + "漢" (cols 4..6) + "ef" (cols 6..8) + "字" (cols 8..10)
    screen.process("abcd漢ef字".as_bytes());

    // Render into a 5-col area: col 4 has "漢" (width 2, 4+2=6 > 5) -> must be replaced with " "
    let narrow_area = Rect::new(0, 0, 5, 2);
    let mut buf = Buffer::empty(narrow_area);
    screen.render_to_buffer(narrow_area, &mut buf);
    let col4 = buf.cell((4, 0)).unwrap().symbol();
    assert_eq!(col4, " ", "boundary wide char must be clamped to space");
    assert!(UnicodeWidthStr::width(col4) <= 1);

    // Shrink screen to 5 cols and re-verify
    screen.resize(5, 4);
    let mut buf_shrunk = Buffer::empty(narrow_area);
    screen.render_to_buffer(narrow_area, &mut buf_shrunk);
    let shrunk_col4 = buf_shrunk.cell((4, 0)).unwrap().symbol();
    assert_eq!(
        shrunk_col4, " ",
        "shrunk boundary wide char must not bleed past area.width"
    );

    // 2. Continuation cell in buffer must be reset/empty, even if buffer had stale text
    let mut wide_screen = AlacrittyScreen::new(6, 2, 100);
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
    let mut zw_screen = AlacrittyScreen::new(20, 3, 100);
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
fn alacritty_screen_exposes_mouse_protocol_modes_and_encodings() {
    use herdr_tui::term::screen::{MouseProtocolEncoding, MouseProtocolMode, ScreenModes};

    let default_modes = ScreenModes::default();
    assert_eq!(default_modes.mouse_mode, MouseProtocolMode::None);
    assert_eq!(default_modes.mouse_encoding, MouseProtocolEncoding::Default);

    let mut screen = AlacrittyScreen::new(80, 24, 100);
    assert_eq!(screen.modes().mouse_mode, MouseProtocolMode::None);
    assert_eq!(
        screen.modes().mouse_encoding,
        MouseProtocolEncoding::Default
    );

    // Legacy X10 ?9h is intentionally ignored by alacritty_terminal
    screen.process(b"\x1b[?9h");
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

    // Control characters inside screen bytes never leak into AlacrittyScreen::cell
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

#[test]
fn alacritty_screen_format_vs_and_zwj_sanitization() {
    use unicode_width::UnicodeWidthStr;

    let mut screen = AlacrittyScreen::new(30, 5, 100);
    // Row 0: VS16 warning sign followed immediately by 'X', plus ZWJ family emoji followed by 'Y'
    screen.process("\u{26A0}\u{FE0F}X \u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}Y".as_bytes());
    screen.process(b"\r\n");
    // Row 1: Soft hyphen, BIDI overrides/isolates, Cf format chars attached to ASCII and standalone
    screen.process("a\u{00AD}b\u{202E}c\u{2066}d\u{200F}\u{061C}\u{FEFF}e".as_bytes());
    screen.process(b"\r\n");
    // Row 2: Arabic with harakat (Mn combining marks) must stay intact
    screen.process("بِ لَّ شُ".as_bytes());

    // Row 0 checks: ⚠️ collapses to ⚠ (width 1) and does not eat 'X'
    let warn_cell = screen.cell(0, 0).expect("warn cell");
    assert_eq!(warn_cell.symbol, "\u{26A0}");
    assert_eq!(UnicodeWidthStr::width(warn_cell.symbol.as_str()), 1);
    let x_cell = screen.cell(0, 1).expect("X cell");
    assert_eq!(x_cell.symbol, "X");

    // ZWJ sequence has ZWJ stripped cleanly and does not exceed vt100's 2-col cell width
    let zwj_cell_3 = screen.cell(0, 3).expect("zwj cell 3");
    assert_eq!(zwj_cell_3.symbol, "\u{1F468}");
    assert!(!zwj_cell_3.symbol.contains('\u{200D}'));
    assert_eq!(UnicodeWidthStr::width(zwj_cell_3.symbol.as_str()), 2);
    let zwj_cell_5 = screen.cell(0, 5).expect("zwj cell 5");
    assert_eq!(zwj_cell_5.symbol, "\u{1F469}");
    assert!(!zwj_cell_5.symbol.contains('\u{200D}'));

    // Render row 0 into Buffer and verify 'X' and 'Y' sit in their exact columns
    let area = Rect::new(0, 0, 30, 5);
    let mut buf = Buffer::empty(area);
    screen.render_to_buffer(area, &mut buf);
    assert_eq!(buf.cell((0, 0)).unwrap().symbol(), "\u{26A0}");
    assert_eq!(buf.cell((1, 0)).unwrap().symbol(), "X");
    assert_eq!(buf.cell((3, 0)).unwrap().symbol(), "\u{1F468}");
    assert_eq!(buf.cell((4, 0)).unwrap().symbol(), " ");
    assert_eq!(buf.cell((5, 0)).unwrap().symbol(), "\u{1F469}");
    assert_eq!(buf.cell((6, 0)).unwrap().symbol(), " ");
    assert_eq!(buf.cell((7, 0)).unwrap().symbol(), "\u{1F467}");
    assert_eq!(buf.cell((8, 0)).unwrap().symbol(), " ");
    assert_eq!(buf.cell((9, 0)).unwrap().symbol(), "Y");

    // Row 1 checks: no Cf / U+00AD / VS codepoints remain in any cell
    for col in 0..10 {
        let sym = &screen.cell(1, col).expect("row1 cell").symbol;
        assert!(
            !sym.chars().any(|c| matches!(
                c,
                '\u{00AD}' | '\u{061C}' | '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2060}'..='\u{206F}' | '\u{FEFF}' | '\u{FE00}'..='\u{FE0F}'
            )),
            "format/VS char leaked at col {col}: {sym:?}"
        );
    }
    assert_eq!(screen.row_text(1), "abcde");

    // Row 2 checks: Arabic harakat remain intact
    let row2 = screen.row_text(2);
    assert!(row2.contains("بِ"), "got {row2:?}");
    assert!(row2.contains("لَّ"), "got {row2:?}");
    assert!(row2.contains("شُ"), "got {row2:?}");
}

#[derive(Clone, Default)]
struct SharedVecWriter(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);

impl SharedVecWriter {
    fn take_string(&self) -> String {
        let mut guard = self.0.lock().unwrap();
        let bytes = std::mem::take(&mut *guard);
        String::from_utf8(bytes).expect("valid utf8 ansi stream")
    }
}

impl std::io::Write for SharedVecWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[test]
fn alacritty_screen_sanitization_end_to_end_with_sync_backend() {
    use herdr_tui::ui::backend::SyncCrosstermBackend;
    use ratatui::{Terminal, TerminalOptions, Viewport};

    let mut screen = AlacrittyScreen::new(30, 4, 100);
    // Row 0: ⚠️ followed by 'X', then ASCII run "HELLO", then divider '│' and 'Z'
    screen.process("\u{26A0}\u{FE0F}XHELLO│Z".as_bytes());
    screen.process(b"\r\n");
    // Row 1: BIDI / soft-hyphen / ZWJ + Arabic harakat
    screen
        .process("a\u{00AD}\u{202E}b\u{2066}\u{FEFF} \u{1F468}\u{200D}\u{1F469} بِ لَّ شُ".as_bytes());

    let area = Rect::new(0, 0, 30, 4);
    let writer = SharedVecWriter::default();
    let backend = SyncCrosstermBackend::new(writer.clone());
    let mut terminal = Terminal::with_options(
        backend,
        TerminalOptions {
            viewport: Viewport::Fixed(area),
        },
    )
    .expect("create fixed terminal");

    terminal
        .draw(|f| {
            screen.render_to_buffer(f.area(), f.buffer_mut());
        })
        .expect("draw frame");

    let ansi = writer.take_string();
    assert!(
        ansi.starts_with("\x1b[?2026h"),
        "missing sync begin: {ansi:?}"
    );
    let body = ansi.strip_suffix("\x1b[?25l").unwrap_or(&ansi);
    assert!(body.ends_with("\x1b[?2026l"), "missing sync end: {ansi:?}");

    // VS16, ZWJ, soft hyphen, BIDI, BOM must not appear anywhere in the ANSI stream
    for forbidden in [
        '\u{FE0F}', '\u{200D}', '\u{00AD}', '\u{202E}', '\u{2066}', '\u{FEFF}',
    ] {
        assert!(
            !ansi.contains(forbidden),
            "forbidden char {forbidden:?} leaked into ANSI output: {ansi:?}"
        );
    }

    // ⚠️ sanitized to ⚠ at (0,0), immediately followed by MoveTo(1,0) = \x1b[1;2H before 'X',
    // then consecutive ASCII "XHELLO" elides MoveTo until '│', then MoveTo(8,0) = \x1b[1;9H before 'Z'
    assert!(
        ansi.contains("⚠\x1b[1;2HXHELLO│\x1b[1;9HZ"),
        "expected MoveTo after ⚠ and │ with ASCII elision in {ansi:?}"
    );

    // Row 1: "ab" elides MoveTo, Arabic harakat preserved and followed by MoveTo
    assert!(ansi.contains("ab"), "expected sanitized 'ab' in {ansi:?}");
    assert!(ansi.contains("بِ"), "expected 'بِ' preserved in {ansi:?}");
    assert!(ansi.contains("لَّ"), "expected 'لَّ' preserved in {ansi:?}");
    assert!(ansi.contains("شُ"), "expected 'شُ' preserved in {ansi:?}");
}

#[test]
fn alacritty_screen_rep_lualine_statusline_alignment() {
    let mut screen = AlacrittyScreen::new(80, 24, 100);
    // Simulate lualine statusline on bottom row (row 24, 1-based):
    // " NORMAL  main.rs" (16 cols) + 52 spaces via CSI 51 b + "1%" (2 cols) + 5 spaces via CSI 4 b + "20:56" (5 cols) = 80 cols
    screen.process(b"\x1b[24;1H NORMAL  main.rs \x1b[51b1% \x1b[4b20:56");

    let row23 = screen.row_text(23);
    assert_eq!(
        row23.len(),
        80,
        "expected full 80-col statusline: {row23:?}"
    );
    assert!(row23.starts_with(" NORMAL  main.rs"));
    assert!(row23.ends_with("1%     20:56"));
    assert_eq!(screen.cell(23, 68).unwrap().symbol, "1");
    assert_eq!(screen.cell(23, 69).unwrap().symbol, "%");
    assert_eq!(screen.cell(23, 75).unwrap().symbol, "2");
    assert_eq!(screen.cell(23, 79).unwrap().symbol, "6");
}

#[test]
fn alacritty_screen_nvim_sigwinch_redraw_sequences() {
    use ratatui::style::{Color, Modifier};

    let mut screen = AlacrittyScreen::new(80, 24, 100);
    // Enter alt screen and paint stale right-edge marker at col 79
    screen.process(b"\x1b[?1049h\x1b[1;79HSB");
    assert!(screen.modes().alternate_screen);
    assert_eq!(screen.cell(0, 78).unwrap().symbol, "S");

    // Resize to 50x16 (SIGWINCH) and deliver synchronized update split across two chunks
    screen.resize(50, 16);
    assert_eq!(screen.size(), (50, 16));

    // Chunk 1: Begin Sync Update (CSI ? 2026 h) + clear + HVP + colon-SGR
    screen.process(b"\x1b[?2026h\x1b[2J\x1b[1;1f\x1b[38:2::192:202:245;4:3mnvim\x1b[0m");
    // While BSU is active and before ESU arrives, the new frame is buffered
    assert_ne!(screen.row_text(0), "nvim");

    // Chunk 2: CNL (2E), CPL (1F), HPA (`), CHT (I), CBT (Z), ECH (X), REP (b), and ESU (?2026l)
    screen.process(
        b"\x1b[3ECD\x1b[1FEF\x1b[5`12345\x1b[6`\x1b[2X\x1b[5;1H\x1b[I\x1b[I\x1b[Ztab\x1b[16;1H1% \x1b[41b20:56\x1b[?2026l",
    );

    assert!(screen.row_text(0).starts_with("nvim"));
    let nvim_cell = screen.cell(0, 0).unwrap();
    assert_eq!(nvim_cell.fg, Color::Rgb(192, 202, 245));
    assert!(nvim_cell.modifiers.contains(Modifier::UNDERLINED));

    // CNL 3 from row 0 -> row 3 col 0 ("CD"), CPL 1 -> row 2 col 0 ("EF"), HPA 5 -> col 4 ("1  45")
    assert!(screen.row_text(2).starts_with("EF"));
    assert_eq!(screen.cell(2, 4).unwrap().symbol, "1");
    assert_eq!(screen.cell(2, 5).unwrap().symbol, " ");
    assert_eq!(screen.cell(2, 6).unwrap().symbol, " ");
    assert_eq!(screen.cell(2, 7).unwrap().symbol, "4");
    assert_eq!(screen.row_text(3), "CD");

    // CHT x2 (col 0 -> 8 -> 16), CBT x1 (16 -> 8) -> "tab" at col 8
    assert_eq!(screen.cell(4, 8).unwrap().symbol, "t");
    assert_eq!(screen.cell(4, 10).unwrap().symbol, "b");

    // Statusline on row 15: "1%" + 42 spaces (1 + 41 REP) + "20:56" = 49? Wait: 2+1+41+5 = 49 -> cols 44..49
    let status = screen.row_text(15);
    assert!(status.starts_with("1%"));
    assert!(status.ends_with("20:56"));
    assert_eq!(screen.cell(15, 44).unwrap().symbol, "2");

    // Unterminated CSI ? 2026 h recovers cleanly after the 150ms sync timeout
    screen.process(b"\x1b[?2026h\x1b[1;1Htimeout-ok");
    std::thread::sleep(Duration::from_millis(165));
    screen.process(b"");
    assert!(screen.row_text(0).starts_with("timeout-ok"));

    // Exit alt screen
    screen.process(b"\x1b[?1049l");
    assert!(!screen.modes().alternate_screen);
}

#[test]
fn alacritty_screen_bidirectional_soft_wrap_reflow() {
    // 1. Blank-bottom-row preservation: shrinking width on a tall pane reclaims wrapped lines
    // into trailing blank rows instead of pushing them into scrollback.
    let mut screen = AlacrittyScreen::new(40, 10, 100);
    let long_line = "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789abcdefghijklmnopqrstuvwx"; // 60 chars
    screen.process(long_line.as_bytes());
    assert_eq!(screen.row_text(0), &long_line[..40]);
    assert_eq!(screen.row_text(1), &long_line[40..]);

    screen.resize(20, 10);
    assert_eq!(screen.row_text(0), &long_line[..20]);
    assert_eq!(screen.row_text(1), &long_line[20..40]);
    assert_eq!(screen.row_text(2), &long_line[40..60]);
    screen.set_scrollback(1);
    assert_eq!(
        screen.scrollback(),
        0,
        "trailing blank rows should absorb wrapped lines without pushing to scrollback"
    );

    screen.resize(80, 10);
    assert_eq!(screen.row_text(0), long_line);
    assert_eq!(screen.row_text(1), "");

    // 2. Full-screen scrollback reflow: when no blank rows remain below (40x2), shrinking to 20x2
    // pushes the top wrapped chunk into scrollback, and growing to 80x2 pulls it back onto row 0.
    let mut full_screen = AlacrittyScreen::new(40, 2, 100);
    full_screen.process(long_line.as_bytes());
    assert_eq!(full_screen.row_text(0), &long_line[..40]);
    assert_eq!(full_screen.row_text(1), &long_line[40..]);

    full_screen.resize(20, 2);
    assert_eq!(full_screen.row_text(0), &long_line[20..40]);
    assert_eq!(full_screen.row_text(1), &long_line[40..60]);
    full_screen.set_scrollback(1);
    assert_eq!(full_screen.scrollback(), 1);
    assert_eq!(full_screen.row_text(0), &long_line[..20]);
    full_screen.set_scrollback(0);

    full_screen.resize(80, 2);
    assert_eq!(full_screen.row_text(0), long_line);
    assert_eq!(full_screen.row_text(1), "");

    // 3. Alt screen resizes cleanly without bogus primary reflow
    screen.process(b"\x1b[?1049h\x1b[1;1HALT_LINE");
    assert!(screen.modes().alternate_screen);
    screen.resize(20, 6);
    assert_eq!(screen.size(), (20, 6));
    assert!(screen.modes().alternate_screen);
    assert_eq!(screen.row_text(0), "ALT_LINE");
}

#[test]
fn alacritty_screen_scrollback_modes_and_wide_cjk_invariants() {
    let mut screen = AlacrittyScreen::new(20, 5, 100);
    for i in 0..15 {
        screen.process(format!("row-{i:02}\r\n").as_bytes());
    }
    assert!(screen.cursor().2);
    screen.set_scrollback(5);
    assert_eq!(screen.scrollback(), 5);
    assert!(
        !screen.cursor().2,
        "cursor must be hidden while viewing scrollback"
    );
    let top_row = screen.row_text(0);
    assert!(top_row.starts_with("row-"), "got {top_row:?}");
    screen.set_scrollback(0);
    assert_eq!(screen.scrollback(), 0);
    assert!(screen.cursor().2);

    // APP_CURSOR, BRACKETED_PASTE, ALT_SCREEN
    screen.process(b"\x1b[?1h\x1b[?2004h\x1b[?1049h");
    let modes = screen.modes();
    assert!(modes.app_cursor);
    assert!(modes.bracketed_paste);
    assert!(modes.alternate_screen);

    // Wide CJK + Arabic harakat + Cf/VS/ZWJ sanitization
    screen.process(b"\x1b[1;1H");
    screen.process("漢بِ\u{200B}\u{FE0F}a\u{00AD}b".as_bytes());
    assert_eq!(screen.cell(0, 0).unwrap().symbol, "漢");
    assert!(screen.cell(0, 1).unwrap().wide_continuation);
    assert_eq!(screen.cell(0, 2).unwrap().symbol, "بِ");
    assert_eq!(screen.cell(0, 3).unwrap().symbol, "a");
    assert_eq!(screen.cell(0, 4).unwrap().symbol, "b");
}

#[tokio::test]
async fn attach_session_drains_buffered_old_width_frames_before_resize() {
    use fake_hc::make_terminal;

    let srv = FakeHcServer::start().await.expect("start fake server");
    srv.add_terminal(make_terminal("t-drain", "zsh")).await;
    let framed = FramedClient::connect(srv.socket_path())
        .await
        .expect("framed");

    let mut session = AttachSession::new("t-drain".to_string(), 80, 10);
    session.attach_active(&framed).await.expect("attach");
    let channel = session.channel.as_ref().unwrap().channel;

    // 1. Queued 80-col DATA frame must be processed at 80 cols before request_resize shrinks to 40
    srv.send_attach_data(
        channel,
        b"htui (main) > \x1b[63C \xE2\x9C\x93 \x1b[65D".to_vec(),
    )
    .await;
    tokio::time::sleep(Duration::from_millis(20)).await;

    let t0 = Instant::now();
    assert!(session.request_resize(40, 10, t0).expect("request_resize"));
    assert_eq!(session.screen.size(), (40, 10));
    // Drain again (simulating tick): nothing old-width should remain to corrupt the 40-col screen
    session.drain_frames();
    assert_eq!(session.screen.row_text(0), "htui (main) >");
    assert_eq!(
        session.screen.cursor(),
        (14, 0, true),
        "old-width frame must have been processed before request_resize shrank screen"
    );

    // 2. Same check for flush_pending_resize when debounced
    assert!(!session
        .request_resize(60, 10, t0 + Duration::from_millis(5))
        .expect("debounced"));
    // Send a 40-col frame while screen is still 40x10 and resize to 60x10 is pending
    srv.send_attach_data(
        channel,
        b"\r\x1b[Jhtui (main) > \x1b[23C \xE2\x9C\x93 \x1b[25D".to_vec(),
    )
    .await;
    tokio::time::sleep(Duration::from_millis(20)).await;

    assert!(session
        .flush_pending_resize(t0 + Duration::from_millis(60))
        .expect("flush_pending_resize"));
    assert_eq!(session.screen.size(), (60, 10));
    session.drain_frames();
    assert_eq!(
        session.screen.row_text(0),
        "htui (main) >                         \u{2713}"
    );
    assert_eq!(session.screen.row_text(1), "");
    assert_eq!(session.screen.cursor(), (14, 0, true));
}

fn assert_clean_zsh_screen(screen: &AlacrittyScreen, expected_ls_line: Option<&str>, ctx: &str) {
    let (_, rows) = screen.size();
    let all_rows: Vec<String> = (0..rows).map(|r| screen.row_text(r)).collect();
    let prompt_count: usize = all_rows
        .iter()
        .map(|r| r.matches("htui (main) >").count())
        .sum();
    assert_eq!(
        prompt_count, 1,
        "{ctx}: expected exactly 1 prompt across viewport, got {prompt_count} in {all_rows:?}"
    );
    for (r_idx, row) in all_rows.iter().enumerate() {
        assert!(
            row.matches("htui (main) >").count() <= 1,
            "{ctx}: duplicated prompt on row {r_idx}: {row:?}"
        );
        assert_ne!(
            row.trim(),
            "\u{2713}",
            "{ctx}: orphaned RPROMPT checkmark on row {r_idx}: {all_rows:?}"
        );
        assert!(
            row.trim() != "ht" && row.trim() != "ui" && row.trim() != "mai",
            "{ctx}: mid-word split fragment on row {r_idx}: {all_rows:?}"
        );
    }
    if let Some(ls_line) = expected_ls_line {
        assert!(
            all_rows.iter().any(|r| r == ls_line),
            "{ctx}: expected ls output {ls_line:?} intact in {all_rows:?}"
        );
    }
}

fn zsh_rprompt_bytes(cols: u16) -> Vec<u8> {
    let prompt = "htui (main) > ";
    let gap = cols.saturating_sub(prompt.len() as u16 + 3);
    format!(
        "\r\r\x1b[0m\x1b[27m\x1b[24m\x1b[J{prompt}\x1b[K\x1b[{gap}C \u{2713} \x1b[{}D",
        gap + 2
    )
    .into_bytes()
}

#[test]
fn zsh_rprompt_multi_step_shrink_and_grow_keeps_ls_intact_and_single_prompt() {
    let ls_line = "Cargo.toml  README.md  src  tests";
    let mut screen = AlacrittyScreen::new(138, 41, 1000);
    screen.process(format!("$ ls\r\n{ls_line}\r\n").as_bytes());
    screen.process(&zsh_rprompt_bytes(138));
    assert_clean_zsh_screen(&screen, Some(ls_line), "initial 138x41");

    // Multi-step shrink with zsh SIGWINCH redraws at intermediate sizes
    for (cols, rows) in [(115, 36), (90, 32), (69, 29)] {
        screen.resize(cols, rows);
        screen.process(&zsh_rprompt_bytes(cols));
        assert_clean_zsh_screen(&screen, Some(ls_line), &format!("shrink {cols}x{rows}"));
    }

    // Two consecutive shrinks before zsh redraws, then redraw + grow back to 134x39
    screen.resize(58, 27);
    screen.resize(50, 25);
    screen.process(&zsh_rprompt_bytes(50));
    assert_clean_zsh_screen(&screen, Some(ls_line), "double-shrink 50x25");

    for (cols, rows) in [(85, 33), (110, 36), (134, 39)] {
        screen.resize(cols, rows);
        screen.process(&zsh_rprompt_bytes(cols));
        assert_clean_zsh_screen(&screen, Some(ls_line), &format!("grow {cols}x{rows}"));
    }

    assert_eq!(screen.row_text(0), "$ ls");
    assert_eq!(screen.row_text(1), ls_line);
    assert!(screen.row_text(2).starts_with("htui (main) >"));
    assert!(screen.row_text(2).ends_with('\u{2713}'));
}

#[tokio::test]
async fn zsh_rprompt_line0_shrink_grow_and_buffered_frame_drain() {
    use fake_hc::make_terminal;

    let srv = FakeHcServer::start().await.expect("start fake server");
    srv.add_terminal(make_terminal("t-zsh", "zsh")).await;
    let framed = FramedClient::connect(srv.socket_path())
        .await
        .expect("framed");

    let mut session = AttachSession::new("t-zsh".to_string(), 80, 20);
    session.attach_active(&framed).await.expect("attach");
    let ch = session.channel.as_ref().unwrap().channel;

    // Queue an 80-col Line(0) zsh RPROMPT frame in ch.rx right before shrinking to 40x20
    srv.send_attach_data(ch, zsh_rprompt_bytes(80)).await;
    tokio::time::sleep(Duration::from_millis(20)).await;

    let t0 = Instant::now();
    assert!(session.request_resize(40, 20, t0).expect("shrink 40x20"));
    assert_eq!(session.screen.row_text(0), "htui (main) >");
    assert_eq!(session.screen.cursor(), (14, 0, true));

    // zsh SIGWINCH redraw at 40 cols, then grow back to 80x20
    srv.send_attach_data(ch, zsh_rprompt_bytes(40)).await;
    tokio::time::sleep(Duration::from_millis(20)).await;
    assert!(session
        .request_resize(80, 20, t0 + Duration::from_millis(60))
        .expect("grow 80x20"));
    session.drain_frames();
    assert_eq!(session.screen.cursor(), (14, 0, true));
    assert!(session.screen.row_text(0).starts_with("htui (main) >"));
    assert!(session.screen.row_text(0).ends_with('\u{2713}'));
    assert_eq!(session.screen.row_text(1), "");
}

#[tokio::test]
async fn attach_session_emits_in_band_resize_on_active_snapshot_resize_flush_and_takeover() {
    use fake_hc::{make_terminal, FRAME_DATA};

    let srv = FakeHcServer::start().await.expect("start fake server");
    srv.add_terminal(make_terminal("t-2048-a", "nvim")).await;
    srv.add_terminal(make_terminal("t-2048-b", "nvim")).await;
    {
        let mut st = srv.state.lock().await;
        st.echo_attach_input = false;
        st.attach_outputs
            .insert("t-2048-a".to_string(), b"\x1b[?1049h\x1b[?2048h".to_vec());
        st.attach_outputs
            .insert("t-2048-b".to_string(), b"\x1b[?1049h\x1b[?2048h".to_vec());
    }
    let framed = FramedClient::connect(srv.socket_path())
        .await
        .expect("framed");

    // 1. attach_active receives snapshot with ?2048h -> emits CSI 48 once, no duplicate on replay
    let mut sess_a = AttachSession::new("t-2048-a".to_string(), 80, 24);
    sess_a.attach_active(&framed).await.expect("attach_active");
    let ch_a = sess_a.channel.as_ref().unwrap().channel;
    let first = sess_a.recv_next().await.expect("snapshot frame");
    sess_a.handle_frame(first);
    assert!(sess_a.screen.modes().in_band_resize);

    // Replaying ?2048h at same size must not emit duplicate CSI 48
    srv.send_attach_data(ch_a, b"\x1b[?2048h".to_vec()).await;
    tokio::time::sleep(Duration::from_millis(20)).await;
    sess_a.drain_frames();

    // 2. request_resize and debounced flush_pending_resize emit CSI 48 once per dimension
    let t0 = Instant::now();
    assert!(sess_a.request_resize(100, 30, t0).expect("resize 100x30"));
    assert!(!sess_a
        .request_resize(90, 28, t0 + Duration::from_millis(10))
        .expect("debounced 90x28"));
    assert!(sess_a
        .flush_pending_resize(t0 + Duration::from_millis(60))
        .expect("flush 90x28"));

    tokio::time::sleep(Duration::from_millis(30)).await;
    let frames_a: Vec<Vec<u8>> = srv
        .recorded_frames()
        .await
        .into_iter()
        .filter(|f| f.channel == ch_a && f.kind == FRAME_DATA)
        .map(|f| f.payload)
        .collect();
    assert_eq!(
        frames_a,
        vec![
            b"\x1b[48;24;80;0;0t".to_vec(),
            b"\x1b[48;30;100;0;0t".to_vec(),
            b"\x1b[48;28;90;0;0t".to_vec(),
        ]
    );

    // 3. Observing suppresses CSI 48; take_over_at emits CSI 48 once without duplicating on snapshot
    let mut sess_b = AttachSession::new("t-2048-b".to_string(), 80, 24);
    sess_b
        .attach_observe(&framed)
        .await
        .expect("attach_observe");
    let obs_ch = sess_b.channel.as_ref().unwrap().channel;
    let obs_snap = sess_b.recv_next().await.expect("obs snapshot");
    sess_b.handle_frame(obs_snap);
    assert!(sess_b.screen.modes().in_band_resize);
    tokio::time::sleep(Duration::from_millis(20)).await;
    let obs_data_count = srv
        .recorded_frames()
        .await
        .into_iter()
        .filter(|f| f.channel == obs_ch && f.kind == FRAME_DATA)
        .count();
    assert_eq!(obs_data_count, 0, "Observing must never emit CSI 48");

    sess_b
        .take_over_at(&framed, 120, 40)
        .await
        .expect("take_over_at");
    let active_ch = sess_b.channel.as_ref().unwrap().channel;
    let active_snap = sess_b.recv_next().await.expect("active snapshot");
    sess_b.handle_frame(active_snap);
    tokio::time::sleep(Duration::from_millis(30)).await;

    let frames_b: Vec<Vec<u8>> = srv
        .recorded_frames()
        .await
        .into_iter()
        .filter(|f| f.channel == active_ch && f.kind == FRAME_DATA)
        .map(|f| f.payload)
        .collect();
    assert_eq!(frames_b, vec![b"\x1b[48;40;120;0;0t".to_vec()]);
}

#[tokio::test]
async fn attach_session_suppresses_in_band_resize_when_observing_or_mode_2048_disabled() {
    use fake_hc::{make_terminal, FRAME_DATA, FRAME_RESIZE};

    let srv = FakeHcServer::start().await.expect("start fake server");
    srv.add_terminal(make_terminal("t-off", "nvim")).await;
    {
        let mut st = srv.state.lock().await;
        st.echo_attach_input = false;
    }
    let framed = FramedClient::connect(srv.socket_path())
        .await
        .expect("framed");

    // Observing session ignores server FrameKind::Resize even if ?2048h was seen
    let mut sess = AttachSession::new("t-off".to_string(), 80, 24);
    sess.attach_observe(&framed).await.expect("observe");
    let obs_ch = sess.channel.as_ref().unwrap().channel;
    srv.send_attach_data(obs_ch, b"\x1b[?1049h\x1b[?2048h".to_vec())
        .await;
    srv.send_attach_resize(obs_ch, 90, 25).await;
    tokio::time::sleep(Duration::from_millis(20)).await;
    sess.drain_frames();
    assert!(sess.screen.modes().in_band_resize);

    // Promote to Active at 90x25 -> emits CSI 48 for 90x25
    sess.take_over_at(&framed, 90, 25)
        .await
        .expect("take_over_at");
    let act_ch = sess.channel.as_ref().unwrap().channel;

    // Disable ?2048l -> subsequent resize emits only FRAME_RESIZE, no CSI 48
    srv.send_attach_data(act_ch, b"\x1b[?2048l".to_vec()).await;
    tokio::time::sleep(Duration::from_millis(20)).await;
    sess.drain_frames();
    assert!(!sess.screen.modes().in_band_resize);

    let t0 = Instant::now();
    assert!(sess
        .request_resize(70, 22, t0)
        .expect("resize after ?2048l"));

    // Re-enable ?2048h on alt screen, then exit alt screen via ?1049l without ?2048l
    srv.send_attach_data(act_ch, b"\x1b[?2048h\x1b[?1049l".to_vec())
        .await;
    tokio::time::sleep(Duration::from_millis(20)).await;
    sess.drain_frames();
    assert!(!sess.screen.modes().in_band_resize);

    assert!(sess
        .request_resize(60, 20, t0 + Duration::from_millis(60))
        .expect("resize after ?1049l"));

    tokio::time::sleep(Duration::from_millis(30)).await;
    let recorded = srv.recorded_frames().await;
    let obs_data: Vec<_> = recorded
        .iter()
        .filter(|f| f.channel == obs_ch && f.kind == FRAME_DATA)
        .collect();
    assert!(obs_data.is_empty(), "Observing must emit zero FRAME_DATA");

    let act_data: Vec<Vec<u8>> = recorded
        .iter()
        .filter(|f| f.channel == act_ch && f.kind == FRAME_DATA)
        .map(|f| f.payload.clone())
        .collect();
    assert_eq!(
        act_data,
        vec![b"\x1b[48;25;90;0;0t".to_vec()],
        "resizes after ?2048l and ?1049l must not emit CSI 48"
    );

    let resize_count = recorded
        .iter()
        .filter(|f| f.channel == act_ch && f.kind == FRAME_RESIZE)
        .count();
    assert_eq!(resize_count, 2);
}

fn nvim_cargo_toml_frame(cols: u16, rows: u16) -> Vec<u8> {
    let mut out = String::new();
    let cols_usize = cols as usize;
    let rows_usize = rows as usize;
    // Kanagawa Wave background #1f1f28, clear entire alt screen cleanly
    out.push_str("\x1b[?2026h\x1b[48;2;31;31;40m\x1b[38;2;220;215;186m\x1b[2J");
    // Row 1: bufferline across full width
    let bufline = format!("{:<width$}", "  Cargo.toml  ×", width = cols_usize);
    out.push_str(&format!("\x1b[1;1H{}", &bufline[..cols_usize]));
    // Content rows 2..(rows-1): numbered lines 9.. with line 19 inline git-blame & colorcolumn=120 marker
    let content_rows = rows_usize.saturating_sub(2);
    for idx in 0..content_rows {
        let line_no = 9 + idx;
        let body = if line_no == 19 {
            "tokio = { version = \"1\", features = [\"full\"] }  // tawargy: feat"
        } else if line_no == 27 {
            "ratatui = \"0.29\"  # tui framework"
        } else {
            "serde = { version = \"1\", features = [\"derive\"] }"
        };
        let line = format!("{line_no:>3} {body}");
        let padded: String = format!("{line:<width$}", width = cols_usize)
            .chars()
            .take(cols_usize)
            .collect();
        out.push_str(&format!("\x1b[{};1H{padded}", idx + 2));
    }
    // Bottom row: single-row lualine with right-aligned line:col
    if rows_usize >= 2 {
        let left = " NORMAL   Cargo.toml ";
        let right = " 19:1 ";
        let gap = cols_usize.saturating_sub(left.len() + right.len());
        let lualine: String = format!("{left}{}{right}", " ".repeat(gap))
            .chars()
            .take(cols_usize)
            .collect();
        out.push_str(&format!("\x1b[{rows_usize};1H\x1b[48;2;42;42;55m{lualine}"));
    }
    out.push_str("\x1b[0m\x1b[?2026l");
    out.into_bytes()
}

#[tokio::test]
async fn nvim_cargo_toml_mode_2048_attach_shrink_grow_and_exit_to_zsh() {
    use fake_hc::{make_terminal, FRAME_DATA};
    use ratatui::style::Color;

    let srv = FakeHcServer::start().await.expect("start fake server");
    let mut nvim_term = make_terminal("t-nvim-ct", "nvim");
    nvim_term["cols"] = serde_json::json!(200);
    nvim_term["rows"] = serde_json::json!(120);
    srv.add_terminal(nvim_term).await;
    {
        let mut st = srv.state.lock().await;
        st.echo_attach_input = false;
        st.attach_outputs
            .insert("t-nvim-ct".to_string(), b"\x1b[?1049h\x1b[?2048h".to_vec());
    }

    let framed = FramedClient::connect(srv.socket_path())
        .await
        .expect("framed");
    let mut sess = AttachSession::new("t-nvim-ct".to_string(), 134, 38);
    sess.attach_active(&framed).await.expect("attach_active");
    let ch = sess.channel.as_ref().unwrap().channel;

    let snap = sess.recv_next().await.expect("snapshot");
    sess.handle_frame(snap);
    assert!(sess.screen.modes().alternate_screen);
    assert!(sess.screen.modes().in_band_resize);

    // 1. Attach at 134x38 triggers CSI 48;38;134;0;0t and full-width redraw
    srv.send_attach_data(ch, nvim_cargo_toml_frame(134, 38))
        .await;
    tokio::time::sleep(Duration::from_millis(20)).await;
    sess.drain_frames();
    assert!(sess.screen.row_text(0).contains("Cargo.toml"));
    assert!(sess.screen.row_text(11).starts_with(" 19 tokio"));
    assert!(sess.screen.row_text(37).contains("NORMAL"));

    // 2. Shrink 134x38 -> 52x33 emits CSI 48;33;52;0;0t and redraws cleanly at 52 cols
    let t0 = Instant::now();
    assert!(sess.request_resize(52, 33, t0).expect("shrink 52x33"));
    srv.send_attach_data(ch, nvim_cargo_toml_frame(52, 33))
        .await;
    tokio::time::sleep(Duration::from_millis(20)).await;
    sess.drain_frames();
    assert!(sess.screen.row_text(11).starts_with(" 19 tokio"));
    assert!(sess.screen.row_text(12).starts_with(" 20 serde"));
    assert!(!sess.screen.row_text(31).contains("NORMAL"));
    assert!(sess.screen.row_text(32).contains("NORMAL"));
    assert!(sess.screen.row_text(32).contains("19:1"));

    // 3. Grow 52x33 -> 78x33 emits CSI 48;33;78;0;0t and renders full 78-col bg (no col-52 Reset gap)
    assert!(sess
        .request_resize(78, 33, t0 + Duration::from_millis(60))
        .expect("grow 78x33"));
    srv.send_attach_data(ch, nvim_cargo_toml_frame(78, 33))
        .await;
    tokio::time::sleep(Duration::from_millis(20)).await;
    sess.drain_frames();
    assert!(sess.screen.row_text(11).contains("tawargy: feat"));
    assert!(sess.screen.row_text(32).contains("NORMAL"));
    assert!(sess.screen.row_text(32).ends_with("19:1"));
    for col in 52..78 {
        assert_eq!(
            sess.screen.cell(11, col).unwrap().bg,
            Color::Rgb(31, 31, 40),
            "col {col} must not have stale Color::Reset gap"
        );
    }

    // 4. Exit nvim back to zsh (?2048l ?1049l) -> subsequent resize must NOT emit CSI 48
    srv.send_attach_data(ch, b"\x1b[?2048l\x1b[?1049lhtui > ".to_vec())
        .await;
    tokio::time::sleep(Duration::from_millis(20)).await;
    sess.drain_frames();
    assert!(!sess.screen.modes().alternate_screen);
    assert!(!sess.screen.modes().in_band_resize);
    assert_eq!(sess.screen.row_text(0), "htui >");

    assert!(sess
        .request_resize(100, 35, t0 + Duration::from_millis(120))
        .expect("post-exit zsh resize"));

    tokio::time::sleep(Duration::from_millis(30)).await;
    let emitted: Vec<Vec<u8>> = srv
        .recorded_frames()
        .await
        .into_iter()
        .filter(|f| f.channel == ch && f.kind == FRAME_DATA)
        .map(|f| f.payload)
        .collect();
    assert_eq!(
        emitted,
        vec![
            b"\x1b[48;38;134;0;0t".to_vec(),
            b"\x1b[48;33;52;0;0t".to_vec(),
            b"\x1b[48;33;78;0;0t".to_vec(),
        ]
    );
}

#[tokio::test]
async fn take_over_at_drains_queued_observing_mode_resets_before_in_band_resize() {
    use fake_hc::{make_terminal, FRAME_DATA};

    let srv = FakeHcServer::start().await.expect("start fake server");
    srv.add_terminal(make_terminal("t-obs-reset", "nvim")).await;
    {
        let mut st = srv.state.lock().await;
        st.echo_attach_input = false;
    }
    let framed = FramedClient::connect(srv.socket_path())
        .await
        .expect("framed");

    let mut sess = AttachSession::new("t-obs-reset".to_string(), 80, 24);
    sess.attach_observe(&framed).await.expect("attach_observe");
    let obs_ch = sess.channel.as_ref().unwrap().channel;

    // While observing, program enters alt screen + Mode 2048
    srv.send_attach_data(obs_ch, b"\x1b[?1049h\x1b[?2048h".to_vec())
        .await;
    tokio::time::sleep(Duration::from_millis(20)).await;
    sess.drain_frames();
    assert!(sess.screen.modes().in_band_resize);

    // Program exits alt screen + disables Mode 2048, queued in ch.rx before take_over_at
    srv.send_attach_data(obs_ch, b"\x1b[?2048l\x1b[?1049l".to_vec())
        .await;
    tokio::time::sleep(Duration::from_millis(20)).await;

    // Promote directly without manually calling drain_frames first
    sess.take_over_at(&framed, 90, 25)
        .await
        .expect("take_over_at");
    let act_ch = sess.channel.as_ref().unwrap().channel;

    assert!(!sess.screen.modes().in_band_resize);
    tokio::time::sleep(Duration::from_millis(30)).await;
    let act_data: Vec<Vec<u8>> = srv
        .recorded_frames()
        .await
        .into_iter()
        .filter(|f| f.channel == act_ch && f.kind == FRAME_DATA)
        .map(|f| f.payload)
        .collect();
    assert!(
        act_data.is_empty(),
        "queued ?2048l/?1049l must suppress CSI 48 on take_over_at, got {act_data:?}"
    );
}
