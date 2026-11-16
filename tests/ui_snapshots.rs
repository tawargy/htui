#[path = "support/fake_hc.rs"]
mod fake_hc;

use std::collections::{BTreeMap, HashMap};

use crossterm::event::{KeyCode, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use herdr_tui::config::Config;
use herdr_tui::hc::types::Terminal;
use herdr_tui::layout::panel::SpaceLayout;
use herdr_tui::layout::tree::{SplitAxis, SplitTree, TermRef};
use herdr_tui::model::tree::{MachineNode, SpaceNode, TerminalEntry};
use herdr_tui::term::session::{AttachSession, SessionState};
use herdr_tui::ui::app::{AppAction, TuiApp};
use herdr_tui::ui::dialogs::{ActiveDialog, ConfirmDialog, DialogOutcome, HelpDialog, SpawnDialog};
use herdr_tui::ui::mouse::{handle_mouse, MouseAction, MouseDragState};
use herdr_tui::ui::panes::render_panes;
use herdr_tui::ui::sidebar::{render_sidebar, SidebarPanel, SidebarState};
use herdr_tui::ui::theme::Theme;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier};
use ratatui::Terminal as RatatuiTerminal;

fn buffer_to_string(buf: &Buffer) -> String {
    let area = buf.area;
    let mut out = String::new();
    for y in area.y..area.y + area.height {
        for x in area.x..area.x + area.width {
            out.push_str(buf.cell((x, y)).map(|c| c.symbol()).unwrap_or(" "));
        }
        out.push('\n');
    }
    out
}

#[test]
fn sidebar_snapshot() {
    let mut t1 = Terminal {
        address: "t1".into(),
        state: "live".into(),
        argv: vec!["claude".into()],
        ..Default::default()
    };
    t1.labels.insert("space".into(), "api".into());

    let mut spaces = BTreeMap::new();
    spaces.insert(
        "api".into(),
        SpaceNode {
            name: "api".into(),
            terminals: vec![TerminalEntry { terminal: t1 }],
        },
    );
    let tree = vec![MachineNode {
        name: "local".into(),
        state: "connected".into(),
        is_local: true,
        spaces,
    }];

    let mut sb = SidebarState::default();
    sb.rebuild(&tree, None);
    assert_eq!(sb.visible_items.len(), 3);
    assert_eq!(sb.agent_items.len(), 1);
    assert_eq!(
        sb.visible_items.first(),
        Some(&herdr_tui::ui::sidebar::SidebarItem::NewSpaceAction)
    );

    let area = Rect::new(0, 0, 32, 14);
    let mut backend = TestBackend::new(32, 14);
    let mut term = RatatuiTerminal::new(backend).unwrap();
    term.draw(|f| {
        render_sidebar(area, f.buffer_mut(), &sb, true);
    })
    .unwrap();
    backend = term.backend().clone();
    let rendered = buffer_to_string(backend.buffer());
    assert!(rendered.contains(" Spaces "));
    assert!(rendered.contains(" Agents "));
    assert!(rendered.contains("local (connected)"));
    assert!(rendered.contains("api (1)"));
    assert!(rendered.contains("claude [api]"));
    assert!(!rendered.contains("[+ New Terminal]"));
}

#[test]
fn sidebar_agents_panel_excludes_plain_shells() {
    let mut t_zsh = Terminal {
        address: "t1".into(),
        state: "live".into(),
        argv: vec!["zsh".into()],
        ..Default::default()
    };
    t_zsh.labels.insert("space".into(), "dev".into());

    let mut t_claude = Terminal {
        address: "t2".into(),
        state: "live".into(),
        argv: vec!["claude".into()],
        ..Default::default()
    };
    t_claude.labels.insert("space".into(), "dev".into());

    let mut spaces = BTreeMap::new();
    spaces.insert(
        "dev".into(),
        SpaceNode {
            name: "dev".into(),
            terminals: vec![
                TerminalEntry { terminal: t_zsh },
                TerminalEntry { terminal: t_claude },
            ],
        },
    );
    let tree = vec![MachineNode {
        name: "local".into(),
        state: "connected".into(),
        is_local: true,
        spaces,
    }];

    let mut sb = SidebarState::default();
    sb.rebuild(&tree, None);
    assert_eq!(sb.agent_items.len(), 1);

    let area = Rect::new(0, 0, 32, 14);
    let mut backend = TestBackend::new(32, 14);
    let mut term = RatatuiTerminal::new(backend).unwrap();
    term.draw(|f| {
        render_sidebar(area, f.buffer_mut(), &sb, true);
    })
    .unwrap();
    backend = term.backend().clone();
    let rendered = buffer_to_string(backend.buffer());
    assert!(rendered.contains("dev (2)"));
    assert!(rendered.contains("claude [dev]"));
    assert!(!rendered.contains("zsh [dev]"));
}

#[test]
fn panes_arabic_snapshot() {
    let (mut tree, p1) = SplitTree::with_single(Some(TermRef {
        address: "t1".into(),
    }));
    let _p2 = tree.split(
        p1,
        SplitAxis::Horizontal,
        Some(TermRef {
            address: "t2".into(),
        }),
    );

    let mut s1 = AttachSession::new("t1".into(), 38, 10);
    s1.state = SessionState::Observing;
    s1.screen.process("السَّلَامُ عَلَيْكُمْ".as_bytes());

    let mut s2 = AttachSession::new("t2".into(), 38, 10);
    s2.state = SessionState::Active;
    s2.screen.process(b"active pane");

    let mut sessions = HashMap::new();
    sessions.insert("t1".into(), s1);
    sessions.insert("t2".into(), s2);

    let area = Rect::new(0, 0, 80, 12);
    let mut buf = Buffer::empty(area);
    let geom = render_panes(area, &mut buf, &tree, Some(p1), &sessions, None);
    assert_eq!(geom.panes.len(), 2);
    assert!(geom.panes[0].takeover_btn.is_some());
    assert!(geom.panes[1].takeover_btn.is_none());

    let rendered = buffer_to_string(&buf);
    assert!(rendered.contains("[observing]"));
    assert!(rendered.contains("[Take over]"));
    assert!(!rendered.contains("[active]"));
    assert!(!rendered.contains(" t2 "));
    assert!(rendered.contains("السَّلَامُ"));
}

#[test]
fn mouse_hit_testing() {
    let (mut tree, p1) = SplitTree::with_single(Some(TermRef {
        address: "t1".into(),
    }));
    let p2 = tree
        .split(
            p1,
            SplitAxis::Horizontal,
            Some(TermRef {
                address: "t2".into(),
            }),
        )
        .unwrap();

    let mut s1 = AttachSession::new("t1".into(), 40, 10);
    s1.state = SessionState::Observing;
    let mut sessions = HashMap::new();
    sessions.insert("t1".into(), s1);

    let sidebar_areas = Some((Rect::new(0, 0, 20, 6), Rect::new(0, 6, 20, 6)));
    let panes_area = Rect::new(20, 0, 80, 12);
    let mut buf = Buffer::empty(Rect::new(0, 0, 100, 12));
    let geom = render_panes(panes_area, &mut buf, &tree, Some(p2), &sessions, None);

    let mut drag = MouseDragState::default();

    // Click sidebar Spaces row 2
    let sb_click = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 5,
        row: 3,
        modifiers: KeyModifiers::NONE,
    };
    assert_eq!(
        handle_mouse(sb_click, sidebar_areas, &geom, &mut drag),
        MouseAction::SelectSidebarRow {
            panel: SidebarPanel::Spaces,
            idx: 2
        }
    );

    // Click [Take over] button on pane 1
    let btn = geom.panes[0].takeover_btn.unwrap();
    let btn_click = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: btn.x + 1,
        row: btn.y,
        modifiers: KeyModifiers::NONE,
    };
    assert_eq!(
        handle_mouse(btn_click, sidebar_areas, &geom, &mut drag),
        MouseAction::TakeOverPane(p1)
    );

    // Drag split divider
    let div = geom.dividers[0];
    let div_down = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: div.line_pos,
        row: 5,
        modifiers: KeyModifiers::NONE,
    };
    assert!(matches!(
        handle_mouse(div_down, sidebar_areas, &geom, &mut drag),
        MouseAction::StartDragDivider(_)
    ));

    let div_drag = MouseEvent {
        kind: MouseEventKind::Drag(MouseButton::Left),
        column: panes_area.x + 60,
        row: 5,
        modifiers: KeyModifiers::NONE,
    };
    match handle_mouse(div_drag, sidebar_areas, &geom, &mut drag) {
        MouseAction::DragDivider { new_ratio, .. } => {
            assert!((new_ratio - 0.75).abs() < 0.02);
        }
        other => panic!("expected DragDivider, got {other:?}"),
    }
}

#[test]
fn dialogs_snapshot() {
    let area = Rect::new(0, 0, 80, 24);
    let mut buf = Buffer::empty(area);

    // Spawn dialog
    let spawn = ActiveDialog::Spawn(SpawnDialog::new("api".into(), vec!["claude".into()]));
    spawn.render(area, &mut buf);
    let s = buffer_to_string(&buf);
    assert!(s.contains("Spawn Agent / Shell"));
    assert!(s.contains("claude"));

    // Confirm kill dialog requires explicit y/n
    let mut kill = ActiveDialog::Confirm(ConfirmDialog::kill("t1".into()));
    assert_eq!(
        kill.handle_key(crossterm::event::KeyEvent::new(
            KeyCode::Char('n'),
            KeyModifiers::NONE
        )),
        DialogOutcome::Close
    );
    assert!(matches!(
        kill.handle_key(crossterm::event::KeyEvent::new(
            KeyCode::Char('y'),
            KeyModifiers::NONE
        )),
        DialogOutcome::SubmitConfirm(_)
    ));

    // Help dialog
    let mut buf2 = Buffer::empty(area);
    let help = ActiveDialog::Help(HelpDialog);
    help.render(area, &mut buf2);
    let hs = buffer_to_string(&buf2);
    assert!(hs.contains("Keybindings Help"));
    assert!(hs.contains("Ctrl+B t"));
    assert!(hs.contains("Ctrl+B x / q"));
    assert!(hs.contains("Close focused pane"));

    // Tall terminal renders all HelpDialog bindings including quit
    let tall_area = Rect::new(0, 0, 80, 40);
    let mut buf3 = Buffer::empty(tall_area);
    help.render(tall_area, &mut buf3);
    let hs_tall = buffer_to_string(&buf3);
    assert!(hs_tall.contains("Ctrl+B x / q"));
    assert!(hs_tall.contains("Close focused pane"));
    assert!(hs_tall.contains("Ctrl+B d"));
    assert!(hs_tall.contains("Detach and quit htui"));
    assert!(!hs_tall.contains("Ctrl+B d / q"));
}

#[test]
fn app_lifecycle() {
    let mut app = TuiApp::new(Config::default(), Some("api".into()));
    let now = std::time::Instant::now();
    let area = Rect::new(0, 0, 100, 30);
    let mut buf = Buffer::empty(area);
    app.render(area, &mut buf, now);

    // Prefix q closes focused pane and does not quit
    let ctrl_b = crossterm::event::KeyEvent::new(KeyCode::Char('b'), KeyModifiers::CONTROL);
    let mut pane_app = TuiApp::new(Config::default(), None);
    let key_pct = crossterm::event::KeyEvent::new(KeyCode::Char('%'), KeyModifiers::NONE);
    let key_q = crossterm::event::KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE);
    assert!(pane_app.handle_key(ctrl_b, now).is_empty());
    pane_app.handle_key(key_pct, now + std::time::Duration::from_millis(20));
    assert_eq!(pane_app.current_tree().pane_ids().len(), 2);
    assert!(pane_app
        .handle_key(ctrl_b, now + std::time::Duration::from_millis(40))
        .is_empty());
    let q_actions = pane_app.handle_key(key_q, now + std::time::Duration::from_millis(60));
    assert!(!pane_app.should_quit);
    assert!(!q_actions.contains(&AppAction::Quit));
    assert_eq!(pane_app.current_tree().pane_ids().len(), 1);

    // Prefix d quits cleanly (UI-9)
    let key_d = crossterm::event::KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE);
    assert!(app.handle_key(ctrl_b, now).is_empty());
    let actions = app.handle_key(key_d, now + std::time::Duration::from_millis(50));
    assert_eq!(actions, vec![AppAction::Quit]);
    assert!(app.should_quit);
}

#[test]
fn new_space_shortcut_and_spawn_unassigned() {
    use herdr_tui::model::tree::UNASSIGNED_SPACE;
    use herdr_tui::ui::sidebar::SidebarItem;

    // 1. SpawnDialog with "(unassigned)" starts with blank space
    let spawn = SpawnDialog::new(UNASSIGNED_SPACE.into(), vec!["claude".into()]);
    assert_eq!(spawn.space, "");

    // 2. Prefix c opens NewSpaceDialog and immediately shows the new active named space (count: 0) while keeping (unassigned) hidden
    let mut app = TuiApp::new(Config::default(), None);
    let now = std::time::Instant::now();
    let ctrl_b = crossterm::event::KeyEvent::new(KeyCode::Char('b'), KeyModifiers::CONTROL);
    let key_c = crossterm::event::KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE);
    app.handle_key(ctrl_b, now);
    app.handle_key(key_c, now + std::time::Duration::from_millis(20));
    assert!(app.dialog.is_some());

    for ch in "backend".chars() {
        app.handle_key(
            crossterm::event::KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE),
            now,
        );
    }
    let actions = app.handle_key(
        crossterm::event::KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        now,
    );
    assert!(actions.contains(&AppAction::SaveLayout("backend".into())));
    assert_eq!(app.active_space, "backend");
    assert!(app.layouts.spaces.contains_key("backend"));
    assert!(app.model.declared_spaces.contains("backend"));
    assert!(app.sidebar.visible_items.iter().any(|item| matches!(
        item,
        SidebarItem::Space { name, count: 0, .. } if name == "backend"
    )));
    assert!(!app.sidebar.visible_items.iter().any(|item| matches!(
        item,
        SidebarItem::Space { name, .. } if name == UNASSIGNED_SPACE
    )));
    assert!(app
        .sidebar
        .visible_items
        .iter()
        .any(|item| matches!(item, SidebarItem::NewSpaceAction)));

    // Adding a terminal in "backend" makes it appear in the sidebar
    let mut t1 = Terminal {
        address: "t1".into(),
        state: "live".into(),
        argv: vec!["claude".into()],
        ..Default::default()
    };
    t1.labels.insert("space".into(), "backend".into());
    app.model.terminals.insert("t1".into(), t1);
    app.refresh_sidebar();
    assert!(app.sidebar.visible_items.iter().any(|item| matches!(
        item,
        SidebarItem::Space { name, count: 1, .. } if name == "backend"
    )));
}

