use std::collections::{BTreeMap, HashMap, HashSet};

use crate::hc::types::{
    CoreInfo, EventPayload, Foreground, MachineView, MissedPayload, Status, Terminal,
};
use crate::model::address::TerminalAddress;
use crate::model::tree::{MachineNode, SpaceNode, TerminalEntry, UNASSIGNED_SPACE};

#[derive(Debug, Clone, Default)]
pub struct AppModelState {
    pub local_core: CoreInfo,
    pub machines: Vec<MachineView>,
    pub terminals: HashMap<String, Terminal>,
    pub declared_spaces: HashSet<String>,
    pub last_seq: u64,
    pub last_foreign_size_ms: HashMap<String, u64>,
}

#[derive(Debug, Clone)]
pub enum ModelInput {
    Snapshot {
        seq: u64,
        terminals: Vec<Terminal>,
        machines: Vec<MachineView>,
    },
    Event(EventPayload, u64),
    Missed(MissedPayload),
    MachinesUpdated(Vec<MachineView>),
    DeclareSpace(String),
}

pub fn apply(mut state: AppModelState, input: ModelInput) -> AppModelState {
    match input {
        ModelInput::Snapshot {
            seq,
            terminals,
            machines,
        } => {
            state.last_seq = seq;
            state.terminals.clear();
            for t in terminals {
                if let Some(sp) = t.labels.get("space") {
                    if !sp.is_empty() {
                        state.declared_spaces.insert(sp.clone());
                    }
                }
                state.terminals.insert(t.address.clone(), t);
            }
            state.machines = machines;
        }
        ModelInput::Missed(missed) => {
            state.last_seq = missed.seq;
            state.terminals.clear();
            for t in missed.terminals {
                if let Some(sp) = t.labels.get("space") {
                    if !sp.is_empty() {
                        state.declared_spaces.insert(sp.clone());
                    }
                }
                state.terminals.insert(t.address.clone(), t);
            }
        }
        ModelInput::MachinesUpdated(machines) => {
            state.machines = machines;
        }
        ModelInput::DeclareSpace(name) => {
            if !name.is_empty() {
                state.declared_spaces.insert(name);
            }
        }
        ModelInput::Event(ev, now_ms) => {
            if ev.seq <= state.last_seq {
                return state;
            }
            state.last_seq = ev.seq;

            let Some(addr) = ev.terminal else {
                return state;
            };

            match ev.event_type.as_str() {
                "created" => {
                    if let Ok(t) = serde_json::from_value::<Terminal>(ev.data) {
                        if let Some(sp) = t.labels.get("space") {
                            if !sp.is_empty() {
                                state.declared_spaces.insert(sp.clone());
                            }
                        }
                        state.terminals.insert(addr, t);
                    }
                }
                "removed" => {
                    state.terminals.remove(&addr);
                }
                "title" => {
                    if let Some(t) = state.terminals.get_mut(&addr) {
                        if let Some(title) = ev.data.get("title").and_then(|v| v.as_str()) {
                            t.title = title.to_string();
                        }
                    }
                }
                "labels" => {
                    if let Some(t) = state.terminals.get_mut(&addr) {
                        if let Ok(labels) = serde_json::from_value::<HashMap<String, String>>(
                            ev.data["labels"].clone(),
                        ) {
                            if let Some(sp) = labels.get("space") {
                                if !sp.is_empty() {
                                    state.declared_spaces.insert(sp.clone());
                                }
                            }
                            t.labels = labels;
                        }
                    }
                }
                "keep" => {
                    if let Some(t) = state.terminals.get_mut(&addr) {
                        if let Some(k) = ev.data.get("keep").and_then(|v| v.as_bool()) {
                            t.keep = k;
                        }
                    }
                }
                "status" => {
                    if let Some(t) = state.terminals.get_mut(&addr) {
                        if ev.data.is_null() {
                            t.status = None;
                        } else if let Ok(st) = serde_json::from_value::<Status>(ev.data) {
                            t.status = Some(st);
                            t.reported = true;
                        }
                    }
                }
                "foreground" => {
                    if let Some(t) = state.terminals.get_mut(&addr) {
                        if ev.data.is_null() {
                            t.foreground = None;
                        } else if let Ok(fg) = serde_json::from_value::<Foreground>(ev.data) {
                            t.foreground = Some(fg);
                        }
                    }
                }
                "turn" => {
                    if let Some(t) = state.terminals.get_mut(&addr) {
                        if let Some(turn) = ev.data.get("turn").and_then(|v| v.as_u64()) {
                            t.turn = turn;
                        }
                    }
                }
                "size" => {
                    if let Some(t) = state.terminals.get_mut(&addr) {
                        if let Some(cols) = ev.data.get("cols").and_then(|v| v.as_u64()) {
                            t.cols = cols as u16;
                        }
                        if let Some(rows) = ev.data.get("rows").and_then(|v| v.as_u64()) {
                            t.rows = rows as u16;
                        }
                    }
                    state.last_foreign_size_ms.insert(addr, now_ms);
                }
                "finished" => {
                    if let Some(t) = state.terminals.get_mut(&addr) {
                        t.state = "finished".to_string();
                        t.exit_code = ev
                            .data
                            .get("exit_code")
                            .and_then(|v| v.as_i64())
                            .map(|c| c as i32);
                        t.signal = ev
                            .data
                            .get("signal")
                            .and_then(|v| v.as_i64())
                            .map(|s| s as i32);
                        t.finished_ms = ev.data.get("finished_ms").and_then(|v| v.as_u64());
                        t.reason = ev
                            .data
                            .get("reason")
                            .and_then(|v| v.as_str())
                            .map(String::from);
                    }
                }
                "restarted" => {
                    if let Some(t) = state.terminals.get_mut(&addr) {
                        t.state = "live".to_string();
                        t.reason = None;
                        t.exit_code = None;
                        t.signal = None;
                    }
                }
                _ => {}
            }
        }
    }
    state
}

