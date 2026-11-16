use std::collections::{HashMap, HashSet};
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crossterm::event::{
    DisableMouseCapture, EnableMouseCapture, Event, EventStream as CrosstermEvents,
};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use futures::StreamExt;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use tokio::time::MissedTickBehavior;

use crate::config::{AttachMode, Config};
use crate::hc::events::{EventMessage, EventStream};
use crate::hc::framed::FramedClient;
use crate::hc::rpc::RpcClient;
use crate::hc::types::{EventPayload, RunParams};
use crate::layout::presence::PresenceManager;
use crate::layout::panel::SpaceLayout;
use crate::layout::store::LayoutStore;
use crate::model::reducer::{apply, ModelInput};
use crate::model::tree::UNASSIGNED_SPACE;
use crate::term::session::{AttachSession, SessionState};
use crate::ui::app::{AppAction, TuiApp};
use crate::ui::dialogs::SpawnTarget;

pub fn restore_terminal() {
    let _ = disable_raw_mode();
    let _ = execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture);
}

pub fn install_panic_hook() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore_terminal();
        default_hook(info);
    }));
}

pub async fn run_tui(
    socket_path: PathBuf,
    config: Config,
    initial_space: Option<String>,
) -> anyhow::Result<()> {
    install_panic_hook();
    enable_raw_mode()?;
    execute!(io::stdout(), EnterAlternateScreen, EnableMouseCapture)?;

    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    let res = run_tui_loop(&mut terminal, &socket_path, config, initial_space).await;
    restore_terminal();
    res
}