#[test]
fn sidebar_rebuild_hides_empty_and_finished_only_spaces() {
    use herdr_tui::model::tree::UNASSIGNED_SPACE;
    use herdr_tui::ui::sidebar::SidebarItem;

    let mut t_live = Terminal {
        address: "t1".into(),
        state: "live".into(),
        argv: vec!["claude".into()],
        ..Default::default()
    };
    t_live.labels.insert("space".into(), "api".into());

    let mut spaces = BTreeMap::new();
    spaces.insert(
        "empty-declared".into(),
        SpaceNode {
            name: "empty-declared".into(),
            terminals: Vec::new(),
        },
    );
    spaces.insert(
        UNASSIGNED_SPACE.into(),
        SpaceNode {
            name: UNASSIGNED_SPACE.into(),
            terminals: Vec::new(),
        },
    );
    spaces.insert(
        "api".into(),
        SpaceNode {
            name: "api".into(),
            terminals: vec![TerminalEntry { terminal: t_live }],
        },
    );
    let tree = vec![MachineNode {
        name: "local".into(),
        state: "connected".into(),
        is_local: true,
        spaces,
    }];

    let mut sb = SidebarState::default();
    sb.rebuild(&tree, None);
    assert!(!sb
        .visible_items
        .iter()
        .any(|item| matches!(item, SidebarItem::Space { count: 0, .. })));
    assert!(!sb.visible_items.iter().any(|item| matches!(
        item,
        SidebarItem::Space { name, .. } if name == "empty-declared" || name == UNASSIGNED_SPACE
    )));
    assert!(sb.visible_items.iter().any(|item| matches!(
        item,
        SidebarItem::Space { name, count: 1, .. } if name == "api"
    )));

    // Active UNASSIGNED_SPACE still hides empty UNASSIGNED_SPACE and inactive empty spaces
    sb.rebuild(&tree, Some(UNASSIGNED_SPACE));
    assert!(!sb.visible_items.iter().any(|item| matches!(
        item,
        SidebarItem::Space { name, .. } if name == "empty-declared" || name == UNASSIGNED_SPACE
    )));

    // Active named empty space appears with count: 0 while empty UNASSIGNED_SPACE stays hidden
    sb.rebuild(&tree, Some("empty-declared"));
    assert!(sb.visible_items.iter().any(|item| matches!(
        item,
        SidebarItem::Space { name, count: 0, .. } if name == "empty-declared"
    )));
    assert!(!sb.visible_items.iter().any(|item| matches!(
        item,
        SidebarItem::Space { name, .. } if name == UNASSIGNED_SPACE
    )));

    // When show_finished == false hides an inactive space's only terminal, SidebarState::rebuild hides that space
    let mut cfg = Config::default();
    cfg.ui.show_finished = false;
    let mut app = TuiApp::new(cfg, None);
    let mut t_fin = Terminal {
        address: "t-fin".into(),
        state: "finished".into(),
        argv: vec!["sh".into()],
        ..Default::default()
    };
    t_fin.labels.insert("space".into(), "jobs".into());
    app.model.terminals.insert("t-fin".into(), t_fin);
    app.model.declared_spaces.insert("jobs".into());
    app.refresh_sidebar();
    assert!(!app.sidebar.visible_items.iter().any(|item| matches!(
        item,
        SidebarItem::Space { name, .. } if name == "jobs"
    )));

    app.config.ui.show_finished = true;
    app.refresh_sidebar();
    assert!(app.sidebar.visible_items.iter().any(|item| matches!(
        item,
        SidebarItem::Space { name, count: 1, .. } if name == "jobs"
    )));

    // When "jobs" is the active space and show_finished == false, it appears as count: 0,
    // and hides immediately when switching away to another space.
    app.config.ui.show_finished = false;
    app.switch_space("jobs".into());
    app.refresh_sidebar();
    assert!(app.sidebar.visible_items.iter().any(|item| matches!(
        item,
        SidebarItem::Space { name, count: 0, .. } if name == "jobs"
    )));
    app.switch_space("api".into());
    app.refresh_sidebar();
    assert!(!app.sidebar.visible_items.iter().any(|item| matches!(
        item,
        SidebarItem::Space { name, .. } if name == "jobs"
    )));
}

#[test]
fn empty_pane_spawn_button_click() {
    let mut app = TuiApp::new(Config::default(), Some("api".into()));
    app.sidebar_focused = true;
    let now = std::time::Instant::now();
    let area = Rect::new(0, 0, 100, 30);
    let mut buf = Buffer::empty(area);
    app.render(area, &mut buf, now);

    let pane_hitbox = app.last_geom.panes.first().expect("pane hitbox");
    let pane_id = pane_hitbox.pane_id;
    let btn = pane_hitbox.spawn_btn.expect("spawn_btn on empty pane");
    let click = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: btn.x + 1,
        row: btn.y,
        modifiers: KeyModifiers::NONE,
    };
    app.handle_mouse_event(click, now);
    assert!(!app.sidebar_focused);
    assert_eq!(app.focused_pane, Some(pane_id));
    assert!(matches!(app.dialog, Some(ActiveDialog::Spawn(_))));
}

#[test]
fn empty_pane_spawn_button_style() {
    let mut app = TuiApp::new(Config::default(), Some("api".into()));
    let now = std::time::Instant::now();
    let area = Rect::new(0, 0, 100, 30);
    let mut buf = Buffer::empty(area);
    app.render(area, &mut buf, now);

    let pane_hitbox = app.last_geom.panes.first().expect("pane hitbox");
    let btn = pane_hitbox.spawn_btn.expect("spawn_btn on empty pane");
    for x in btn.x..btn.x + btn.width {
        let cell = buf.cell((x, btn.y)).expect("spawn button cell");
        assert_eq!(cell.bg, Theme::SUMI_INK3);
        assert_eq!(cell.fg, Theme::OLD_WHITE);
        assert!(!cell.modifier.contains(Modifier::BOLD));
    }
}

#[test]
fn sidebar_new_space_at_index_zero_and_keyboard_nav() {
    use herdr_tui::ui::sidebar::SidebarItem;

    let mut app = TuiApp::new(Config::default(), None);
    let mut t1 = Terminal {
        address: "t1".into(),
        state: "live".into(),
        argv: vec!["claude".into()],
        ..Default::default()
    };
    t1.labels.insert("space".into(), "api".into());
    app.model.terminals.insert("t1".into(), t1);
    app.model.declared_spaces.insert("api".into());
    app.refresh_sidebar();

    // 1. [+ New Space] must be at index 0
    assert_eq!(
        app.sidebar.visible_items.first(),
        Some(&SidebarItem::NewSpaceAction)
    );

    let now = std::time::Instant::now();
    // 2. On an empty pane, Down focuses sidebar and moves selection
    app.sidebar_focused = false;
    app.sidebar.selected = 0;
    app.handle_key(
        crossterm::event::KeyEvent::new(KeyCode::Down, KeyModifiers::NONE),
        now,
    );
    assert!(app.sidebar_focused);
    assert_eq!(app.sidebar.selected, 1);

    // Move down to the "api" Space row and press Enter -> switches space without collapsing
    while !matches!(
        app.sidebar.selected_item(),
        Some(SidebarItem::Space { name, .. }) if name == "api"
    ) {
        let prev = app.sidebar.selected;
        app.handle_key(
            crossterm::event::KeyEvent::new(KeyCode::Down, KeyModifiers::NONE),
            now,
        );
        assert_ne!(app.sidebar.selected, prev, "api space not found in sidebar");
    }
    app.handle_key(
        crossterm::event::KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        now,
    );
    assert_eq!(app.active_space, "api");
    assert!(app.sidebar.collapsed_spaces.is_empty());

    // Move down to "t1" Terminal row in Agents panel and press Enter -> attaches & focuses pane
    app.handle_key(
        crossterm::event::KeyEvent::new(KeyCode::Down, KeyModifiers::NONE),
        now,
    );
    assert_eq!(app.sidebar.active_panel, SidebarPanel::Agents);
    assert!(matches!(
        app.sidebar.selected_item(),
        Some(SidebarItem::Terminal { address, .. }) if address == "t1"
    ));
    let actions = app.handle_key(
        crossterm::event::KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        now,
    );
    assert!(!app.sidebar_focused);
    assert!(actions.iter().any(|a| matches!(
        a,
        AppAction::AttachTerminal { terminal, .. } if terminal == "t1"
    )));
}

#[test]
fn sidebar_new_space_action_styling_unselected_and_selected() {
    use herdr_tui::ui::sidebar::SidebarItem;

    let mut sb = SidebarState::default();
    sb.visible_items.push(SidebarItem::NewSpaceAction);
    sb.visible_items.push(SidebarItem::Machine {
        name: "local".into(),
        state: "connected".into(),
    });

    let area = Rect::new(0, 0, 32, 14);

    // 1. Unselected: sb.selected = 1
    sb.active_panel = SidebarPanel::Spaces;
    sb.selected = 1;

    let mut buf = Buffer::empty(area);
    render_sidebar(area, &mut buf, &sb, true);

    // Row 0 of spaces panel inner is at y = 1, x = 1 (border at x = 0, y = 0)
    // Indent at x = 1, 2 must have Color::Reset bg
    let indent0 = buf.cell((1, 1)).expect("indent cell 0");
    let indent1 = buf.cell((2, 1)).expect("indent cell 1");
    assert_eq!(indent0.symbol(), " ");
    assert_eq!(indent0.bg, Color::Reset);
    assert_eq!(indent1.symbol(), " ");
    assert_eq!(indent1.bg, Color::Reset);

    // Button "[+ New Space]" at x = 3..16 (13 chars)
    let label = "[+ New Space]";
    for (i, ch) in label.chars().enumerate() {
        let x = 3 + i as u16;
        let cell = buf.cell((x, 1)).expect("new space button cell");
        assert_eq!(cell.symbol(), &ch.to_string());
        assert_eq!(cell.bg, Theme::SUMI_INK3);
        assert_eq!(cell.fg, Theme::OLD_WHITE);
        assert!(!cell.modifier.contains(Modifier::BOLD));
    }

    // 2. Selected: sb.selected = 0
    sb.selected = 0;
    let mut buf_sel = Buffer::empty(area);
    render_sidebar(area, &mut buf_sel, &sb, true);

    // Entire row must have WAVE_BLUE2 bg, FUJI_WHITE fg, BOLD modifier
    for x in 1..31 {
        let cell = buf_sel.cell((x, 1)).expect("selected row cell");
        assert_eq!(cell.bg, Theme::WAVE_BLUE2, "cell at x={x} should have WAVE_BLUE2 bg");
        assert_eq!(cell.fg, Theme::FUJI_WHITE, "cell at x={x} should have FUJI_WHITE fg");
        assert!(cell.modifier.contains(Modifier::BOLD), "cell at x={x} should have BOLD modifier");
    }
}

#[test]
fn close_last_terminal_removes_terminal_and_empty_space() {
    use herdr_tui::model::tree::UNASSIGNED_SPACE;
    use herdr_tui::ui::sidebar::SidebarItem;

    let mut app = TuiApp::new(Config::default(), Some("api".into()));
    let mut t1 = Terminal {
        address: "t1".into(),
        state: "live".into(),
        argv: vec!["claude".into()],
        reported: true,
        ..Default::default()
    };
    t1.labels.insert("space".into(), "api".into());
    app.model.terminals.insert("t1".into(), t1);
    app.model.declared_spaces.insert("api".into());
    app.sessions
        .insert("t1".into(), AttachSession::new("t1".into(), 80, 24));
    let pid = app.focused_pane.expect("focused pane");
    app.current_tree_mut().set_terminal(
        pid,
        Some(TermRef {
            address: "t1".into(),
        }),
    );
    app.refresh_sidebar();

    let now = std::time::Instant::now();
    let ctrl_b = crossterm::event::KeyEvent::new(KeyCode::Char('b'), KeyModifiers::CONTROL);
    let key_x = crossterm::event::KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE);
    app.handle_key(ctrl_b, now);
    let actions = app.handle_key(key_x, now + std::time::Duration::from_millis(20));

    assert_eq!(
        actions,
        vec![
            AppAction::Kill("t1".into()),
            AppAction::Remove("t1".into()),
            AppAction::RemoveLayout("api".into()),
        ]
    );
    assert!(!app.model.terminals.contains_key("t1"));
    assert!(!app.sessions.contains_key("t1"));
    assert!(!app.sidebar.agent_items.iter().any(|i| matches!(
        i,
        SidebarItem::Terminal { address, .. } if address == "t1"
    )));
    assert!(!app.sidebar.visible_items.iter().any(|i| matches!(
        i,
        SidebarItem::Space { name, .. } if name == UNASSIGNED_SPACE
    )));
    assert!(!app.model.declared_spaces.contains("api"));
    assert!(!app.layouts.spaces.contains_key("api"));
    assert_ne!(app.active_space, "api");
}

#[test]
fn close_pane_finished_terminal_emits_remove_without_kill() {
    use herdr_tui::model::tree::UNASSIGNED_SPACE;
    use herdr_tui::ui::sidebar::SidebarItem;

    let mut app = TuiApp::new(Config::default(), Some("api".into()));
    let mut t1 = Terminal {
        address: "t1".into(),
        state: "finished".into(),
        argv: vec!["claude".into()],
        reported: true,
        ..Default::default()
    };
    t1.labels.insert("space".into(), "api".into());
    app.model.terminals.insert("t1".into(), t1);
    app.model.declared_spaces.insert("api".into());
    app.sessions
        .insert("t1".into(), AttachSession::new("t1".into(), 80, 24));
    let pid = app.focused_pane.expect("focused pane");
    app.current_tree_mut().set_terminal(
        pid,
        Some(TermRef {
            address: "t1".into(),
        }),
    );
    app.refresh_sidebar();

    let now = std::time::Instant::now();
    let ctrl_b = crossterm::event::KeyEvent::new(KeyCode::Char('b'), KeyModifiers::CONTROL);
    let key_x = crossterm::event::KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE);
    app.handle_key(ctrl_b, now);
    let actions = app.handle_key(key_x, now + std::time::Duration::from_millis(20));

    assert_eq!(
        actions,
        vec![
            AppAction::Remove("t1".into()),
            AppAction::RemoveLayout("api".into()),
        ]
    );
    assert!(!actions.iter().any(|a| matches!(a, AppAction::Kill(_))));
    assert!(!app.model.terminals.contains_key("t1"));
    assert!(!app.sessions.contains_key("t1"));
    assert!(!app.sidebar.agent_items.iter().any(|i| matches!(
        i,
        SidebarItem::Terminal { address, .. } if address == "t1"
    )));
    assert!(!app.sidebar.visible_items.iter().any(|i| matches!(
        i,
        SidebarItem::Space { name, .. } if name == UNASSIGNED_SPACE
    )));
}