impl AppModelState {
    pub fn build_tree(&self, show_finished: bool, show_unassigned: bool) -> Vec<MachineNode> {
        let local_name = if self.local_core.name.is_empty() {
            "local".to_string()
        } else {
            self.local_core.name.clone()
        };

        let mut machines: BTreeMap<String, MachineNode> = BTreeMap::new();
        let mut local_node = MachineNode {
            name: local_name.clone(),
            state: "connected".to_string(),
            is_local: true,
            spaces: BTreeMap::new(),
        };

        for sp in &self.declared_spaces {
            local_node
                .spaces
                .entry(sp.clone())
                .or_insert_with(|| SpaceNode {
                    name: sp.clone(),
                    terminals: Vec::new(),
                });
        }
        machines.insert(local_name.clone(), local_node);

        for m in &self.machines {
            machines
                .entry(m.name.clone())
                .or_insert_with(|| MachineNode {
                    name: m.name.clone(),
                    state: m.state.clone(),
                    is_local: false,
                    spaces: BTreeMap::new(),
                });
        }

        for t in self.terminals.values() {
            if !show_finished && !t.is_live() {
                continue;
            }
            let parsed = TerminalAddress::parse(&t.address);
            let m_name = parsed.machine.unwrap_or_else(|| local_name.clone());
            let entry = TerminalEntry {
                terminal: t.clone(),
            };
            let sp_name = entry.space_name().to_string();
            if !show_unassigned && sp_name == UNASSIGNED_SPACE {
                continue;
            }

            let m_node = machines
                .entry(m_name.clone())
                .or_insert_with(|| MachineNode {
                    name: m_name,
                    state: "connected".to_string(),
                    is_local: false,
                    spaces: BTreeMap::new(),
                });
            let sp_node = m_node
                .spaces
                .entry(sp_name.clone())
                .or_insert_with(|| SpaceNode {
                    name: sp_name,
                    terminals: Vec::new(),
                });
            sp_node.terminals.push(entry);
        }

        for m in machines.values_mut() {
            for sp in m.spaces.values_mut() {
                sp.terminals
                    .sort_by(|a, b| a.terminal.address.cmp(&b.terminal.address));
            }
        }

        machines.into_values().collect()
    }
}