async fn run_tui_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    socket_path: &Path,
    config: Config,
    initial_space: Option<String>,
) -> anyhow::Result<()> {
    let initial_space_name = initial_space.clone();
    let mut app = TuiApp::new(config.clone(), initial_space);
    let instance_id = format!("htui-{}", std::process::id());
    let presence = PresenceManager::new(instance_id);

    let mut rpc = RpcClient::connect(socket_path).await.ok();
    let mut framed = FramedClient::connect(socket_path).await.ok();
    let mut ev_stream: Option<EventStream> = None;

    let fallback_layout =
        Config::layout_fallback_path().unwrap_or_else(|| PathBuf::from("/tmp/htui-layout.json"));
    let methods = rpc
        .as_ref()
        .map(|r| r.hello().methods.clone())
        .unwrap_or_default();
    let store = LayoutStore::new(fallback_layout, &methods);

    if let Ok((snap, stream)) = EventStream::subscribe(socket_path).await {
        ev_stream = Some(stream);
        if let Some(client) = &rpc {
            app.model.local_core = client.hello().core.clone();
            if let Ok(tpls) = client.templates_list().await {
                app.templates = tpls.templates.into_iter().map(|t| t.name).collect();
            }
            if let Ok((doc, _)) = store.load(Some(client)).await {
                if !doc.spaces.is_empty() {
                    app.layouts = doc;
                }
            }
        }
        let live_terminals = snap.terminals.into_iter().filter(|t| t.is_live()).collect();
        app.model = apply(
            app.model,
            ModelInput::Snapshot {
                seq: snap.seq,
                terminals: live_terminals,
                machines: Vec::new(),
            },
        );
        reconcile_startup_layouts(
            &mut app,
            rpc.as_ref(),
            &store,
            initial_space_name.as_deref(),
        )
        .await;
    } else {
        init_disconnected_startup(&mut app, Instant::now());
    }

    let mut term_events = CrosstermEvents::new();
    let mut tick = tokio::time::interval(Duration::from_millis(16)); // 60 FPS cap
    tick.set_missed_tick_behavior(MissedTickBehavior::Skip);

    let mut reconnect_at: Option<Instant> = None;
    let mut backoff_secs: u64 = 1;

    loop {
        if app.should_quit {
            if let Some(client) = &rpc {
                let _ = presence.release_all(client).await;
            }
            break;
        }

        // Poll any incoming frames from attached sessions
        let mut any_frames = false;
        for sess in app.sessions.values_mut() {
            if sess.drain_frames() {
                any_frames = true;
            }
        }
        if any_frames {
            app.dirty = true;
        }

        // Handle reconnection if disconnected
        if !app.connected {
            let now = Instant::now();
            if reconnect_at.map_or(true, |t| now >= t) {
                if let (Ok(new_rpc), Ok(new_framed), Ok((snap, new_ev))) = (
                    RpcClient::connect(socket_path).await,
                    FramedClient::connect(socket_path).await,
                    EventStream::subscribe(socket_path).await,
                ) {
                    app.model.local_core = new_rpc.hello().core.clone();
                    let live_terminals =
                        snap.terminals.into_iter().filter(|t| t.is_live()).collect();
                    app.model = apply(
                        app.model,
                        ModelInput::Snapshot {
                            seq: snap.seq,
                            terminals: live_terminals,
                            machines: Vec::new(),
                        },
                    );
                    rpc = Some(new_rpc);
                    framed = Some(new_framed);
                    ev_stream = Some(new_ev);
                    app.connected = true;
                    backoff_secs = 1;
                    reconnect_at = None;
                    prune_and_sync_empty_spaces(&mut app, rpc.as_ref(), &store).await;
                    reconcile_and_sync_live_terminals(&mut app, rpc.as_ref(), &store).await;
                    app.set_toast("Reconnected to hc core", now);
                    app.refresh_sidebar();
                } else {
                    app.set_toast(format!("Disconnected; retrying in {backoff_secs}s..."), now);
                    reconnect_at = Some(now + Duration::from_secs(backoff_secs));
                    backoff_secs = (backoff_secs * 2).min(30);
                }
            }
        }

        if app.dirty {
            let now = Instant::now();
            terminal.draw(|f| {
                app.render(f.area(), f.buffer_mut(), now);
                if let Some((cx, cy)) = app.focused_cursor_position() {
                    f.set_cursor_position((cx, cy));
                }
            })?;
            app.sync_pane_sizes(now);
        }

        tokio::select! {
            _ = tick.tick() => {
                let now = Instant::now();
                app.router.tick(now);
                for sess in app.sessions.values_mut() {
                    let _ = sess.flush_pending_resize(now);
                }
            }
            Some(ev_res) = term_events.next() => {
                let now = Instant::now();
                let actions = match ev_res {
                    Ok(Event::Key(k)) => app.handle_key(k, now),
                    Ok(Event::Mouse(m)) => app.handle_mouse_event(m, now),
                    Ok(Event::Resize(_, _)) => {
                        app.dirty = true;
                        Vec::new()
                    }
                    _ => Vec::new(),
                };
                for act in actions {
                    dispatch_action(act, &mut app, rpc.as_ref(), framed.as_ref(), &store).await;
                }
            }
            ev_msg = async {
                match ev_stream.as_mut() {
                    Some(stream) => Some(stream.next_event().await),
                    None => std::future::pending().await,
                }
            } => {
                let now = Instant::now();
                match ev_msg {
                    Some(Ok(Some(EventMessage::Event(ev)))) => {
                        handle_core_event(ev, &mut app, rpc.as_ref(), &store).await;
                    }
                    Some(Ok(Some(EventMessage::Missed(mut m)))) => {
                        m.terminals.retain(|t| t.is_live());
                        app.model = apply(app.model, ModelInput::Missed(m));
                        prune_and_sync_empty_spaces(&mut app, rpc.as_ref(), &store).await;
                        reconcile_and_sync_live_terminals(&mut app, rpc.as_ref(), &store).await;
                        app.refresh_sidebar();
                    }
                    Some(Ok(Some(EventMessage::Heartbeat(_)))) => {}
                    Some(Ok(None)) | Some(Err(_)) => {
                        app.connected = false;
                        ev_stream = None;
                        app.set_toast("Core connection lost; reconnecting...", now);
                    }
                    None => {}
                }
            }
        }
    }
    Ok(())
}

pub fn init_disconnected_startup(app: &mut TuiApp, now: Instant) {
    app.connected = false;
    app.set_toast("Disconnected from hc core; retrying...", now);
    app.refresh_sidebar();
}