#[test]
fn close_pane_in_multi_pane_split_kills_closed_terminal_and_saves_layout() {
    use herdr_tui::model::tree::UNASSIGNED_SPACE;
    use herdr_tui::ui::sidebar::SidebarItem;

    let mut app = TuiApp::new(Config::default(), Some("api".into()));
    let mut t1 = Terminal {
        address: "t1".into(),
        state: "live".into(),
        argv: vec!["claude".into()],
        reported: true,
        ..Default::default()
    };
    t1.labels.insert("space".into(), "api".into());
    let mut t2 = Terminal {
        address: "t2".into(),
        state: "live".into(),
        argv: vec!["sh".into()],
        reported: true,
        ..Default::default()
    };
    t2.labels.insert("space".into(), "api".into());
    app.model.terminals.insert("t1".into(), t1);
    app.model.terminals.insert("t2".into(), t2);
    app.model.declared_spaces.insert("api".into());
    app.sessions
        .insert("t1".into(), AttachSession::new("t1".into(), 80, 24));
    app.sessions
        .insert("t2".into(), AttachSession::new("t2".into(), 80, 24));

    let p1 = app.focused_pane.expect("focused pane");
    app.current_tree_mut().set_terminal(
        p1,
        Some(TermRef {
            address: "t1".into(),
        }),
    );
    let p2 = app
        .current_tree_mut()
        .split(
            p1,
            SplitAxis::Horizontal,
            Some(TermRef {
                address: "t2".into(),
            }),
        )
        .expect("split pane");
    app.focused_pane = Some(p1);
    app.refresh_sidebar();

    let now = std::time::Instant::now();
    let ctrl_b = crossterm::event::KeyEvent::new(KeyCode::Char('b'), KeyModifiers::CONTROL);
    let key_x = crossterm::event::KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE);
    app.handle_key(ctrl_b, now);
    let actions = app.handle_key(key_x, now + std::time::Duration::from_millis(20));

    assert_eq!(
        actions,
        vec![
            AppAction::Kill("t1".into()),
            AppAction::Remove("t1".into()),
            AppAction::SaveLayout("api".into()),
        ]
    );
    assert!(!app.model.terminals.contains_key("t1"));
    assert!(!app.sessions.contains_key("t1"));
    assert!(app.model.terminals.contains_key("t2"));
    assert!(app.sessions.contains_key("t2"));
    assert_eq!(app.current_tree().pane_ids(), vec![p2]);
    assert_eq!(app.focused_pane, Some(p2));
    assert!(!app.sidebar.agent_items.iter().any(|i| matches!(
        i,
        SidebarItem::Terminal { address, .. } if address == "t1"
    )));
    assert!(app.sidebar.agent_items.iter().any(|i| matches!(
        i,
        SidebarItem::Terminal { address, .. } if address == "t2"
    )));
    assert!(app.sidebar.visible_items.iter().any(|i| matches!(
        i,
        SidebarItem::Space { name, count: 1, .. } if name == "api"
    )));
    assert!(!app.sidebar.visible_items.iter().any(|i| matches!(
        i,
        SidebarItem::Space { name, .. } if name == UNASSIGNED_SPACE
    )));
}

#[test]
fn close_pane_named_space_empty_last_pane_kills_unattached_terminals_and_closes_space() {
    let mut app = TuiApp::new(Config::default(), Some("api".into()));

    let mut t_live = Terminal {
        address: "t-live-2".into(),
        state: "live".into(),
        argv: vec!["claude".into()],
        reported: true,
        ..Default::default()
    };
    t_live.labels.insert("space".into(), "api".into());

    let mut t_fin = Terminal {
        address: "t-fin-1".into(),
        state: "finished".into(),
        argv: vec!["sh".into()],
        reported: true,
        ..Default::default()
    };
    t_fin.labels.insert("space".into(), "api".into());

    let mut t_web = Terminal {
        address: "t-web".into(),
        state: "live".into(),
        argv: vec!["zsh".into()],
        reported: true,
        ..Default::default()
    };
    t_web.labels.insert("space".into(), "web".into());

    let mut t_zebra = Terminal {
        address: "t-zebra".into(),
        state: "live".into(),
        argv: vec!["zsh".into()],
        reported: true,
        ..Default::default()
    };
    t_zebra.labels.insert("space".into(), "zebra".into());

    app.model.terminals.insert("t-live-2".into(), t_live);
    app.model.terminals.insert("t-fin-1".into(), t_fin);
    app.model.terminals.insert("t-web".into(), t_web);
    app.model.terminals.insert("t-zebra".into(), t_zebra);
    app.model.declared_spaces.insert("api".into());
    app.model.declared_spaces.insert("web".into());
    app.model.declared_spaces.insert("zebra".into());
    app.sessions.insert(
        "t-live-2".into(),
        AttachSession::new("t-live-2".into(), 80, 24),
    );
    app.sessions.insert(
        "t-fin-1".into(),
        AttachSession::new("t-fin-1".into(), 80, 24),
    );

    let (web_space, _, _) = SpaceLayout::with_single(Some(TermRef {
        address: "t-web".into(),
    }));
    let (zebra_space, _, _) = SpaceLayout::with_single(Some(TermRef {
        address: "t-zebra".into(),
    }));
    app.layouts.spaces.insert("web".into(), web_space);
    app.layouts.spaces.insert("zebra".into(), zebra_space);
    app.refresh_sidebar();

    // "api" currently has an (empty) single pane, plus unattached t-fin-1 and t-live-2
    assert!(app
        .current_tree()
        .find_terminal(app.focused_pane.unwrap())
        .is_none());

    let now = std::time::Instant::now();
    let ctrl_b = crossterm::event::KeyEvent::new(KeyCode::Char('b'), KeyModifiers::CONTROL);
    let key_q = crossterm::event::KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE);
    app.handle_key(ctrl_b, now);
    let actions = app.handle_key(key_q, now + std::time::Duration::from_millis(20));

    assert_eq!(
        actions,
        vec![
            AppAction::Remove("t-fin-1".into()),
            AppAction::Kill("t-live-2".into()),
            AppAction::Remove("t-live-2".into()),
            AppAction::RemoveLayout("api".into()),
        ]
    );
    assert!(!app.model.terminals.contains_key("t-fin-1"));
    assert!(!app.model.terminals.contains_key("t-live-2"));
    assert!(!app.sessions.contains_key("t-fin-1"));
    assert!(!app.sessions.contains_key("t-live-2"));
    assert!(app.model.terminals.contains_key("t-web"));
    assert!(!app.layouts.spaces.contains_key("api"));
    assert!(!app.model.declared_spaces.contains("api"));
    assert_eq!(app.active_space, "web");
    assert!(app.toast.is_none(), "unexpected toast: {:?}", app.toast);
}

#[test]
fn close_pane_named_space_last_pane_with_attached_and_unattached_closes_all_and_removes_space() {
    use herdr_tui::model::tree::UNASSIGNED_SPACE;

    let mut app = TuiApp::new(Config::default(), Some("api".into()));

    let mut t1 = Terminal {
        address: "t1".into(),
        state: "live".into(),
        argv: vec!["claude".into()],
        reported: true,
        ..Default::default()
    };
    t1.labels.insert("space".into(), "api".into());

    let mut t0 = Terminal {
        address: "t0".into(),
        state: "finished".into(),
        argv: vec!["sh".into()],
        reported: true,
        ..Default::default()
    };
    t0.labels.insert("space".into(), "api".into());

    let mut t2 = Terminal {
        address: "t2".into(),
        state: "live".into(),
        argv: vec!["zsh".into()],
        reported: true,
        ..Default::default()
    };
    t2.labels.insert("space".into(), "api".into());

    app.model.terminals.insert("t1".into(), t1);
    app.model.terminals.insert("t0".into(), t0);
    app.model.terminals.insert("t2".into(), t2);
    app.model.declared_spaces.insert("api".into());
    app.sessions
        .insert("t1".into(), AttachSession::new("t1".into(), 80, 24));
    app.sessions
        .insert("t0".into(), AttachSession::new("t0".into(), 80, 24));
    app.sessions
        .insert("t2".into(), AttachSession::new("t2".into(), 80, 24));

    let p1 = app.focused_pane.expect("focused pane");
    app.current_tree_mut().set_terminal(
        p1,
        Some(TermRef {
            address: "t1".into(),
        }),
    );
    app.refresh_sidebar();

    let now = std::time::Instant::now();
    let ctrl_b = crossterm::event::KeyEvent::new(KeyCode::Char('b'), KeyModifiers::CONTROL);
    let key_x = crossterm::event::KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE);
    app.handle_key(ctrl_b, now);
    let actions = app.handle_key(key_x, now + std::time::Duration::from_millis(20));

    assert_eq!(
        actions,
        vec![
            AppAction::Kill("t1".into()),
            AppAction::Remove("t1".into()),
            AppAction::Remove("t0".into()),
            AppAction::Kill("t2".into()),
            AppAction::Remove("t2".into()),
            AppAction::RemoveLayout("api".into()),
        ]
    );
    assert!(!app.model.terminals.contains_key("t0"));
    assert!(!app.model.terminals.contains_key("t1"));
    assert!(!app.model.terminals.contains_key("t2"));
    assert!(!app.sessions.contains_key("t0"));
    assert!(!app.sessions.contains_key("t1"));
    assert!(!app.sessions.contains_key("t2"));
    assert!(!app.layouts.spaces.contains_key("api"));
    assert!(!app.model.declared_spaces.contains("api"));
    assert_eq!(app.active_space, UNASSIGNED_SPACE);
    assert!(app.toast.is_none(), "unexpected toast: {:?}", app.toast);
}

#[test]
fn close_pane_unassigned_empty_last_pane_kills_unattached_and_switches_to_named_space() {
    use herdr_tui::model::tree::UNASSIGNED_SPACE;
    use herdr_tui::ui::sidebar::SidebarItem;

    let mut app = TuiApp::new(Config::default(), None);
    assert_eq!(app.active_space, UNASSIGNED_SPACE);

    let t_live = Terminal {
        address: "t-u2".into(),
        state: "live".into(),
        argv: vec!["claude".into()],
        reported: true,
        ..Default::default()
    };
    let t_fin = Terminal {
        address: "t-u1".into(),
        state: "finished".into(),
        argv: vec!["sh".into()],
        reported: true,
        ..Default::default()
    };
    let mut t_api = Terminal {
        address: "t-api".into(),
        state: "live".into(),
        argv: vec!["zsh".into()],
        reported: true,
        ..Default::default()
    };
    t_api.labels.insert("space".into(), "api".into());

    app.model.terminals.insert("t-u2".into(), t_live);
    app.model.terminals.insert("t-u1".into(), t_fin);
    app.model.terminals.insert("t-api".into(), t_api);
    app.model.declared_spaces.insert("api".into());
    app.sessions
        .insert("t-u2".into(), AttachSession::new("t-u2".into(), 80, 24));
    app.sessions
        .insert("t-u1".into(), AttachSession::new("t-u1".into(), 80, 24));

    let (api_space, _, _) = SpaceLayout::with_single(Some(TermRef {
        address: "t-api".into(),
    }));
    app.layouts.spaces.insert("api".into(), api_space);
    app.refresh_sidebar();

    let now = std::time::Instant::now();
    let ctrl_b = crossterm::event::KeyEvent::new(KeyCode::Char('b'), KeyModifiers::CONTROL);
    let key_x = crossterm::event::KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE);
    app.handle_key(ctrl_b, now);
    let actions = app.handle_key(key_x, now + std::time::Duration::from_millis(20));

    assert_eq!(
        actions,
        vec![
            AppAction::Remove("t-u1".into()),
            AppAction::Kill("t-u2".into()),
            AppAction::Remove("t-u2".into()),
            AppAction::RemoveLayout(UNASSIGNED_SPACE.into()),
        ]
    );
    assert!(!app.model.terminals.contains_key("t-u1"));
    assert!(!app.model.terminals.contains_key("t-u2"));
    assert!(!app.sessions.contains_key("t-u1"));
    assert!(!app.sessions.contains_key("t-u2"));
    assert!(app.model.terminals.contains_key("t-api"));
    assert!(!app.layouts.spaces.contains_key(UNASSIGNED_SPACE));
    assert_eq!(app.active_space, "api");
    assert!(app.toast.is_none(), "unexpected toast: {:?}", app.toast);
    assert!(!app.sidebar.visible_items.iter().any(|i| matches!(
        i,
        SidebarItem::Space { name, .. } if name == UNASSIGNED_SPACE
    )));
}

#[test]
fn close_pane_unassigned_empty_last_pane_without_terminals_switches_to_named_space() {
    use herdr_tui::model::tree::UNASSIGNED_SPACE;
    use herdr_tui::ui::sidebar::SidebarItem;

    let mut app = TuiApp::new(Config::default(), None);
    let mut t_api = Terminal {
        address: "t-api".into(),
        state: "live".into(),
        argv: vec!["zsh".into()],
        reported: true,
        ..Default::default()
    };
    t_api.labels.insert("space".into(), "api".into());
    app.model.terminals.insert("t-api".into(), t_api);
    app.model.declared_spaces.insert("api".into());
    let (api_space, _, _) = SpaceLayout::with_single(Some(TermRef {
        address: "t-api".into(),
    }));
    app.layouts.spaces.insert("api".into(), api_space);
    app.refresh_sidebar();

    let now = std::time::Instant::now();
    let ctrl_b = crossterm::event::KeyEvent::new(KeyCode::Char('b'), KeyModifiers::CONTROL);
    let key_q = crossterm::event::KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE);
    app.handle_key(ctrl_b, now);
    let actions = app.handle_key(key_q, now + std::time::Duration::from_millis(20));

    assert_eq!(
        actions,
        vec![AppAction::RemoveLayout(UNASSIGNED_SPACE.into())]
    );
    assert!(!app.layouts.spaces.contains_key(UNASSIGNED_SPACE));
    assert_eq!(app.active_space, "api");
    assert!(app.toast.is_none(), "unexpected toast: {:?}", app.toast);
    assert!(!app.sidebar.visible_items.iter().any(|i| matches!(
        i,
        SidebarItem::Space { name, .. } if name == UNASSIGNED_SPACE
    )));
}

#[test]
fn close_pane_unassigned_empty_last_pane_no_other_spaces_handles_unattached_and_empty_workspace() {
    use herdr_tui::model::tree::UNASSIGNED_SPACE;
    use herdr_tui::ui::sidebar::SidebarItem;

    let now = std::time::Instant::now();
    let ctrl_b = crossterm::event::KeyEvent::new(KeyCode::Char('b'), KeyModifiers::CONTROL);
    let key_q = crossterm::event::KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE);

    // 1. Empty workspace (0 terminals, 0 other spaces) -> no layout action, informational toast, no (unassigned) in sidebar
    let mut empty_app = TuiApp::new(Config::default(), None);
    empty_app.refresh_sidebar();
    empty_app.handle_key(ctrl_b, now);
    let empty_actions = empty_app.handle_key(key_q, now + std::time::Duration::from_millis(20));
    assert!(empty_actions.is_empty());
    assert_eq!(empty_app.active_space, UNASSIGNED_SPACE);
    assert_eq!(
        empty_app.active_toast(now),
        Some("Workspace is already empty")
    );
    assert!(!empty_app.sidebar.visible_items.iter().any(|i| matches!(
        i,
        SidebarItem::Space { name, .. } if name == UNASSIGNED_SPACE
    )));

    // 2. Unattached unassigned terminal present and 0 other spaces -> kills/removes it and emits RemoveLayout(UNASSIGNED_SPACE) with no toast
    let mut app_with_unattached = TuiApp::new(Config::default(), None);
    app_with_unattached.model.terminals.insert(
        "t-u1".into(),
        Terminal {
            address: "t-u1".into(),
            state: "live".into(),
            argv: vec!["sh".into()],
            reported: true,
            ..Default::default()
        },
    );
    app_with_unattached
        .sessions
        .insert("t-u1".into(), AttachSession::new("t-u1".into(), 80, 24));
    app_with_unattached.refresh_sidebar();
    app_with_unattached.handle_key(ctrl_b, now);
    let unattached_actions =
        app_with_unattached.handle_key(key_q, now + std::time::Duration::from_millis(20));
    assert_eq!(
        unattached_actions,
        vec![
            AppAction::Kill("t-u1".into()),
            AppAction::Remove("t-u1".into()),
            AppAction::RemoveLayout(UNASSIGNED_SPACE.into()),
        ]
    );
    assert!(!app_with_unattached.model.terminals.contains_key("t-u1"));
    assert!(!app_with_unattached.sessions.contains_key("t-u1"));
    assert_eq!(app_with_unattached.active_space, UNASSIGNED_SPACE);
    assert!(app_with_unattached.toast.is_none());
    assert!(!app_with_unattached
        .sidebar
        .visible_items
        .iter()
        .any(|i| matches!(
            i,
            SidebarItem::Space { name, .. } if name == UNASSIGNED_SPACE
        )));
}

