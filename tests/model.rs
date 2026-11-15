use std::collections::HashMap;

use herdr_tui::hc::types::{CoreInfo, EventPayload, MachineView, MissedPayload, Terminal};
use herdr_tui::model::reducer::{apply, AppModelState, ModelInput};
use herdr_tui::model::tree::UNASSIGNED_SPACE;
use serde_json::json;

#[test]
fn state_reducer() {
    let mut state = AppModelState {
        local_core: CoreInfo {
            id: "c1".into(),
            name: "local".into(),
        },
        ..Default::default()
    };

    let mut t1 = Terminal {
        address: "t1".into(),
        state: "live".into(),
        ..Default::default()
    };
    t1.labels.insert("space".into(), "api".into());

    let t2 = Terminal {
        address: "t2".into(),
        state: "live".into(),
        ..Default::default()
    };

    let unreachable = MachineView {
        name: "vps".into(),
        state: "unreachable".into(),
        ..Default::default()
    };

    // ST-1: Snapshot populates machines, spaces, and (unassigned)
    state = apply(
        state,
        ModelInput::Snapshot {
            seq: 10,
            terminals: vec![t1.clone(), t2],
            machines: vec![unreachable],
        },
    );

    let tree = state.build_tree(true, true);
    assert_eq!(tree.len(), 2); // local + vps
    let local = tree.iter().find(|m| m.is_local).unwrap();
    assert_eq!(local.spaces["api"].terminals.len(), 1);
    assert_eq!(local.spaces[UNASSIGNED_SPACE].terminals.len(), 1);
    let vps = tree.iter().find(|m| m.name == "vps").unwrap();
    assert_eq!(vps.state, "unreachable"); // ST-5

    // ST-2: Label change moves t1 from api -> web without dropping declared space api
    state = apply(
        state,
        ModelInput::Event(
            EventPayload {
                seq: 11,
                terminal: Some("t1".into()),
                event_type: "labels".into(),
                data: json!({ "labels": { "space": "web" } }),
            },
            1000,
        ),
    );
    let tree2 = state.build_tree(true, true);
    let local2 = tree2.iter().find(|m| m.is_local).unwrap();
    assert!(local2.spaces.contains_key("api"));
    assert_eq!(local2.spaces["api"].terminals.len(), 0);
    assert_eq!(local2.spaces["web"].terminals.len(), 1);

    // ST-3: Missed snapshot replaces state and drops obsolete seq <= snapshot.seq
    let mut t3 = Terminal {
        address: "t3".into(),
        state: "live".into(),
        labels: HashMap::new(),
        ..Default::default()
    };
    t3.labels.insert("space".into(), "web".into());
    state = apply(
        state,
        ModelInput::Missed(MissedPayload {
            seq: 50,
            terminals: vec![t3],
        }),
    );
    assert_eq!(state.terminals.len(), 1);
    assert!(state.terminals.contains_key("t3"));

    // Stale event seq 49 must be ignored
    state = apply(
        state,
        ModelInput::Event(
            EventPayload {
                seq: 49,
                terminal: Some("t3".into()),
                event_type: "title".into(),
                data: json!({ "title": "stale" }),
            },
            2000,
        ),
    );
    assert_eq!(state.terminals["t3"].title, "");
}

#[test]
fn foreground_event_updates_display_name() {
    use herdr_tui::hc::types::Status;
    use herdr_tui::model::tree::TerminalEntry;

    let t1 = Terminal {
        address: "t1".into(),
        state: "live".into(),
        argv: vec!["/usr/bin/zsh".into()],
        title: "zsh".into(),
        ..Default::default()
    };

    let mut state = apply(
        AppModelState::default(),
        ModelInput::Snapshot {
            seq: 1,
            terminals: vec![t1],
            machines: Vec::new(),
        },
    );

    // Foreground event changes display_name to nvim
    state = apply(
        state,
        ModelInput::Event(
            EventPayload {
                seq: 2,
                terminal: Some("t1".into()),
                event_type: "foreground".into(),
                data: json!({
                    "argv": ["nvim", "src/main.rs"],
                    "cwd": "/tmp",
                    "name": "nvim",
                    "pid": 1234
                }),
            },
            1000,
        ),
    );
    let entry = TerminalEntry {
        terminal: state.terminals["t1"].clone(),
    };
    assert_eq!(entry.display_name(), "nvim");

    // Status app takes priority over foreground name
    state = apply(
        state,
        ModelInput::Event(
            EventPayload {
                seq: 3,
                terminal: Some("t1".into()),
                event_type: "status".into(),
                data: serde_json::to_value(Status {
                    app: "opencode".into(),
                    state: "working".into(),
                    ..Default::default()
                })
                .unwrap(),
            },
            2000,
        ),
    );
    let entry2 = TerminalEntry {
        terminal: state.terminals["t1"].clone(),
    };
    assert_eq!(entry2.display_name(), "opencode");
}