pub async fn reconcile_startup_layouts(
    app: &mut TuiApp,
    rpc: Option<&RpcClient>,
    store: &LayoutStore,
    initial_space_name: Option<&str>,
) {
    let present: HashSet<String> = app
        .model
        .terminals
        .iter()
        .filter(|(_, t)| t.is_live())
        .map(|(k, _)| k.clone())
        .collect();

    let mut pruned_spaces = Vec::new();
    for (sp, space_layout) in &mut app.layouts.spaces {
        let mut pruned = space_layout.prune_removed(&present);
        let mut label_pruned = false;
        for panel in &mut space_layout.panels {
            for pid in panel.tree.pane_ids() {
                if let Some(term_ref) = panel.tree.find_terminal(pid) {
                    if let Some(term) = app.model.terminals.get(&term_ref.address) {
                        let term_space = term
                            .labels
                            .get("space")
                            .map(String::as_str)
                            .filter(|s| !s.is_empty())
                            .unwrap_or(UNASSIGNED_SPACE);
                        if term_space != sp.as_str() {
                            panel.tree.set_terminal(pid, None);
                            pruned += 1;
                            label_pruned = true;
                        }
                    }
                }
            }
        }
        if label_pruned {
            space_layout.collapse_empty_splits_or_prune();
        }
        if pruned > 0 {
            pruned_spaces.push(sp.clone());
        }
    }

    if let Some(sp) = initial_space_name {
        if sp != UNASSIGNED_SPACE {
            app.model.declared_spaces.insert(sp.to_string());
            if let Some(space_layout) = app.layouts.spaces.get_mut(sp) {
                space_layout.collapse_empty_splits_or_prune();
            } else {
                app.layouts
                    .spaces
                    .insert(sp.to_string(), SpaceLayout::new());
            }
            app.switch_space(sp.to_string());
        }
    }

    if let Some(space_layout) = app.layouts.spaces.get_mut(UNASSIGNED_SPACE) {
        space_layout.collapse_empty_splits_or_prune();
    }

    let reconciled_spaces = app.reconcile_live_terminals();

    let mut removed_from_store: HashSet<String> = HashSet::new();
    let stale_spaces: Vec<String> = app
        .layouts
        .spaces
        .keys()
        .chain(app.model.declared_spaces.iter())
        .filter(|sp| {
            sp.as_str() != UNASSIGNED_SPACE
                && initial_space_name != Some(sp.as_str())
                && !app.space_has_open_terminals(sp)
        })
        .cloned()
        .collect();

    for sp in stale_spaces {
        app.remove_empty_space(&sp);
        removed_from_store.insert(sp);
    }

    let unassigned_empty = app
        .layouts
        .spaces
        .get(UNASSIGNED_SPACE)
        .is_some_and(|s| !s.has_terminals());
    if unassigned_empty {
        if app.active_space != UNASSIGNED_SPACE {
            app.layouts.spaces.remove(UNASSIGNED_SPACE);
        }
        removed_from_store.insert(UNASSIGNED_SPACE.to_string());
    }

    if !removed_from_store.is_empty() {
        let _ = store.prune_empty_spaces(rpc, &removed_from_store).await;
    }

    let mut spaces_to_save: HashSet<String> = pruned_spaces.into_iter().collect();
    spaces_to_save.extend(reconciled_spaces);
    for sp in spaces_to_save {
        if !removed_from_store.contains(&sp) {
            if let Some(space_layout) = app.layouts.spaces.get(&sp) {
                if space_layout.has_terminals() {
                    let _ = store.save_space(rpc, &sp, space_layout).await;
                }
            }
        }
    }

    let active = app.active_space.clone();
    let entry = app
        .layouts
        .spaces
        .entry(active)
        .or_insert_with(SpaceLayout::new);
    entry.ensure_valid_active_panel();
    let active_pane_ids = entry
        .active_panel()
        .map(|p| p.tree.pane_ids())
        .unwrap_or_default();
    if app
        .focused_pane
        .is_none_or(|pid| !active_pane_ids.contains(&pid))
    {
        app.focused_pane = active_pane_ids.into_iter().next();
    }

    app.refresh_sidebar();
}

async fn prune_and_sync_empty_spaces(
    app: &mut TuiApp,
    rpc: Option<&RpcClient>,
    store: &LayoutStore,
) {
    let emptied = app.prune_empty_spaces();
    for sp in &emptied {
        let _ = store.remove_space(rpc, sp).await;
    }
}

async fn reconcile_and_sync_live_terminals(
    app: &mut TuiApp,
    rpc: Option<&RpcClient>,
    store: &LayoutStore,
) {
    let modified = app.reconcile_live_terminals();
    for sp in modified {
        if let Some(space_layout) = app.layouts.spaces.get(&sp) {
            if space_layout.has_terminals() {
                let _ = store.save_space(rpc, &sp, space_layout).await;
            }
        }
    }
}