#[tokio::test]
async fn finished_event_removes_terminal_session_and_empty_space() {
    use herdr_tui::hc::types::EventPayload;
    use herdr_tui::layout::store::LayoutStore;
    use herdr_tui::ui::event_loop::handle_core_event;
    use herdr_tui::ui::sidebar::SidebarItem;

    let mut app = TuiApp::new(Config::default(), Some("api".into()));
    let mut t1 = Terminal {
        address: "t1".into(),
        state: "running".into(),
        argv: vec!["sh".into()],
        ..Default::default()
    };
    t1.labels.insert("space".into(), "api".into());
    app.model.terminals.insert("t1".into(), t1);
    app.model.declared_spaces.insert("api".into());
    app.sessions
        .insert("t1".into(), AttachSession::new("t1".into(), 80, 24));
    let pid = app.focused_pane.expect("focused pane");
    app.current_tree_mut().set_terminal(
        pid,
        Some(TermRef {
            address: "t1".into(),
        }),
    );
    app.refresh_sidebar();

    let tmp = tempfile::tempdir().expect("tempdir");
    let store = LayoutStore::new(tmp.path().join("layouts.toml"), &[]);

    let ev = EventPayload {
        seq: 1,
        terminal: Some("t1".into()),
        event_type: "finished".into(),
        data: serde_json::json!({ "exit_code": 0 }),
    };
    handle_core_event(ev, &mut app, None, &store).await;

    assert!(!app.model.terminals.contains_key("t1"));
    assert!(!app.sessions.contains_key("t1"));
    assert!(!app.model.declared_spaces.contains("api"));
    assert!(!app.layouts.spaces.contains_key("api"));
    assert!(!app
        .sidebar
        .visible_items
        .iter()
        .any(|i| matches!(i, SidebarItem::Terminal { address, .. } if address == "t1")));
    assert!(!app
        .sidebar
        .visible_items
        .iter()
        .any(|i| matches!(i, SidebarItem::Space { name, .. } if name == "api")));
}

#[test]
fn focused_cursor_position_tracks_pane_and_hides_on_sidebar_or_dialog() {
    let mut app = TuiApp::new(Config::default(), Some("api".into()));
    let pid = app.focused_pane.expect("focused pane");
    app.current_tree_mut().set_terminal(
        pid,
        Some(TermRef {
            address: "t1".into(),
        }),
    );

    let mut sess = AttachSession::new("t1".into(), 60, 20);
    sess.state = SessionState::Active;
    sess.screen.process(b"hello");
    app.sessions.insert("t1".into(), sess);

    let now = std::time::Instant::now();
    let area = Rect::new(0, 0, 100, 30);
    let mut buf = Buffer::empty(area);
    app.render(area, &mut buf, now);

    let hitbox = app.last_geom.panes.first().expect("pane hitbox");
    let (col, row, visible) = app.sessions.get("t1").unwrap().screen.cursor();
    assert!(visible);
    assert_eq!(col, 5);
    assert_eq!(row, 0);
    assert_eq!(
        app.focused_cursor_position(),
        Some((hitbox.rect.x + 1 + col, hitbox.rect.y + 1 + row))
    );

    // Hidden when sidebar is focused
    app.sidebar_focused = true;
    assert_eq!(app.focused_cursor_position(), None);
    app.sidebar_focused = false;

    // Hidden when dialog is open
    app.dialog = Some(ActiveDialog::Help(HelpDialog));
    assert_eq!(app.focused_cursor_position(), None);
}

#[test]
fn tab_bar_render_click_and_cursor_alignment() {
    use herdr_tui::ui::tabs::TabHitbox;

    let mut app = TuiApp::new(Config::default(), Some("api".into()));
    let mut t1 = Terminal {
        address: "t1".into(),
        state: "running".into(),
        argv: vec!["zsh".into()],
        created_ms: 1,
        ..Default::default()
    };
    t1.labels.insert("space".into(), "api".into());
    let mut t2 = Terminal {
        address: "t2".into(),
        state: "running".into(),
        argv: vec!["bash".into()],
        created_ms: 2,
        ..Default::default()
    };
    t2.labels.insert("space".into(), "api".into());
    app.model.terminals.insert("t1".into(), t1);
    app.model.terminals.insert("t2".into(), t2);
    app.model.declared_spaces.insert("api".into());

    let pid = app.focused_pane.expect("focused pane");
    app.current_tree_mut().set_terminal(
        pid,
        Some(TermRef {
            address: "t1".into(),
        }),
    );

    let mut sess1 = AttachSession::new("t1".into(), 60, 20);
    sess1.state = SessionState::Active;
    sess1.screen.process(b"hi");
    app.sessions.insert("t1".into(), sess1);

    let now = std::time::Instant::now();
    let area = Rect::new(0, 0, 120, 30);
    let mut buf = Buffer::empty(area);
    app.render(area, &mut buf, now);

    // 1. Tab bar rendered in row 0, panes_area starts at row 1 (below 1-row tab bar)
    assert_eq!(app.last_panes_area.y, 1);
    let rendered = buffer_to_string(&buf);
    assert!(rendered.contains("zsh"));
    assert!(rendered.contains("bash"));
    assert!(rendered.contains("[+ New Terminal]"));

    // Cursor alignment below the 1-row tab bar
    let hitbox = app.last_geom.panes.first().expect("pane hitbox");
    assert_eq!(hitbox.rect.y, 1);
    assert_eq!(
        app.focused_cursor_position(),
        Some((hitbox.rect.x + 1 + 2, hitbox.rect.y + 1))
    );

    // 2. Clicking t2 tab switches focused pane to t2 and emits AttachTerminal
    let t2_rect = app
        .last_tabs_geom
        .iter()
        .find_map(|h| match h {
            TabHitbox::Terminal { address, rect } if address == "t2" => Some(*rect),
            _ => None,
        })
        .expect("t2 tab hitbox");
    let click_t2 = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: t2_rect.x + 1,
        row: t2_rect.y,
        modifiers: KeyModifiers::NONE,
    };
    let actions = app.handle_mouse_event(click_t2, now);
    assert!(!app.sidebar_focused);
    assert!(actions.iter().any(|a| matches!(
        a,
        AppAction::AttachTerminal { terminal, .. } if terminal == "t2"
    )));

    // 3. Clicking [+ New Terminal] tab button opens SpawnDialog
    let new_rect = app
        .last_tabs_geom
        .iter()
        .find_map(|h| match h {
            TabHitbox::NewTerminal(rect) => Some(*rect),
            _ => None,
        })
        .expect("NewTerminal tab hitbox");
    let click_new = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: new_rect.x + 1,
        row: new_rect.y,
        modifiers: KeyModifiers::NONE,
    };
    app.handle_mouse_event(click_new, now);
    assert!(matches!(app.dialog, Some(ActiveDialog::Spawn(_))));
}

#[test]
fn tab_bar_and_footer_inherit_app_background() {
    use herdr_tui::ui::tabs::TabHitbox;

    let mut app = TuiApp::new(Config::default(), Some("api".into()));
    let mut t1 = Terminal {
        address: "t1".into(),
        state: "running".into(),
        argv: vec!["zsh".into()],
        created_ms: 1,
        ..Default::default()
    };
    t1.labels.insert("space".into(), "api".into());
    let mut t2 = Terminal {
        address: "t2".into(),
        state: "running".into(),
        argv: vec!["bash".into()],
        created_ms: 2,
        ..Default::default()
    };
    t2.labels.insert("space".into(), "api".into());
    app.model.terminals.insert("t1".into(), t1);
    app.model.terminals.insert("t2".into(), t2);
    app.model.declared_spaces.insert("api".into());

    let pid = app.focused_pane.expect("focused pane");
    app.current_tree_mut().set_terminal(
        pid,
        Some(TermRef {
            address: "t1".into(),
        }),
    );

    let now = std::time::Instant::now();
    let area = Rect::new(0, 0, 120, 30);
    let mut buf = Buffer::empty(area);
    app.render(area, &mut buf, now);

    // Sidebar and pane frames leave buffer background at Color::Reset
    let sidebar_corner = buf.cell((0, 0)).expect("sidebar corner");
    assert_eq!(sidebar_corner.bg, Color::Reset);
    let pane_corner = buf
        .cell((app.last_panes_area.x, app.last_panes_area.y))
        .expect("pane corner");
    assert_eq!(pane_corner.bg, Color::Reset);

    // Footer status bar inherits app background (Color::Reset, not SUMI_INK0)
    let footer_y = area.height - 1;
    for x in 0..area.width {
        let cell = buf.cell((x, footer_y)).expect("footer cell");
        assert_eq!(
            cell.bg,
            Color::Reset,
            "footer cell at x={x} should inherit bg"
        );
        assert_ne!(cell.bg, Theme::SUMI_INK0);
        assert_eq!(cell.fg, Theme::OLD_WHITE);
    }

    let t1_rect = app
        .last_tabs_geom
        .iter()
        .find_map(|h| match h {
            TabHitbox::Terminal { address, rect } if address == "t1" => Some(*rect),
            _ => None,
        })
        .expect("t1 tab hitbox");
    let t2_rect = app
        .last_tabs_geom
        .iter()
        .find_map(|h| match h {
            TabHitbox::Terminal { address, rect } if address == "t2" => Some(*rect),
            _ => None,
        })
        .expect("t2 tab hitbox");
    let new_rect = app
        .last_tabs_geom
        .iter()
        .find_map(|h| match h {
            TabHitbox::NewTerminal(rect) => Some(*rect),
            _ => None,
        })
        .expect("NewTerminal tab hitbox");

    // Active tab pill preserves CRYSTAL_BLUE background and SUMI_INK0 foreground
    for x in t1_rect.x..t1_rect.x + t1_rect.width {
        let cell = buf.cell((x, 0)).expect("active tab cell");
        assert_eq!(cell.bg, Theme::CRYSTAL_BLUE);
        assert_eq!(cell.fg, Theme::SUMI_INK0);
    }

    // "│" separator between t1 and t2 inherits app background
    let sep_x = t1_rect.x + t1_rect.width;
    let sep_cell = buf.cell((sep_x, 0)).expect("separator cell");
    assert_eq!(sep_cell.symbol(), "│");
    assert_eq!(sep_cell.bg, Color::Reset);
    assert_ne!(sep_cell.bg, Theme::SUMI_INK0);
    assert_eq!(sep_cell.fg, Theme::SUMI_INK4);

    // Inactive tab pill preserves SUMI_INK3 background and OLD_WHITE foreground
    for x in t2_rect.x..t2_rect.x + t2_rect.width {
        let cell = buf.cell((x, 0)).expect("inactive tab cell");
        assert_eq!(cell.bg, Theme::SUMI_INK3);
        assert_eq!(cell.fg, Theme::OLD_WHITE);
    }

    // Trailing "│" separator and empty tab-bar space before [+ New Terminal] inherit app background
    let trailing_sep_x = t2_rect.x + t2_rect.width;
    let trailing_sep = buf
        .cell((trailing_sep_x, 0))
        .expect("trailing separator cell");
    assert_eq!(trailing_sep.symbol(), "│");
    assert_eq!(trailing_sep.bg, Color::Reset);
    assert_ne!(trailing_sep.bg, Theme::SUMI_INK0);

    assert!(
        trailing_sep_x + 1 < new_rect.x,
        "expected empty tab-bar space"
    );
    for x in (trailing_sep_x + 1)..new_rect.x {
        let cell = buf.cell((x, 0)).expect("empty tab-bar cell");
        assert_eq!(cell.symbol(), " ");
        assert_eq!(cell.bg, Color::Reset);
        assert_ne!(cell.bg, Theme::SUMI_INK0);
    }

    // [+ New Terminal] button matches tab_inactive style: SUMI_INK3 background, OLD_WHITE foreground
    for x in new_rect.x..new_rect.x + new_rect.width {
        let cell = buf.cell((x, 0)).expect("new terminal button cell");
        assert_eq!(cell.bg, Theme::SUMI_INK3);
        assert_eq!(cell.fg, Theme::OLD_WHITE);
        assert!(!cell.modifier.contains(Modifier::BOLD));
    }
}

#[test]
fn pane_mouse_drag_selection_and_osc52_copy() {
    use herdr_tui::ui::clipboard::osc52_sequence;

    assert_eq!(osc52_sequence("hello"), "\x1b]52;c;aGVsbG8=\x07");

    let mut app = TuiApp::new(Config::default(), Some("api".into()));
    let pid = app.focused_pane.expect("focused pane");
    app.current_tree_mut().set_terminal(
        pid,
        Some(TermRef {
            address: "t1".into(),
        }),
    );

    let mut sess = AttachSession::new("t1".into(), 60, 20);
    sess.state = SessionState::Active;
    sess.screen.process("hello world   \r\nسَلَام".as_bytes());
    app.sessions.insert("t1".into(), sess);

    let now = std::time::Instant::now();
    let area = Rect::new(0, 0, 100, 30);
    let mut buf = Buffer::empty(area);
    app.render(area, &mut buf, now);

    let hitbox = app.last_geom.panes.first().expect("pane hitbox");
    let ix = hitbox.rect.x + 1;
    let iy = hitbox.rect.y + 1;

    // 1. Mouse down -> drag -> up across 2 lines
    app.handle_mouse_event(
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: ix,
            row: iy,
            modifiers: KeyModifiers::NONE,
        },
        now,
    );
    app.handle_mouse_event(
        MouseEvent {
            kind: MouseEventKind::Drag(MouseButton::Left),
            column: ix + 15,
            row: iy + 1,
            modifiers: KeyModifiers::NONE,
        },
        now,
    );
    assert!(app.selection.is_some());
    let actions = app.handle_mouse_event(
        MouseEvent {
            kind: MouseEventKind::Up(MouseButton::Left),
            column: ix + 15,
            row: iy + 1,
            modifiers: KeyModifiers::NONE,
        },
        now,
    );
    let copied = actions.iter().find_map(|a| match a {
        AppAction::CopyToClipboard(t) => Some(t.clone()),
        _ => None,
    });
    assert_eq!(copied.as_deref(), Some("hello world\nسَلَام"));

    // 2. Plain click (down + up at same cell) clears selection and does not copy
    app.handle_mouse_event(
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: ix + 2,
            row: iy,
            modifiers: KeyModifiers::NONE,
        },
        now,
    );
    let actions2 = app.handle_mouse_event(
        MouseEvent {
            kind: MouseEventKind::Up(MouseButton::Left),
            column: ix + 2,
            row: iy,
            modifiers: KeyModifiers::NONE,
        },
        now,
    );
    assert!(app.selection.is_none());
    assert!(!actions2
        .iter()
        .any(|a| matches!(a, AppAction::CopyToClipboard(_))));
}

