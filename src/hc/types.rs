use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct CoreInfo {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct HelloResult {
    pub core: CoreInfo,
    pub version: String,
    pub generation: u32,
    pub framed: bool,
    pub methods: Vec<String>,
    pub features: Vec<String>,
    pub keeps_running_after_logout: Option<bool>,
    pub logout_linger_user: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct HolderInfo {
    pub current: bool,
    pub instance: String,
    pub pid: u32,
    pub terminals: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct StatusResult {
    pub core: CoreInfo,
    pub version: String,
    pub pid: u32,
    pub uptime_ms: u64,
    pub holder: Option<HolderInfo>,
    pub holders: Vec<HolderInfo>,
    pub installed_by: Option<String>,
    pub keeps_running_after_logout: Option<bool>,
    pub logout_linger_user: Option<String>,
    pub live: usize,
    pub finished: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StatusKind {
    Idle,
    Working,
    Done,
    Blocked,
    Error,
    Other(String),
}

impl StatusKind {
    pub fn parse(s: &str) -> Self {
        match s {
            "idle" => Self::Idle,
            "working" => Self::Working,
            "done" => Self::Done,
            "blocked" => Self::Blocked,
            "error" => Self::Error,
            other => Self::Other(other.to_string()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Status {
    pub state: String,
    pub kind: Option<String>,
    pub progress: Option<u8>,
    pub msg: String,
    pub title: String,
    pub app: String,
}

impl Status {
    pub fn kind_enum(&self) -> StatusKind {
        StatusKind::parse(&self.state)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Foreground {
    pub pid: i32,
    pub name: String,
    pub argv: Vec<String>,
    pub cwd: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct FirstMessage {
    pub text: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Terminal {
    pub address: String,
    pub argv: Vec<String>,
    pub cols: u16,
    pub rows: u16,
    pub created_ms: u64,
    pub finished_ms: Option<u64>,
    pub cwd: String,
    pub die_with_parent: bool,
    pub exit_code: Option<i32>,
    pub signal: Option<i32>,
    pub foreground: Option<Foreground>,
    pub first_message: Option<FirstMessage>,
    pub identity: String,
    pub keep: bool,
    pub labels: HashMap<String, String>,
    pub name: Option<String>,
    pub notify: bool,
    pub parent: Option<String>,
    pub pid: Option<u32>,
    pub reason: Option<String>,
    pub records: HashMap<String, Status>,
    pub reported: bool,
    pub rm: bool,
    pub screen: String,
    pub state: String,
    pub status: Option<Status>,
    pub status_ms: Option<u64>,
    pub template: Option<String>,
    pub title: String,
    pub turn: u64,
}

impl Terminal {
    pub fn is_live(&self) -> bool {
        self.state == "live"
    }

    pub fn is_agent(&self) -> bool {
        self.template.is_some() || self.reported
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct MachineView {
    pub name: String,
    pub target: String,
    pub id: String,
    pub host: String,
    pub version: String,
    pub state: String,
    pub session: Option<String>,
    pub since_ms: Option<u64>,
    pub why: Option<String>,
    pub temporary: bool,
    pub via: bool,
    pub keeps_running_after_logout: Option<bool>,
    pub logout_linger_user: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ListResult {
    pub terminals: Vec<Terminal>,
    pub hidden_finished: usize,
    pub machines: Vec<MachineView>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct MachineListResult {
    pub machines: Vec<MachineView>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct TemplateInfo {
    pub name: String,
    pub description: String,
    pub command: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct TemplatesResult {
    pub templates: Vec<TemplateInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct AttachResult {
    pub channel: u32,
    pub terminal: Terminal,
    pub resize_notifications: bool,
    pub automatic_input: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct RemoveResult {
    pub removed: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ReadResult {
    pub text: String,
    pub title: String,
    pub cols: u16,
    pub rows: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct SubscribeResult {
    pub channel: Option<u32>,
    pub seq: u64,
    pub terminals: Vec<Terminal>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct DocGetResult {
    pub value: Value,
    pub version: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct DocSetResult {
    pub version: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct EventPayload {
    pub seq: u64,
    pub terminal: Option<String>,
    #[serde(rename = "type")]
    pub event_type: String,
    pub data: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct HeartbeatPayload {
    pub seq: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct MissedPayload {
    pub seq: u64,
    pub terminals: Vec<Terminal>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct InitialSize {
    pub cols: u16,
    pub rows: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct RunParams {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub argv: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on: Option<String>,
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub labels: HashMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<InitialSize>,
    pub keep: bool,
    pub wait: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hc::error::HcErrorKind;

    #[test]
    fn status_kind_fallback() {
        assert_eq!(StatusKind::parse("idle"), StatusKind::Idle);
        assert_eq!(StatusKind::parse("working"), StatusKind::Working);
        assert_eq!(StatusKind::parse("done"), StatusKind::Done);
        assert_eq!(StatusKind::parse("blocked"), StatusKind::Blocked);
        assert_eq!(StatusKind::parse("error"), StatusKind::Error);
        assert_eq!(
            StatusKind::parse("thinking_hard"),
            StatusKind::Other("thinking_hard".to_string())
        );
    }

    #[test]
    fn run_params_serializes_size_only_when_present() {
        let without_size = serde_json::to_value(RunParams::default()).expect("serialize");
        assert!(without_size.get("size").is_none());

        let with_size = serde_json::to_value(RunParams {
            argv: vec!["nvim".into()],
            size: Some(InitialSize {
                cols: 134,
                rows: 38,
            }),
            ..Default::default()
        })
        .expect("serialize");
        assert_eq!(
            with_size["size"],
            serde_json::json!({ "cols": 134, "rows": 38 })
        );
    }

    #[test]
    fn openrpc_error_kinds_covered() {
        let raw = include_str!("../../tests/fixtures/openrpc.json");
        let doc: Value = serde_json::from_str(raw).expect("valid openrpc json");
        let methods = doc["methods"].as_array().expect("methods array");
        for m in methods {
            if let Some(errs) = m.get("errors").and_then(|e| e.as_array()) {
                for err in errs {
                    if let Some(kind_str) = err
                        .get("data")
                        .and_then(|d| d.get("kind"))
                        .and_then(|k| k.as_str())
                    {
                        let mapped = HcErrorKind::from_wire(kind_str);
                        assert!(
                            !matches!(mapped, HcErrorKind::Other(_)),
                            "unmapped OpenRPC error kind: {kind_str}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn deserialize_events_fixture() {
        let raw = include_str!("../../tests/fixtures/events.jsonl");
        for (idx, line) in raw.lines().enumerate() {
            let v: Value = serde_json::from_str(line).expect("valid json line");
            if idx == 0 {
                let snap: SubscribeResult =
                    serde_json::from_value(v["result"].clone()).expect("snapshot");
                assert!(!snap.terminals.is_empty());
            } else if v.get("method").and_then(|m| m.as_str()) == Some("event") {
                let ev: EventPayload = serde_json::from_value(v["params"].clone()).expect("event");
                assert!(!ev.event_type.is_empty());
            } else if v.get("method").and_then(|m| m.as_str()) == Some("heartbeat") {
                let hb: HeartbeatPayload =
                    serde_json::from_value(v["params"].clone()).expect("heartbeat");
                assert!(hb.seq > 0);
            }
        }
    }
}