pub async fn handle_core_event(
    ev: EventPayload,
    app: &mut TuiApp,
    rpc: Option<&RpcClient>,
    store: &LayoutStore,
) {
    let is_exit = ev.event_type == "finished" || ev.event_type == "removed";
    let ev_type = ev.event_type.clone();
    let ev_term = ev.terminal.clone();
    app.model = apply(
        std::mem::take(&mut app.model),
        ModelInput::Event(ev, now_epoch_ms()),
    );
    if is_exit {
        if let Some(addr) = ev_term {
            app.sessions.remove(&addr);
            let emptied = app.prune_terminal_from_spaces(&addr);
            app.model.terminals.remove(&addr);
            for sp in &emptied {
                app.remove_empty_space(sp);
                let _ = store.remove_space(rpc, sp).await;
            }
            if ev_type == "finished" {
                if let Some(client) = rpc {
                    let _ = client.terminal_rm(&addr).await;
                }
            }
            app.dirty = true;
        }
    }
    prune_and_sync_empty_spaces(app, rpc, store).await;
    reconcile_and_sync_live_terminals(app, rpc, store).await;
    app.refresh_sidebar();
}

pub async fn dispatch_action(
    action: AppAction,
    app: &mut TuiApp,
    rpc: Option<&RpcClient>,
    framed: Option<&FramedClient>,
    store: &LayoutStore,
) {
    let now = Instant::now();
    match action {
        AppAction::Quit => {
            let removed = app.cleanup_empty_spaces_on_quit();
            let _ = store.prune_empty_spaces(rpc, &removed).await;
            app.should_quit = true;
        }
        AppAction::AttachTerminal { pane_id, terminal } => {
            app.sidebar_focused = false;
            let (cols, rows) = app.pane_inner_size(pane_id);
            let already_live = app.sessions.get(&terminal).is_some_and(|sess| {
                (sess.state == SessionState::Active || sess.state == SessionState::Observing)
                    && sess.channel.is_some()
            });
            if already_live {
                if let Some(sess) = app.sessions.get_mut(&terminal) {
                    sess.drain_frames();
                    if sess.screen.size() != (cols, rows) {
                        let _ = sess.request_resize(cols, rows, now);
                    }
                }
                app.dirty = true;
            } else if let Some(fc) = framed {
                let mut sess = AttachSession::new(terminal.clone(), cols, rows);
                let res = if app.config.attach.mode == AttachMode::Observe {
                    sess.attach_observe(fc).await
                } else {
                    sess.attach_active(fc).await
                };
                match res {
                    Ok(()) => {
                        sess.drain_frames();
                        app.sessions.insert(terminal, sess);
                        app.dirty = true;
                    }
                    Err(e) => {
                        app.set_toast(format!("Attach failed: {e}"), now);
                    }
                }
            }
        }
        AppAction::TakeOver(terminal) => {
            if let Some(fc) = framed {
                if let Some(sess) = app.sessions.get_mut(&terminal) {
                    if let Err(e) = sess.take_over(fc).await {
                        app.set_toast(format!("Take over failed: {e}"), now);
                    } else {
                        sess.drain_frames();
                        app.dirty = true;
                    }
                }
            }
        }
        AppAction::Spawn {
            command_or_template,
            task,
            space,
            target,
        } => {
            if let Some(client) = rpc {
                let mut argv = vec![command_or_template];
                if !task.is_empty() {
                    argv.push(task);
                }
                let mut labels = HashMap::new();
                let target_space = if !space.is_empty() && space != UNASSIGNED_SPACE {
                    labels.insert("space".to_string(), space.clone());
                    Some(space)
                } else {
                    None
                };
                match client
                    .terminal_run(RunParams {
                        argv,
                        labels,
                        ..Default::default()
                    })
                    .await
                {
                    Ok(term) => {
                        let addr = term.address.clone();
                        app.model.terminals.insert(addr.clone(), term);
                        let orig_space = app.active_space.clone();
                        let switched_space = if let Some(sp) = &target_space {
                            app.model.declared_spaces.insert(sp.clone());
                            if *sp != orig_space {
                                if let Some(removed) = app.switch_space(sp.clone()) {
                                    let _ = store.remove_space(Some(client), &removed).await;
                                }
                                true
                            } else {
                                false
                            }
                        } else {
                            false
                        };
                        let effective_target = if switched_space {
                            SpawnTarget::NewPanel
                        } else {
                            target
                        };
                        app.place_spawned_terminal(&addr, effective_target);
                        app.sidebar_focused = false;
                        app.refresh_sidebar();
                        if let Some(fc) = framed {
                            let (cols, rows) = app
                                .focused_pane
                                .map(|pid| app.pane_inner_size(pid))
                                .unwrap_or((80, 24));
                            let mut sess = AttachSession::new(addr.clone(), cols, rows);
                            if let Err(e) = sess.attach_active(fc).await {
                                app.set_toast(format!("Attach failed: {e}"), now);
                            } else {
                                sess.drain_frames();
                                app.sessions.insert(addr, sess);
                                app.dirty = true;
                            }
                        }
                        let space_key = app.active_space.clone();
                        if let Some(space_layout) = app.layouts.spaces.get(&space_key) {
                            let _ = store.save_space(Some(client), &space_key, space_layout).await;
                        }
                    }
                    Err(e) => {
                        app.set_toast(format!("Spawn failed: {e}"), now);
                    }
                }
            }
        }
        AppAction::Kill(addr) => {
            if let Some(client) = rpc {
                if let Err(e) = client.terminal_kill(&addr).await {
                    app.set_toast(format!("Kill failed: {e}"), now);
                } else {
                    app.sessions.remove(&addr);
                    if let Some(t) = app.model.terminals.get_mut(&addr) {
                        t.state = "finished".to_string();
                    }
                    let emptied = app.prune_terminal_from_spaces(&addr);
                    app.model.terminals.remove(&addr);
                    for sp in &emptied {
                        app.remove_empty_space(sp);
                        let _ = store.remove_space(Some(client), sp).await;
                    }
                    app.refresh_sidebar();
                    app.dirty = true;
                }
            }
        }
        AppAction::Remove(addr) => {
            if let Some(client) = rpc {
                if let Err(e) = client.terminal_rm(&addr).await {
                    app.set_toast(format!("Remove failed: {e}"), now);
                } else {
                    app.sessions.remove(&addr);
                    let emptied = app.prune_terminal_from_spaces(&addr);
                    app.model.terminals.remove(&addr);
                    for sp in &emptied {
                        app.remove_empty_space(sp);
                        let _ = store.remove_space(Some(client), sp).await;
                    }
                    app.refresh_sidebar();
                    app.dirty = true;
                }
            }
        }
        AppAction::RenameSpace {
            old_space,
            new_space,
        } => {
            if let Some(client) = rpc {
                let members: Vec<String> = app
                    .model
                    .terminals
                    .values()
                    .filter(|t| {
                        t.labels.get("space").map(String::as_str) == Some(old_space.as_str())
                    })
                    .map(|t| t.address.clone())
                    .collect();
                for addr in members {
                    let mut map = HashMap::new();
                    map.insert("space".to_string(), Some(new_space.clone()));
                    let _ = client.terminal_label(&addr, map).await;
                }
            }
            if let Some(space_layout) = app.layouts.spaces.remove(&old_space) {
                let _ = store.remove_space(rpc, &old_space).await;
                let _ = store.save_space(rpc, &new_space, &space_layout).await;
                app.layouts.spaces.insert(new_space.clone(), space_layout);
            }
            if app.active_space == old_space {
                app.active_space = new_space;
            }
        }
        AppAction::SaveLayout(space) => {
            if let Some(space_layout) = app.layouts.spaces.get(&space) {
                let _ = store.save_space(rpc, &space, space_layout).await;
            }
        }
        AppAction::RemoveLayout(space) => {
            let _ = store.remove_space(rpc, &space).await;
        }
        AppAction::CopyToClipboard(text) => {
            crate::ui::clipboard::copy_to_clipboard(&text);
            app.set_toast(format!("Copied {} chars", text.chars().count()), now);
        }
        AppAction::SendInput { terminal, bytes } => {
            if let Some(sess) = app.sessions.get_mut(&terminal) {
                let _ = sess.send_input(&bytes);
            }
        }
        AppAction::ResizePane { .. } | AppAction::None => {}
    }
}

fn now_epoch_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