#[test]
fn mouse_passthrough_active_pane_forwards_and_focuses() {
    let mut app = TuiApp::new(Config::default(), Some("api".into()));
    let p1 = app.focused_pane.expect("focused pane");
    app.current_tree_mut().set_terminal(
        p1,
        Some(TermRef {
            address: "t1".into(),
        }),
    );
    let p2 = app
        .current_tree_mut()
        .split(
            p1,
            SplitAxis::Horizontal,
            Some(TermRef {
                address: "t2".into(),
            }),
        )
        .unwrap();

    let mut s1 = AttachSession::new("t1".into(), 40, 20);
    s1.state = SessionState::Active;
    s1.screen.process(b"\x1b[?1003h\x1b[?1006h");

    let mut s2 = AttachSession::new("t2".into(), 40, 20);
    s2.state = SessionState::Active;
    s2.screen.process(b"\x1b[?1002h\x1b[?1006h");

    app.sessions.insert("t1".into(), s1);
    app.sessions.insert("t2".into(), s2);
    app.sidebar_focused = true;
    app.selection = Some(herdr_tui::ui::selection::TextSelection {
        pane_id: p1,
        start: (0, 0),
        end: (1, 0),
    });

    let now = std::time::Instant::now();
    let area = Rect::new(0, 0, 120, 30);
    let mut buf = Buffer::empty(area);
    app.render(area, &mut buf, now);

    let p1_rect = app
        .last_geom
        .panes
        .iter()
        .find(|p| p.pane_id == p1)
        .unwrap()
        .rect;
    let p2_rect = app
        .last_geom
        .panes
        .iter()
        .find(|p| p.pane_id == p2)
        .unwrap()
        .rect;

    // Moved on unfocused pane 1 (AnyMotion) forwards without stealing focus
    app.focused_pane = Some(p2);
    let moved_actions = app.handle_mouse_event(
        MouseEvent {
            kind: MouseEventKind::Moved,
            column: p1_rect.x + 3,
            row: p1_rect.y + 2,
            modifiers: KeyModifiers::NONE,
        },
        now,
    );
    assert_eq!(
        moved_actions,
        vec![AppAction::SendInput {
            terminal: "t1".into(),
            bytes: b"\x1b[<35;3;2M".to_vec(),
        }]
    );
    assert_eq!(app.focused_pane, Some(p2));

    // Down on unfocused pane 1 focuses pane 1, clears sidebar_focused & selection, and sends SGR Down
    app.dirty = false;
    let down_actions = app.handle_mouse_event(
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: p1_rect.x + 2,
            row: p1_rect.y + 3,
            modifiers: KeyModifiers::NONE,
        },
        now,
    );
    assert_eq!(
        down_actions,
        vec![AppAction::SendInput {
            terminal: "t1".into(),
            bytes: b"\x1b[<0;2;3M".to_vec(),
        }]
    );
    assert_eq!(app.focused_pane, Some(p1));
    assert!(!app.sidebar_focused);
    assert!(app.selection.is_none());
    assert!(app.dirty);

    // Drag & Up & ScrollUp on pane 2 (ButtonMotion + SGR); use x + 2 to stay clear of +/-1 divider tolerance
    let down_p2 = app.handle_mouse_event(
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: p2_rect.x + 2,
            row: p2_rect.y + 1,
            modifiers: KeyModifiers::NONE,
        },
        now,
    );
    assert_eq!(
        down_p2,
        vec![AppAction::SendInput {
            terminal: "t2".into(),
            bytes: b"\x1b[<0;2;1M".to_vec(),
        }]
    );
    let drag_p2 = app.handle_mouse_event(
        MouseEvent {
            kind: MouseEventKind::Drag(MouseButton::Left),
            column: p2_rect.x + 4,
            row: p2_rect.y + 2,
            modifiers: KeyModifiers::NONE,
        },
        now,
    );
    assert_eq!(
        drag_p2,
        vec![AppAction::SendInput {
            terminal: "t2".into(),
            bytes: b"\x1b[<32;4;2M".to_vec(),
        }]
    );
    let up_p2 = app.handle_mouse_event(
        MouseEvent {
            kind: MouseEventKind::Up(MouseButton::Left),
            column: p2_rect.x + 4,
            row: p2_rect.y + 2,
            modifiers: KeyModifiers::NONE,
        },
        now,
    );
    assert_eq!(
        up_p2,
        vec![AppAction::SendInput {
            terminal: "t2".into(),
            bytes: b"\x1b[<0;4;2m".to_vec(),
        }]
    );
    let scroll_p2 = app.handle_mouse_event(
        MouseEvent {
            kind: MouseEventKind::ScrollUp,
            column: p2_rect.x + 2,
            row: p2_rect.y + 2,
            modifiers: KeyModifiers::NONE,
        },
        now,
    );
    assert_eq!(
        scroll_p2,
        vec![AppAction::SendInput {
            terminal: "t2".into(),
            bytes: b"\x1b[<64;2;2M".to_vec(),
        }]
    );
}

#[test]
fn mouse_passthrough_drops_unrequested_events_without_starting_selection() {
    let mut app = TuiApp::new(Config::default(), Some("api".into()));
    let pid = app.focused_pane.expect("focused pane");
    app.current_tree_mut().set_terminal(
        pid,
        Some(TermRef {
            address: "t1".into(),
        }),
    );

    let mut sess = AttachSession::new("t1".into(), 60, 20);
    sess.state = SessionState::Active;
    // PressRelease (?1000h) does NOT request Drag or Moved
    sess.screen.process(b"\x1b[?1000h\x1b[?1006h");
    app.sessions.insert("t1".into(), sess);

    let now = std::time::Instant::now();
    let area = Rect::new(0, 0, 100, 30);
    let mut buf = Buffer::empty(area);
    app.render(area, &mut buf, now);

    let rect = app.last_geom.panes[0].rect;
    let moved = app.handle_mouse_event(
        MouseEvent {
            kind: MouseEventKind::Moved,
            column: rect.x + 2,
            row: rect.y + 2,
            modifiers: KeyModifiers::NONE,
        },
        now,
    );
    assert!(moved.is_empty());
    assert!(app.selection.is_none());

    let _ = app.handle_mouse_event(
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: rect.x + 2,
            row: rect.y + 2,
            modifiers: KeyModifiers::NONE,
        },
        now,
    );
    let dragged = app.handle_mouse_event(
        MouseEvent {
            kind: MouseEventKind::Drag(MouseButton::Left),
            column: rect.x + 5,
            row: rect.y + 2,
            modifiers: KeyModifiers::NONE,
        },
        now,
    );
    assert!(dragged.is_empty());
    assert!(app.selection.is_none());
}

#[test]
fn mouse_passthrough_shift_falls_back_to_selection_and_scrollback() {
    let mut app = TuiApp::new(Config::default(), Some("api".into()));
    let pid = app.focused_pane.expect("focused pane");
    app.current_tree_mut().set_terminal(
        pid,
        Some(TermRef {
            address: "t1".into(),
        }),
    );

    let mut sess = AttachSession::new("t1".into(), 60, 10);
    sess.state = SessionState::Active;
    sess.screen.process(b"\x1b[?1000h\x1b[?1006h");
    for i in 0..30 {
        sess.screen.process(format!("line {i}\r\n").as_bytes());
    }
    sess.screen.process(b"\x1b[Hcopy-me");
    app.sessions.insert("t1".into(), sess);

    let now = std::time::Instant::now();
    let area = Rect::new(0, 0, 100, 14);
    let mut buf = Buffer::empty(area);
    app.render(area, &mut buf, now);

    let rect = app.last_geom.panes[0].rect;
    let ix = rect.x + 1;
    let iy = rect.y + 1;

    // Shift + selection copies text instead of sending mouse input
    let down = app.handle_mouse_event(
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: ix,
            row: iy,
            modifiers: KeyModifiers::SHIFT,
        },
        now,
    );
    assert!(!down
        .iter()
        .any(|a| matches!(a, AppAction::SendInput { .. })));
    let _ = app.handle_mouse_event(
        MouseEvent {
            kind: MouseEventKind::Drag(MouseButton::Left),
            column: ix + 6,
            row: iy,
            modifiers: KeyModifiers::SHIFT,
        },
        now,
    );
    let up = app.handle_mouse_event(
        MouseEvent {
            kind: MouseEventKind::Up(MouseButton::Left),
            column: ix + 6,
            row: iy,
            modifiers: KeyModifiers::SHIFT,
        },
        now,
    );
    assert!(up
        .iter()
        .any(|a| matches!(a, AppAction::CopyToClipboard(_))));
    assert!(!up.iter().any(|a| matches!(a, AppAction::SendInput { .. })));

    // Shift + ScrollUp adjusts scrollback instead of sending wheel bytes
    let scroll = app.handle_mouse_event(
        MouseEvent {
            kind: MouseEventKind::ScrollUp,
            column: ix + 1,
            row: iy + 1,
            modifiers: KeyModifiers::SHIFT,
        },
        now,
    );
    assert!(scroll.is_empty());
    assert!(app.sessions.get("t1").unwrap().screen.scrollback() > 0);
}

#[test]
fn mouse_passthrough_observing_and_modal_guards() {
    let mut app = TuiApp::new(Config::default(), Some("api".into()));
    let pid = app.focused_pane.expect("focused pane");
    app.current_tree_mut().set_terminal(
        pid,
        Some(TermRef {
            address: "t1".into(),
        }),
    );

    let mut sess = AttachSession::new("t1".into(), 60, 10);
    sess.state = SessionState::Observing;
    sess.screen.process(b"\x1b[?1003h\x1b[?1006h");
    for i in 0..25 {
        sess.screen.process(format!("obs {i}\r\n").as_bytes());
    }
    app.sessions.insert("t1".into(), sess);

    let now = std::time::Instant::now();
    let area = Rect::new(0, 0, 100, 14);
    let mut buf = Buffer::empty(area);
    app.render(area, &mut buf, now);

    let hitbox = app.last_geom.panes[0];
    let ix = hitbox.rect.x + 1;
    let iy = hitbox.rect.y + 1;

    // Observing pane never sends input; Down starts selection and ScrollUp scrolls back
    let obs_down = app.handle_mouse_event(
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: ix,
            row: iy,
            modifiers: KeyModifiers::NONE,
        },
        now,
    );
    assert!(obs_down.is_empty());
    assert!(app.selection.is_some());
    let _ = app.handle_mouse_event(
        MouseEvent {
            kind: MouseEventKind::Up(MouseButton::Left),
            column: ix,
            row: iy,
            modifiers: KeyModifiers::NONE,
        },
        now,
    );
    let obs_scroll = app.handle_mouse_event(
        MouseEvent {
            kind: MouseEventKind::ScrollUp,
            column: ix,
            row: iy,
            modifiers: KeyModifiers::NONE,
        },
        now,
    );
    assert!(obs_scroll.is_empty());
    assert!(app.sessions.get("t1").unwrap().screen.scrollback() > 0);

    // [Take over] button still emits TakeOver
    let btn = hitbox.takeover_btn.expect("takeover button");
    let takeover = app.handle_mouse_event(
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: btn.x + 1,
            row: btn.y,
            modifiers: KeyModifiers::NONE,
        },
        now,
    );
    assert_eq!(takeover, vec![AppAction::TakeOver("t1".into())]);

    // Promote to Active with mouse tracking, then open a modal -> all mouse events blocked
    app.sessions.get_mut("t1").unwrap().state = SessionState::Active;
    app.dialog = Some(ActiveDialog::Spawn(SpawnDialog::new(
        "api".into(),
        vec!["sh".into()],
    )));
    app.selection = None;
    let modal_down = app.handle_mouse_event(
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: ix,
            row: iy,
            modifiers: KeyModifiers::NONE,
        },
        now,
    );
    assert!(modal_down.is_empty());
    assert!(app.selection.is_none());
}

#[test]
fn close_pane_and_quit_cleanup_empty_spaces_and_unassigned_splits() {
    use herdr_tui::model::tree::UNASSIGNED_SPACE;

    let now = std::time::Instant::now();
    let ctrl_b = crossterm::event::KeyEvent::new(KeyCode::Char('b'), KeyModifiers::CONTROL);
    let key_q = crossterm::event::KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE);
    let key_d = crossterm::event::KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE);
    let key_pct = crossterm::event::KeyEvent::new(KeyCode::Char('%'), KeyModifiers::NONE);

    // 1. Ctrl+b q on last empty pane of named space removes space without "Cannot close last pane" toast
    let mut app = TuiApp::new(Config::default(), Some("api".into()));
    app.handle_key(ctrl_b, now);
    let actions = app.handle_key(key_q, now + std::time::Duration::from_millis(20));
    assert!(!app.should_quit);
    assert!(actions.contains(&AppAction::RemoveLayout("api".into())));
    assert!(!app.layouts.spaces.contains_key("api"));
    assert!(!app.model.declared_spaces.contains("api"));
    assert!(app.toast.is_none());

    // 2. UNASSIGNED_SPACE closing last terminal or split emits RemoveLayout(UNASSIGNED_SPACE), never SaveLayout
    let mut uapp = TuiApp::new(Config::default(), None);
    let p1 = uapp.focused_pane.unwrap();
    uapp.current_tree_mut().set_terminal(
        p1,
        Some(TermRef {
            address: "t1".into(),
        }),
    );
    uapp.handle_key(ctrl_b, now);
    let uactions = uapp.handle_key(key_q, now + std::time::Duration::from_millis(20));
    assert_eq!(
        uactions,
        vec![
            AppAction::Kill("t1".into()),
            AppAction::Remove("t1".into()),
            AppAction::RemoveLayout(UNASSIGNED_SPACE.into()),
        ]
    );
    assert!(!uactions
        .iter()
        .any(|a| matches!(a, AppAction::SaveLayout(_))));

    // Split empty UNASSIGNED_SPACE and close one pane -> RemoveLayout(UNASSIGNED_SPACE), never SaveLayout
    uapp.handle_key(ctrl_b, now);
    uapp.handle_key(key_pct, now + std::time::Duration::from_millis(20));
    assert_eq!(uapp.current_tree().pane_ids().len(), 2);
    uapp.handle_key(ctrl_b, now);
    let split_close_actions = uapp.handle_key(key_q, now + std::time::Duration::from_millis(20));
    assert!(split_close_actions.contains(&AppAction::RemoveLayout(UNASSIGNED_SPACE.into())));
    assert!(!split_close_actions
        .iter()
        .any(|a| matches!(a, AppAction::SaveLayout(_))));

    // 3. Command::Quit (Ctrl+b d) clears all empty spaces (including UNASSIGNED_SPACE splits & Ctrl+b c spaces)
    let mut qapp = TuiApp::new(Config::default(), None);
    let up1 = qapp.focused_pane.unwrap();
    qapp.current_tree_mut()
        .split(up1, SplitAxis::Horizontal, None);

    let (empty_space, _, _) = SpaceLayout::with_single(None);
    qapp.layouts.spaces.insert("empty-sp".into(), empty_space);
    qapp.model.declared_spaces.insert("empty-sp".into());

    let (occupied_space, _, _) = SpaceLayout::with_single(Some(TermRef {
        address: "t-live".into(),
    }));
    qapp.layouts.spaces.insert("keep-sp".into(), occupied_space);
    qapp.model.declared_spaces.insert("keep-sp".into());

    qapp.handle_key(ctrl_b, now);
    let quit_actions = qapp.handle_key(key_d, now + std::time::Duration::from_millis(20));
    assert_eq!(quit_actions, vec![AppAction::Quit]);
    assert!(qapp.should_quit);
    assert!(!qapp.layouts.spaces.contains_key(UNASSIGNED_SPACE));
    assert!(!qapp.layouts.spaces.contains_key("empty-sp"));
    assert!(!qapp.model.declared_spaces.contains("empty-sp"));
    assert!(qapp.layouts.spaces.contains_key("keep-sp"));
    assert!(qapp.model.declared_spaces.contains("keep-sp"));
}

#[test]
fn new_space_survives_refresh_while_active_and_cleans_on_switch_and_prune() {
    use herdr_tui::model::tree::UNASSIGNED_SPACE;
    use herdr_tui::ui::sidebar::SidebarItem;

    let now = std::time::Instant::now();
    let ctrl_b = crossterm::event::KeyEvent::new(KeyCode::Char('b'), KeyModifiers::CONTROL);
    let key_c = crossterm::event::KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE);

    let mut app = TuiApp::new(Config::default(), None);
    let mut t1 = Terminal {
        address: "t1".into(),
        state: "live".into(),
        argv: vec!["claude".into()],
        ..Default::default()
    };
    t1.labels.insert("space".into(), "api".into());
    app.model.terminals.insert("t1".into(), t1);
    app.model.declared_spaces.insert("api".into());
    let (mut api_tree, api_p1) = SplitTree::with_single(Some(TermRef {
        address: "t1".into(),
    }));
    api_tree.split(api_p1, SplitAxis::Horizontal, None);
    app.layouts
        .spaces
        .insert("api".into(), SpaceLayout::from_tree(api_tree));
    app.refresh_sidebar();

    // Create "draft" via Ctrl+b c
    app.handle_key(ctrl_b, now);
    app.handle_key(key_c, now + std::time::Duration::from_millis(20));
    for ch in "draft".chars() {
        app.handle_key(
            crossterm::event::KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE),
            now,
        );
    }
    app.handle_key(
        crossterm::event::KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        now,
    );
    assert_eq!(app.active_space, "draft");

    // Background refresh_sidebar must preserve active "draft" (pruned == 0)
    app.refresh_sidebar();
    assert!(app.layouts.spaces.contains_key("draft"));
    assert!(app.model.declared_spaces.contains("draft"));

    // Switching away via sidebar Enter removes empty "draft" and emits RemoveLayout("draft")
    app.sidebar_focused = true;
    let api_idx = app
        .sidebar
        .visible_items
        .iter()
        .position(|i| matches!(i, SidebarItem::Space { name, .. } if name == "api"))
        .expect("api space in sidebar");
    app.sidebar.selected = api_idx;
    let switch_actions = app.handle_key(
        crossterm::event::KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        now,
    );
    assert_eq!(app.active_space, "api");
    assert!(switch_actions.contains(&AppAction::RemoveLayout("draft".into())));
    assert!(!app.layouts.spaces.contains_key("draft"));
    assert!(!app.model.declared_spaces.contains("draft"));

    // Label move in model.terminals + refresh_sidebar unbinds t1 from "api", collapses split, and removes "api"
    app.model
        .terminals
        .get_mut("t1")
        .unwrap()
        .labels
        .insert("space".into(), "web".into());
    let emptied = app.prune_empty_spaces();
    assert!(emptied.contains(&"api".to_string()));
    assert!(!app.layouts.spaces.contains_key("api"));
    assert!(!app.model.declared_spaces.contains("api"));
    assert_eq!(app.active_space, UNASSIGNED_SPACE);

    // prune_terminal_from_spaces collapses multi-pane empty splits in UNASSIGNED_SPACE
    let up1 = app.focused_pane.unwrap();
    app.current_tree_mut().set_terminal(
        up1,
        Some(TermRef {
            address: "t2".into(),
        }),
    );
    app.current_tree_mut()
        .split(up1, SplitAxis::Horizontal, None);
    assert_eq!(app.current_tree().pane_ids().len(), 2);
    let pruned_spaces = app.prune_terminal_from_spaces("t2");
    assert!(pruned_spaces.contains(&UNASSIGNED_SPACE.to_string()));
    assert_eq!(app.current_tree().pane_ids().len(), 1);
    assert!(!app.current_tree().has_terminals());
}

#[tokio::test]
async fn dispatch_quit_prunes_empty_spaces_and_unassigned_from_store() {
    use fake_hc::FakeHcServer;
    use herdr_tui::hc::rpc::RpcClient;
    use herdr_tui::layout::store::LayoutStore;
    use herdr_tui::model::tree::UNASSIGNED_SPACE;
    use herdr_tui::ui::event_loop::dispatch_action;
    use tempfile::TempDir;

    let srv = FakeHcServer::start().await.expect("start fake server");
    let rpc = RpcClient::connect(srv.socket_path()).await.expect("rpc");
    let tmp = TempDir::new().unwrap();
    let fallback = tmp.path().join("layout.json");
    let store = LayoutStore::new(fallback.clone(), &rpc.hello().methods);

    let (mut unassigned_split, up1) = SplitTree::with_single(None);
    unassigned_split.split(up1, SplitAxis::Horizontal, None);
    let unassigned_space = SpaceLayout::from_tree(unassigned_split);
    let (empty_named, _, _) = SpaceLayout::with_single(None);
    let (occupied, _, _) = SpaceLayout::with_single(Some(TermRef {
        address: "t-keep".into(),
    }));

    store
        .save_space(Some(&rpc), UNASSIGNED_SPACE, &unassigned_space)
        .await
        .unwrap();
    store
        .save_space(Some(&rpc), "draft", &empty_named)
        .await
        .unwrap();
    store
        .save_space(Some(&rpc), "keep", &occupied)
        .await
        .unwrap();

    let mut app = TuiApp::new(Config::default(), Some("draft".into()));
    app.layouts
        .spaces
        .insert(UNASSIGNED_SPACE.into(), unassigned_space);
    app.layouts.spaces.insert("keep".into(), occupied.clone());
    app.model.declared_spaces.insert("keep".into());

    dispatch_action(AppAction::Quit, &mut app, Some(&rpc), None, &store).await;
    assert!(app.should_quit);
    assert!(!app.layouts.spaces.contains_key(UNASSIGNED_SPACE));
    assert!(!app.layouts.spaces.contains_key("draft"));
    assert!(app.layouts.spaces.contains_key("keep"));

    let (doc, _) = store.load(Some(&rpc)).await.unwrap();
    assert!(!doc.spaces.contains_key(UNASSIGNED_SPACE));
    assert!(!doc.spaces.contains_key("draft"));
    assert_eq!(doc.spaces.get("keep"), Some(&occupied));

    let local_doc = LayoutStore::load_local(&fallback);
    assert!(!local_doc.spaces.contains_key(UNASSIGNED_SPACE));
    assert!(!local_doc.spaces.contains_key("draft"));
    assert_eq!(local_doc.spaces.get("keep"), Some(&occupied));
}

#[tokio::test]
async fn reconcile_startup_layouts_cleans_stale_and_unassigned_while_preserving_create() {
    use fake_hc::FakeHcServer;
    use herdr_tui::hc::rpc::RpcClient;
    use herdr_tui::layout::store::LayoutStore;
    use herdr_tui::model::tree::UNASSIGNED_SPACE;
    use herdr_tui::ui::event_loop::reconcile_startup_layouts;
    use tempfile::TempDir;

    let srv = FakeHcServer::start().await.expect("start fake server");
    let rpc = RpcClient::connect(srv.socket_path()).await.expect("rpc");
    let tmp = TempDir::new().unwrap();
    let fallback = tmp.path().join("layout.json");
    let store = LayoutStore::new(fallback.clone(), &rpc.hello().methods);

    let (mut unassigned_split, up1) = SplitTree::with_single(None);
    unassigned_split.split(up1, SplitAxis::Horizontal, None);
    let unassigned_space = SpaceLayout::from_tree(unassigned_split);
    let (stale_space, _, _) = SpaceLayout::with_single(Some(TermRef {
        address: "t-dead".into(),
    }));
    let (mut active_tree, ap1) = SplitTree::with_single(Some(TermRef {
        address: "t-live".into(),
    }));
    active_tree.split(
        ap1,
        SplitAxis::Horizontal,
        Some(TermRef {
            address: "t-dead2".into(),
        }),
    );
    let active_space_layout = SpaceLayout::from_tree(active_tree);

    store
        .save_space(Some(&rpc), UNASSIGNED_SPACE, &unassigned_space)
        .await
        .unwrap();
    store
        .save_space(Some(&rpc), "stale", &stale_space)
        .await
        .unwrap();
    store
        .save_space(Some(&rpc), "work", &active_space_layout)
        .await
        .unwrap();

    // Case 1: startup with initial_space_name = Some("new-space") (--create)
    let mut app = TuiApp::new(Config::default(), Some("new-space".into()));
    let (loaded, _) = store.load(Some(&rpc)).await.unwrap();
    app.layouts = loaded;
    let mut t_live = Terminal {
        address: "t-live".into(),
        state: "live".into(),
        argv: vec!["zsh".into()],
        ..Default::default()
    };
    t_live.labels.insert("space".into(), "work".into());
    app.model.terminals.insert("t-live".into(), t_live);

    reconcile_startup_layouts(&mut app, Some(&rpc), &store, Some("new-space")).await;

    assert_eq!(app.active_space, "new-space");
    assert!(app.focused_pane.is_some());
    assert_eq!(app.current_tree().pane_ids().len(), 1);
    assert!(app.model.declared_spaces.contains("new-space"));
    assert!(!app.layouts.spaces.contains_key("stale"));
    assert!(!app.model.declared_spaces.contains("stale"));
    assert!(!app.layouts.spaces.contains_key(UNASSIGNED_SPACE));
    assert!(!app.sidebar.visible_items.iter().any(|item| matches!(
        item,
        herdr_tui::ui::sidebar::SidebarItem::Space { name, .. }
            if name == "stale" || name == UNASSIGNED_SPACE
    )));
    assert!(app.sidebar.visible_items.iter().any(|item| matches!(
        item,
        herdr_tui::ui::sidebar::SidebarItem::Space { name, count: 0, .. } if name == "new-space"
    )));
    assert!(app.sidebar.visible_items.iter().any(|item| matches!(
        item,
        herdr_tui::ui::sidebar::SidebarItem::Space { name, count: 1, .. } if name == "work"
    )));

    // "work" had t-dead2 pruned and remains saved with t-live
    let (doc, _) = store.load(Some(&rpc)).await.unwrap();
    assert!(!doc.spaces.contains_key("stale"));
    assert!(!doc.spaces.contains_key(UNASSIGNED_SPACE));
    let saved_work = doc.spaces.get("work").expect("work saved");
    assert!(saved_work.has_terminals());
    let saved_work_tree = &saved_work.panels[0].tree;
    assert_eq!(saved_work_tree.find_terminal(ap1).unwrap().address, "t-live");
    assert!(saved_work_tree
        .pane_ids()
        .into_iter()
        .all(|pid| pid == ap1 || saved_work_tree.find_terminal(pid).is_none()));

    // Case 2: startup with initial_space_name = None and only UNASSIGNED_SPACE split -> collapses to 1 pane
    let mut app2 = TuiApp::new(Config::default(), None);
    app2.layouts
        .spaces
        .insert(UNASSIGNED_SPACE.into(), unassigned_space.clone());
    app2.focused_pane = Some(999);
    store
        .save_space(Some(&rpc), UNASSIGNED_SPACE, &unassigned_space)
        .await
        .unwrap();
    reconcile_startup_layouts(&mut app2, Some(&rpc), &store, None).await;
    assert_eq!(app2.active_space, UNASSIGNED_SPACE);
    assert_eq!(app2.current_tree().pane_ids().len(), 1);
    assert_eq!(
        app2.focused_pane,
        app2.current_tree().pane_ids().into_iter().next()
    );
    let (doc2, _) = store.load(Some(&rpc)).await.unwrap();
    assert!(!doc2.spaces.contains_key(UNASSIGNED_SPACE));
    assert!(!app2
        .sidebar
        .visible_items
        .iter()
        .any(|item| matches!(item, herdr_tui::ui::sidebar::SidebarItem::Space { .. })));
}

#[tokio::test]
async fn core_labels_and_exit_events_prune_empty_spaces_from_store() {
    use fake_hc::FakeHcServer;
    use herdr_tui::hc::rpc::RpcClient;
    use herdr_tui::hc::types::EventPayload;
    use herdr_tui::layout::store::LayoutStore;
    use herdr_tui::model::tree::UNASSIGNED_SPACE;
    use herdr_tui::ui::event_loop::handle_core_event;
    use tempfile::TempDir;

    let srv = FakeHcServer::start().await.expect("start fake server");
    let rpc = RpcClient::connect(srv.socket_path()).await.expect("rpc");
    let tmp = TempDir::new().unwrap();
    let store = LayoutStore::new(tmp.path().join("layout.json"), &rpc.hello().methods);

    let mut app = TuiApp::new(Config::default(), Some("api".into()));
    let mut t1 = Terminal {
        address: "t1".into(),
        state: "live".into(),
        argv: vec!["claude".into()],
        ..Default::default()
    };
    t1.labels.insert("space".into(), "api".into());
    app.model.terminals.insert("t1".into(), t1);
    app.model.declared_spaces.insert("api".into());
    let p1 = app.focused_pane.unwrap();
    app.current_tree_mut().set_terminal(
        p1,
        Some(TermRef {
            address: "t1".into(),
        }),
    );
    app.current_tree_mut()
        .split(p1, SplitAxis::Horizontal, None);
    store
        .save_space(Some(&rpc), "api", &app.layouts.spaces["api"])
        .await
        .unwrap();
    app.refresh_sidebar();

    // Moving t1 from "api" to "web" via "labels" event prunes "api" from app & store
    let labels_ev = EventPayload {
        seq: 1,
        terminal: Some("t1".into()),
        event_type: "labels".into(),
        data: serde_json::json!({ "labels": { "space": "web" } }),
    };
    handle_core_event(labels_ev, &mut app, Some(&rpc), &store).await;
    assert!(!app.layouts.spaces.contains_key("api"));
    assert!(!app.model.declared_spaces.contains("api"));
    let (doc, _) = store.load(Some(&rpc)).await.unwrap();
    assert!(!doc.spaces.contains_key("api"));

    // Attach t1 in UNASSIGNED_SPACE split, then "removed" event collapses split & removes from store
    app.switch_space(UNASSIGNED_SPACE.into());
    let up1 = app.focused_pane.unwrap();
    app.current_tree_mut().set_terminal(
        up1,
        Some(TermRef {
            address: "t1".into(),
        }),
    );
    app.current_tree_mut()
        .split(up1, SplitAxis::Horizontal, None);
    store
        .save_space(
            Some(&rpc),
            UNASSIGNED_SPACE,
            &app.layouts.spaces[UNASSIGNED_SPACE],
        )
        .await
        .unwrap();

    let rm_ev = EventPayload {
        seq: 2,
        terminal: Some("t1".into()),
        event_type: "removed".into(),
        data: serde_json::json!({}),
    };
    handle_core_event(rm_ev, &mut app, Some(&rpc), &store).await;
    assert_eq!(app.current_tree().pane_ids().len(), 1);
    assert!(!app.current_tree().has_terminals());
    let (doc2, _) = store.load(Some(&rpc)).await.unwrap();
    assert!(!doc2.spaces.contains_key(UNASSIGNED_SPACE));
}

#[tokio::test]
async fn close_pane_dispatches_kill_and_remove_against_fake_hc_and_store() {
    use fake_hc::{make_terminal, FakeHcServer};
    use herdr_tui::hc::error::HcErrorKind;
    use herdr_tui::hc::rpc::RpcClient;
    use herdr_tui::hc::types::EventPayload;
    use herdr_tui::layout::store::LayoutStore;
    use herdr_tui::model::tree::UNASSIGNED_SPACE;
    use herdr_tui::ui::event_loop::{dispatch_action, handle_core_event};
    use herdr_tui::ui::sidebar::SidebarItem;
    use tempfile::TempDir;

    let srv = FakeHcServer::start().await.expect("start fake server");

    let mut fake_t1 = make_terminal("t1", "claude");
    fake_t1["reported"] = serde_json::json!(true);
    fake_t1["labels"] = serde_json::json!({ "space": "api" });
    srv.add_terminal(fake_t1).await;

    let mut fake_t2 = make_terminal("t2", "claude");
    fake_t2["reported"] = serde_json::json!(true);
    fake_t2["labels"] = serde_json::json!({ "space": "web" });
    srv.add_terminal(fake_t2).await;

    let mut fake_t3 = make_terminal("t3", "sh");
    fake_t3["reported"] = serde_json::json!(true);
    fake_t3["labels"] = serde_json::json!({ "space": "web" });
    srv.add_terminal(fake_t3).await;

    let mut fake_t4 = make_terminal("t4", "sh");
    fake_t4["state"] = serde_json::json!("finished");
    fake_t4["reported"] = serde_json::json!(true);
    fake_t4["labels"] = serde_json::json!({ "space": "jobs" });
    srv.add_terminal(fake_t4).await;

    let rpc = RpcClient::connect(srv.socket_path()).await.expect("rpc");
    let tmp = TempDir::new().unwrap();
    let fallback_path = tmp.path().join("layout.json");
    let store = LayoutStore::new(fallback_path.clone(), &rpc.hello().methods);

    // FakeHcServer must reject terminal.rm on a live terminal with HcErrorKind::Live
    let rm_live_err = rpc
        .terminal_rm("t1")
        .await
        .expect_err("terminal.rm must reject live terminal");
    assert_eq!(rm_live_err.kind(), HcErrorKind::Live);

    let mut app = TuiApp::new(Config::default(), Some("api".into()));
    for term in rpc.terminal_list(true).await.unwrap().terminals {
        app.sessions.insert(
            term.address.clone(),
            AttachSession::new(term.address.clone(), 80, 24),
        );
        app.model.terminals.insert(term.address.clone(), term);
    }

    // "api": single pane t1
    let api_p1 = app.focused_pane.unwrap();
    app.current_tree_mut().set_terminal(
        api_p1,
        Some(TermRef {
            address: "t1".into(),
        }),
    );
    app.model.declared_spaces.insert("api".into());
    store
        .save_space(Some(&rpc), "api", &app.layouts.spaces["api"])
        .await
        .unwrap();

    // "web": split t2 + t3
    let (mut web_tree, web_p1) = SplitTree::with_single(Some(TermRef {
        address: "t2".into(),
    }));
    let web_p2 = web_tree
        .split(
            web_p1,
            SplitAxis::Horizontal,
            Some(TermRef {
                address: "t3".into(),
            }),
        )
        .unwrap();
    let web_space = SpaceLayout::from_tree(web_tree);
    app.layouts.spaces.insert("web".into(), web_space.clone());
    app.model.declared_spaces.insert("web".into());
    store
        .save_space(Some(&rpc), "web", &web_space)
        .await
        .unwrap();

    // "jobs": single pane t4 (already finished)
    let (jobs_space, _, _) = SpaceLayout::with_single(Some(TermRef {
        address: "t4".into(),
    }));
    app.layouts.spaces.insert("jobs".into(), jobs_space.clone());
    app.model.declared_spaces.insert("jobs".into());
    store
        .save_space(Some(&rpc), "jobs", &jobs_space)
        .await
        .unwrap();
    app.refresh_sidebar();

    let now = std::time::Instant::now();
    let ctrl_b = crossterm::event::KeyEvent::new(KeyCode::Char('b'), KeyModifiers::CONTROL);
    let key_x = crossterm::event::KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE);
    let key_q = crossterm::event::KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE);

    // 1. Close live t1 in "api" with Ctrl+b x -> Kill + Remove + RemoveLayout("api")
    app.handle_key(ctrl_b, now);
    let api_actions = app.handle_key(key_x, now + std::time::Duration::from_millis(20));
    assert_eq!(
        api_actions,
        vec![
            AppAction::Kill("t1".into()),
            AppAction::Remove("t1".into()),
            AppAction::RemoveLayout("api".into()),
        ]
    );
    for action in api_actions {
        dispatch_action(action, &mut app, Some(&rpc), None, &store).await;
    }
    assert!(app.toast.is_none(), "unexpected toast: {:?}", app.toast);

    // Replaying subsequent "finished" and "removed" core events is idempotent with no error toast
    for (seq, ev_type) in [(10, "finished"), (11, "removed")] {
        let ev = EventPayload {
            seq,
            terminal: Some("t1".into()),
            event_type: ev_type.into(),
            data: serde_json::json!({}),
        };
        handle_core_event(ev, &mut app, Some(&rpc), &store).await;
    }
    assert!(app.toast.is_none(), "unexpected toast: {:?}", app.toast);

    let remaining_addrs: Vec<String> = rpc
        .terminal_list(true)
        .await
        .unwrap()
        .terminals
        .into_iter()
        .map(|t| t.address)
        .collect();
    assert!(!remaining_addrs.contains(&"t1".to_string()));
    assert!(!app.model.terminals.contains_key("t1"));
    assert!(!app.sessions.contains_key("t1"));
    assert!(!app.layouts.spaces.contains_key("api"));
    assert!(!app.model.declared_spaces.contains("api"));
    assert!(!app.sidebar.agent_items.iter().any(|i| matches!(
        i,
        SidebarItem::Terminal { address, .. } if address == "t1"
    )));
    assert!(!app.sidebar.visible_items.iter().any(|i| matches!(
        i,
        SidebarItem::Space { name, .. } if name == UNASSIGNED_SPACE
    )));
    let (remote_doc, _) = store.load(Some(&rpc)).await.unwrap();
    assert!(!remote_doc.spaces.contains_key("api"));
    let local_doc = LayoutStore::load_local(&fallback_path);
    assert!(!local_doc.spaces.contains_key("api"));

    // 2. Switch to "web" (split t2 + t3), close t2 with Ctrl+b q -> Kill + Remove + SaveLayout("web")
    app.switch_space("web".into());
    app.focused_pane = Some(web_p1);
    app.handle_key(ctrl_b, now);
    let web_actions = app.handle_key(key_q, now + std::time::Duration::from_millis(20));
    assert_eq!(
        web_actions,
        vec![
            AppAction::Kill("t2".into()),
            AppAction::Remove("t2".into()),
            AppAction::SaveLayout("web".into()),
        ]
    );
    for action in web_actions {
        dispatch_action(action, &mut app, Some(&rpc), None, &store).await;
    }
    assert!(app.toast.is_none(), "unexpected toast: {:?}", app.toast);

    for (seq, ev_type) in [(12, "finished"), (13, "removed")] {
        let ev = EventPayload {
            seq,
            terminal: Some("t2".into()),
            event_type: ev_type.into(),
            data: serde_json::json!({}),
        };
        handle_core_event(ev, &mut app, Some(&rpc), &store).await;
    }
    assert!(app.toast.is_none(), "unexpected toast: {:?}", app.toast);

    let remaining_addrs: Vec<String> = rpc
        .terminal_list(true)
        .await
        .unwrap()
        .terminals
        .into_iter()
        .map(|t| t.address)
        .collect();
    assert!(!remaining_addrs.contains(&"t2".to_string()));
    assert!(remaining_addrs.contains(&"t3".to_string()));
    assert!(!app.model.terminals.contains_key("t2"));
    assert!(!app.sessions.contains_key("t2"));
    assert_eq!(app.current_tree().pane_ids(), vec![web_p2]);
    let (remote_doc, _) = store.load(Some(&rpc)).await.unwrap();
    assert_eq!(remote_doc.spaces["web"].panels[0].tree.pane_ids(), vec![web_p2]);
    let local_doc = LayoutStore::load_local(&fallback_path);
    assert_eq!(local_doc.spaces["web"].panels[0].tree.pane_ids(), vec![web_p2]);

    // 3. Switch to "jobs" (finished t4), close with Ctrl+b x -> Remove + RemoveLayout("jobs")
    app.switch_space("jobs".into());
    app.handle_key(ctrl_b, now);
    let jobs_actions = app.handle_key(key_x, now + std::time::Duration::from_millis(20));
    assert_eq!(
        jobs_actions,
        vec![
            AppAction::Remove("t4".into()),
            AppAction::RemoveLayout("jobs".into()),
        ]
    );
    for action in jobs_actions {
        dispatch_action(action, &mut app, Some(&rpc), None, &store).await;
    }
    assert!(app.toast.is_none(), "unexpected toast: {:?}", app.toast);

    let ev = EventPayload {
        seq: 14,
        terminal: Some("t4".into()),
        event_type: "removed".into(),
        data: serde_json::json!({}),
    };
    handle_core_event(ev, &mut app, Some(&rpc), &store).await;
    assert!(app.toast.is_none(), "unexpected toast: {:?}", app.toast);

    let remaining_addrs: Vec<String> = rpc
        .terminal_list(true)
        .await
        .unwrap()
        .terminals
        .into_iter()
        .map(|t| t.address)
        .collect();
    assert_eq!(remaining_addrs, vec!["t3".to_string()]);
    assert!(!app.model.terminals.contains_key("t4"));
    assert!(!app.sessions.contains_key("t4"));
    assert!(!app.layouts.spaces.contains_key("jobs"));
    assert!(!app.model.declared_spaces.contains("jobs"));
    assert!(!app.sidebar.visible_items.iter().any(|i| matches!(
        i,
        SidebarItem::Space { name, .. } if name == UNASSIGNED_SPACE
    )));
    let (remote_doc, _) = store.load(Some(&rpc)).await.unwrap();
    assert!(!remote_doc.spaces.contains_key("jobs"));
    let local_doc = LayoutStore::load_local(&fallback_path);
    assert!(!local_doc.spaces.contains_key("jobs"));
}

#[tokio::test]
async fn close_pane_empty_last_pane_with_unattached_dispatches_against_fake_hc_and_store() {
    use fake_hc::{make_terminal, FakeHcServer};
    use herdr_tui::hc::rpc::RpcClient;
    use herdr_tui::hc::types::EventPayload;
    use herdr_tui::layout::store::LayoutStore;
    use herdr_tui::model::tree::UNASSIGNED_SPACE;
    use herdr_tui::ui::event_loop::{dispatch_action, handle_core_event};
    use herdr_tui::ui::sidebar::SidebarItem;
    use tempfile::TempDir;

    let srv = FakeHcServer::start().await.expect("start fake server");

    let mut fake_api = make_terminal("t-api", "claude");
    fake_api["reported"] = serde_json::json!(true);
    fake_api["labels"] = serde_json::json!({ "space": "api" });
    srv.add_terminal(fake_api).await;

    let mut fake_u1 = make_terminal("t-u1", "claude");
    fake_u1["reported"] = serde_json::json!(true);
    srv.add_terminal(fake_u1).await;

    let mut fake_u2 = make_terminal("t-u2", "sh");
    fake_u2["state"] = serde_json::json!("finished");
    fake_u2["reported"] = serde_json::json!(true);
    srv.add_terminal(fake_u2).await;

    let mut fake_web = make_terminal("t-web", "zsh");
    fake_web["reported"] = serde_json::json!(true);
    fake_web["labels"] = serde_json::json!({ "space": "web" });
    srv.add_terminal(fake_web).await;

    let rpc = RpcClient::connect(srv.socket_path()).await.expect("rpc");
    let tmp = TempDir::new().unwrap();
    let fallback_path = tmp.path().join("layout.json");
    let store = LayoutStore::new(fallback_path.clone(), &rpc.hello().methods);

    let mut app = TuiApp::new(Config::default(), Some("api".into()));
    for term in rpc.terminal_list(true).await.unwrap().terminals {
        app.sessions.insert(
            term.address.clone(),
            AttachSession::new(term.address.clone(), 80, 24),
        );
        app.model.terminals.insert(term.address.clone(), term);
    }

    // "api" has an (empty) single pane while t-api is unattached in model.terminals
    app.model.declared_spaces.insert("api".into());
    store
        .save_space(Some(&rpc), "api", &app.layouts.spaces["api"])
        .await
        .unwrap();

    // "web" has attached t-web
    let (web_space, _, _) = SpaceLayout::with_single(Some(TermRef {
        address: "t-web".into(),
    }));
    app.layouts.spaces.insert("web".into(), web_space.clone());
    app.model.declared_spaces.insert("web".into());
    store
        .save_space(Some(&rpc), "web", &web_space)
        .await
        .unwrap();

    // UNASSIGNED_SPACE has an (empty) single pane while t-u1 and t-u2 are unattached
    let (u_space, _, _) = SpaceLayout::with_single(None);
    app.layouts
        .spaces
        .insert(UNASSIGNED_SPACE.into(), u_space.clone());
    store
        .save_space(Some(&rpc), UNASSIGNED_SPACE, &u_space)
        .await
        .unwrap();
    app.refresh_sidebar();

    let now = std::time::Instant::now();
    let ctrl_b = crossterm::event::KeyEvent::new(KeyCode::Char('b'), KeyModifiers::CONTROL);
    let key_x = crossterm::event::KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE);
    let key_q = crossterm::event::KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE);

    // 1. Close empty last pane of "api" -> kills/removes unattached t-api and removes "api"
    app.handle_key(ctrl_b, now);
    let api_actions = app.handle_key(key_x, now + std::time::Duration::from_millis(20));
    assert_eq!(
        api_actions,
        vec![
            AppAction::Kill("t-api".into()),
            AppAction::Remove("t-api".into()),
            AppAction::RemoveLayout("api".into()),
        ]
    );
    for action in api_actions {
        dispatch_action(action, &mut app, Some(&rpc), None, &store).await;
    }
    assert!(app.toast.is_none(), "unexpected toast: {:?}", app.toast);

    for (seq, ev_type) in [(1, "finished"), (2, "removed")] {
        let ev = EventPayload {
            seq,
            terminal: Some("t-api".into()),
            event_type: ev_type.into(),
            data: serde_json::json!({}),
        };
        handle_core_event(ev, &mut app, Some(&rpc), &store).await;
    }
    assert!(app.toast.is_none(), "unexpected toast: {:?}", app.toast);
    assert!(!app.layouts.spaces.contains_key("api"));
    assert!(!app.model.declared_spaces.contains("api"));
    let (remote_doc, _) = store.load(Some(&rpc)).await.unwrap();
    assert!(!remote_doc.spaces.contains_key("api"));
    let local_doc = LayoutStore::load_local(&fallback_path);
    assert!(!local_doc.spaces.contains_key("api"));

    // 2. Switch to UNASSIGNED_SPACE (empty pane + unattached t-u1 & t-u2) -> closes both and switches to "web"
    app.switch_space(UNASSIGNED_SPACE.into());
    app.handle_key(ctrl_b, now);
    let u_actions = app.handle_key(key_q, now + std::time::Duration::from_millis(20));
    assert_eq!(
        u_actions,
        vec![
            AppAction::Kill("t-u1".into()),
            AppAction::Remove("t-u1".into()),
            AppAction::Remove("t-u2".into()),
            AppAction::RemoveLayout(UNASSIGNED_SPACE.into()),
        ]
    );
    for action in u_actions {
        dispatch_action(action, &mut app, Some(&rpc), None, &store).await;
    }
    assert!(app.toast.is_none(), "unexpected toast: {:?}", app.toast);

    for (seq, addr, ev_type) in [
        (3, "t-u1", "finished"),
        (4, "t-u1", "removed"),
        (5, "t-u2", "removed"),
    ] {
        let ev = EventPayload {
            seq,
            terminal: Some(addr.into()),
            event_type: ev_type.into(),
            data: serde_json::json!({}),
        };
        handle_core_event(ev, &mut app, Some(&rpc), &store).await;
    }
    assert!(app.toast.is_none(), "unexpected toast: {:?}", app.toast);
    assert_eq!(app.active_space, "web");
    assert!(!app.layouts.spaces.contains_key(UNASSIGNED_SPACE));

    let remaining_addrs: Vec<String> = rpc
        .terminal_list(true)
        .await
        .unwrap()
        .terminals
        .into_iter()
        .map(|t| t.address)
        .collect();
    assert_eq!(remaining_addrs, vec!["t-web".to_string()]);

    let (remote_doc2, _) = store.load(Some(&rpc)).await.unwrap();
    assert!(!remote_doc2.spaces.contains_key(UNASSIGNED_SPACE));
    assert!(remote_doc2.spaces.contains_key("web"));
    let local_doc2 = LayoutStore::load_local(&fallback_path);
    assert!(!local_doc2.spaces.contains_key(UNASSIGNED_SPACE));
    assert!(local_doc2.spaces.contains_key("web"));
    assert!(!app.sidebar.visible_items.iter().any(|i| matches!(
        i,
        SidebarItem::Space { name, .. } if name == UNASSIGNED_SPACE || name == "api"
    )));
    assert!(app.sidebar.visible_items.iter().any(|i| matches!(
        i,
        SidebarItem::Space { name, count: 1, .. } if name == "web"
    )));
}

#[test]
fn new_space_action_click_border_guard_and_submit_focus() {
    use herdr_tui::ui::sidebar::SidebarItem;

    let mut app = TuiApp::new(Config::default(), None);
    app.refresh_sidebar();
    app.sidebar_focused = false;
    let now = std::time::Instant::now();
    let area = Rect::new(0, 0, 100, 30);
    let mut buf = Buffer::empty(area);
    app.render(area, &mut buf, now);

    let (spaces_area, _) = app.last_sidebar_areas.unwrap();

    // Clicking the Spaces panel border (top row or left col) must NOT open NewSpaceDialog
    for (col, row) in [
        (spaces_area.x + 2, spaces_area.y),
        (spaces_area.x, spaces_area.y + 1),
    ] {
        app.handle_mouse_event(
            MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: col,
                row,
                modifiers: KeyModifiers::NONE,
            },
            now,
        );
        assert!(
            app.dialog.is_none(),
            "border click at ({col}, {row}) should not open dialog"
        );
    }

    // Clicking [+ New Space] at inner row spaces_area.y + 1 opens NewSpaceDialog
    app.handle_mouse_event(
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: spaces_area.x + 2,
            row: spaces_area.y + 1,
            modifiers: KeyModifiers::NONE,
        },
        now,
    );
    assert!(matches!(app.dialog, Some(ActiveDialog::NewSpace(_))));

    for ch in "frontend".chars() {
        app.handle_key(
            crossterm::event::KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE),
            now,
        );
    }
    let actions = app.handle_key(
        crossterm::event::KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        now,
    );
    assert!(app.dialog.is_none());
    assert!(actions.contains(&AppAction::SaveLayout("frontend".into())));
    assert_eq!(app.active_space, "frontend");
    assert!(!app.sidebar_focused);
    assert!(app.sidebar.visible_items.iter().any(|item| matches!(
        item,
        SidebarItem::Space { name, count: 0, .. } if name == "frontend"
    )));
    assert_eq!(app.sidebar.active_panel, SidebarPanel::Spaces);
    assert!(matches!(
        app.sidebar.selected_item(),
        Some(SidebarItem::Space { name, .. }) if name == "frontend"
    ));
}

#[test]
fn new_space_dialog_rejects_empty_whitespace_and_unassigned() {
    use herdr_tui::model::tree::UNASSIGNED_SPACE;

    let mut app = TuiApp::new(Config::default(), None);
    let now = std::time::Instant::now();
    let ctrl_b = crossterm::event::KeyEvent::new(KeyCode::Char('b'), KeyModifiers::CONTROL);
    let key_c = crossterm::event::KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE);

    // 1. Whitespace-only input closes NewSpaceDialog without creating a space
    app.handle_key(ctrl_b, now);
    app.handle_key(key_c, now + std::time::Duration::from_millis(20));
    for ch in "   ".chars() {
        app.handle_key(
            crossterm::event::KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE),
            now,
        );
    }
    let actions = app.handle_key(
        crossterm::event::KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        now,
    );
    assert!(app.dialog.is_none());
    assert!(actions.is_empty());
    assert_eq!(app.active_space, UNASSIGNED_SPACE);
    assert!(app.model.declared_spaces.is_empty());

    // 2. "(unassigned)" input is rejected without inserting into declared_spaces or emitting SaveLayout("(unassigned)")
    app.handle_key(ctrl_b, now + std::time::Duration::from_millis(200));
    app.handle_key(key_c, now + std::time::Duration::from_millis(220));
    for ch in UNASSIGNED_SPACE.chars() {
        app.handle_key(
            crossterm::event::KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE),
            now,
        );
    }
    let actions = app.handle_key(
        crossterm::event::KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        now,
    );
    assert!(app.dialog.is_none());
    assert!(!actions
        .iter()
        .any(|a| matches!(a, AppAction::SaveLayout(s) if s == UNASSIGNED_SPACE)));
    assert!(!app.model.declared_spaces.contains(UNASSIGNED_SPACE));
    assert_eq!(app.active_space, UNASSIGNED_SPACE);
    assert_eq!(app.active_toast(now), Some("Invalid space name"));
}

#[test]
fn new_space_submit_existing_name_preserves_layout_and_terminals() {
    use herdr_tui::ui::sidebar::SidebarItem;

    let mut app = TuiApp::new(Config::default(), Some("api".into()));
    let p1 = app.focused_pane.unwrap();
    app.current_tree_mut().set_terminal(
        p1,
        Some(TermRef {
            address: "t1".into(),
        }),
    );
    let _p2 = app
        .current_tree_mut()
        .split(p1, SplitAxis::Horizontal, None)
        .unwrap();
    let mut t1 = Terminal {
        address: "t1".into(),
        state: "live".into(),
        argv: vec!["claude".into()],
        ..Default::default()
    };
    t1.labels.insert("space".into(), "api".into());
    app.model.terminals.insert("t1".into(), t1);

    // Switch to "backend" with its own terminal so "backend" stays in layout
    app.switch_space("backend".into());
    app.model.declared_spaces.insert("backend".into());
    let bp = app.focused_pane.unwrap();
    app.current_tree_mut().set_terminal(
        bp,
        Some(TermRef {
            address: "t2".into(),
        }),
    );
    let mut t2 = Terminal {
        address: "t2".into(),
        state: "live".into(),
        argv: vec!["zsh".into()],
        ..Default::default()
    };
    t2.labels.insert("space".into(), "backend".into());
    app.model.terminals.insert("t2".into(), t2);
    app.refresh_sidebar();
    app.sidebar_focused = true;

    let now = std::time::Instant::now();
    let ctrl_b = crossterm::event::KeyEvent::new(KeyCode::Char('b'), KeyModifiers::CONTROL);
    let key_c = crossterm::event::KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE);
    app.handle_key(ctrl_b, now);
    app.handle_key(key_c, now + std::time::Duration::from_millis(20));
    for ch in "api".chars() {
        app.handle_key(
            crossterm::event::KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE),
            now,
        );
    }
    app.handle_key(
        crossterm::event::KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        now,
    );

    assert_eq!(app.active_space, "api");
    assert!(!app.sidebar_focused);
    assert_eq!(app.layouts.spaces["api"].panels[0].tree.pane_ids().len(), 2);
    assert_eq!(
        app.layouts.spaces["api"].panels[0]
            .tree
            .find_terminal(p1)
            .map(|t| t.address.as_str()),
        Some("t1")
    );
    assert!(app.sidebar.visible_items.iter().any(|item| matches!(
        item,
        SidebarItem::Space { name, count: 1, .. } if name == "api"
    )));
    assert_eq!(app.sidebar.active_panel, SidebarPanel::Spaces);
    assert!(matches!(
        app.sidebar.selected_item(),
        Some(SidebarItem::Space { name, .. }) if name == "api"
    ));
}

#[test]
fn sidebar_switch_away_from_zero_visible_terminal_space_refreshes_sidebar() {
    use herdr_tui::ui::sidebar::SidebarItem;

    let mut cfg = Config::default();
    cfg.ui.show_finished = false;
    let mut app = TuiApp::new(cfg, Some("jobs".into()));
    let jp = app.focused_pane.unwrap();
    app.current_tree_mut().set_terminal(
        jp,
        Some(TermRef {
            address: "t-fin".into(),
        }),
    );
    let mut t_fin = Terminal {
        address: "t-fin".into(),
        state: "finished".into(),
        argv: vec!["sh".into()],
        ..Default::default()
    };
    t_fin.labels.insert("space".into(), "jobs".into());
    app.model.terminals.insert("t-fin".into(), t_fin);

    // Another space "api" with a live terminal
    let mut t_live = Terminal {
        address: "t-live".into(),
        state: "live".into(),
        argv: vec!["claude".into()],
        ..Default::default()
    };
    t_live.labels.insert("space".into(), "api".into());
    app.model.terminals.insert("t-live".into(), t_live);
    app.model.declared_spaces.insert("api".into());
    app.refresh_sidebar();

    // Active "jobs" is visible as count: 0 when show_finished == false
    assert!(app.sidebar.visible_items.iter().any(|item| matches!(
        item,
        SidebarItem::Space { name, count: 0, .. } if name == "jobs"
    )));

    // Switch via keyboard Enter on "api" in sidebar -> "jobs" must hide immediately while sidebar_focused stays true
    app.sidebar_focused = true;
    app.sidebar.active_panel = SidebarPanel::Spaces;
    app.sidebar.selected = app
        .sidebar
        .visible_items
        .iter()
        .position(|i| matches!(i, SidebarItem::Space { name, .. } if name == "api"))
        .unwrap();
    let now = std::time::Instant::now();
    app.handle_key(
        crossterm::event::KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        now,
    );
    assert_eq!(app.active_space, "api");
    assert!(app.sidebar_focused);
    assert!(!app.sidebar.visible_items.iter().any(|item| matches!(
        item,
        SidebarItem::Space { name, .. } if name == "jobs"
    )));
}

#[test]
fn draft_space_lifecycle_close_pane_switch_quit_and_disconnected_startup() {
    use herdr_tui::model::tree::UNASSIGNED_SPACE;
    use herdr_tui::ui::event_loop::init_disconnected_startup;
    use herdr_tui::ui::sidebar::SidebarItem;

    let now = std::time::Instant::now();
    let ctrl_b = crossterm::event::KeyEvent::new(KeyCode::Char('b'), KeyModifiers::CONTROL);
    let key_c = crossterm::event::KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE);
    let key_x = crossterm::event::KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE);
    let key_d = crossterm::event::KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE);

    // 4. Disconnected startup initializes sidebar so [+ New Space] and --create initial_space render on frame 1
    let mut disconnected = TuiApp::new(Config::default(), Some("boot-space".into()));
    assert!(disconnected.sidebar.visible_items.is_empty());
    init_disconnected_startup(&mut disconnected, now);
    assert!(!disconnected.connected);
    assert!(disconnected
        .sidebar
        .visible_items
        .iter()
        .any(|i| matches!(i, SidebarItem::NewSpaceAction)));
    assert!(disconnected.sidebar.visible_items.iter().any(|i| matches!(
        i,
        SidebarItem::Space { name, count: 0, .. } if name == "boot-space"
    )));
    assert!(!disconnected.sidebar.visible_items.iter().any(|i| matches!(
        i,
        SidebarItem::Space { name, .. } if name == UNASSIGNED_SPACE
    )));

    // 1. Creating "draft" via Ctrl+b c makes "draft (0)" visible and survives refresh/prune while active
    let mut app = TuiApp::new(Config::default(), None);
    app.handle_key(ctrl_b, now);
    app.handle_key(key_c, now + std::time::Duration::from_millis(20));
    for ch in "draft".chars() {
        app.handle_key(
            crossterm::event::KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE),
            now,
        );
    }
    app.handle_key(
        crossterm::event::KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        now,
    );
    assert_eq!(app.active_space, "draft");
    assert!(app.sidebar.visible_items.iter().any(|i| matches!(
        i,
        SidebarItem::Space { name, count: 0, .. } if name == "draft"
    )));

    app.refresh_sidebar();
    assert!(app.prune_empty_spaces().is_empty());
    assert!(app.layouts.spaces.contains_key("draft"));
    assert!(app.model.declared_spaces.contains("draft"));
    assert!(app.sidebar.visible_items.iter().any(|i| matches!(
        i,
        SidebarItem::Space { name, count: 0, .. } if name == "draft"
    )));

    // 2. Closing "draft"'s last empty pane via Command::ClosePane (Ctrl+b x) removes "draft" and falls back to UNASSIGNED_SPACE (hidden)
    app.handle_key(ctrl_b, now + std::time::Duration::from_millis(200));
    let close_actions = app.handle_key(key_x, now + std::time::Duration::from_millis(220));
    assert!(close_actions.contains(&AppAction::RemoveLayout("draft".into())));
    assert!(!app.layouts.spaces.contains_key("draft"));
    assert!(!app.model.declared_spaces.contains("draft"));
    assert_eq!(app.active_space, UNASSIGNED_SPACE);
    assert!(!app.sidebar.visible_items.iter().any(|i| matches!(
        i,
        SidebarItem::Space { name, .. } if name == "draft" || name == UNASSIGNED_SPACE
    )));

    // 3a. Re-create "draft" and switch away via switch_space -> removes "draft"
    app.handle_key(ctrl_b, now + std::time::Duration::from_millis(400));
    app.handle_key(key_c, now + std::time::Duration::from_millis(420));
    for ch in "draft".chars() {
        app.handle_key(
            crossterm::event::KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE),
            now,
        );
    }
    app.handle_key(
        crossterm::event::KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        now,
    );
    assert_eq!(app.active_space, "draft");
    let removed = app.switch_space(UNASSIGNED_SPACE.into());
    assert_eq!(removed.as_deref(), Some("draft"));
    app.refresh_sidebar();
    assert!(!app.layouts.spaces.contains_key("draft"));
    assert!(!app.model.declared_spaces.contains("draft"));
    assert!(!app.sidebar.visible_items.iter().any(|i| matches!(
        i,
        SidebarItem::Space { name, .. } if name == "draft" || name == UNASSIGNED_SPACE
    )));

    // 3b. Re-create "draft" and quit via Command::Quit (Ctrl+b d) -> removes "draft"
    app.handle_key(ctrl_b, now + std::time::Duration::from_millis(600));
    app.handle_key(key_c, now + std::time::Duration::from_millis(620));
    for ch in "draft".chars() {
        app.handle_key(
            crossterm::event::KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE),
            now,
        );
    }
    app.handle_key(
        crossterm::event::KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        now,
    );
    assert_eq!(app.active_space, "draft");
    app.handle_key(ctrl_b, now + std::time::Duration::from_millis(800));
    let quit_actions = app.handle_key(key_d, now + std::time::Duration::from_millis(820));
    assert_eq!(quit_actions, vec![AppAction::Quit]);
    assert!(app.should_quit);
    assert!(!app.layouts.spaces.contains_key("draft"));
    assert!(!app.model.declared_spaces.contains("draft"));
    assert!(!app.sidebar.visible_items.iter().any(|i| matches!(
        i,
        SidebarItem::Space { name, .. } if name == "draft" || name == UNASSIGNED_SPACE
    )));
}
