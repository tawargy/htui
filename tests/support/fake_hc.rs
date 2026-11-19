#![allow(dead_code)]

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::{json, Value};
use tempfile::TempDir;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{mpsc, Mutex};
use tokio::task::JoinHandle;

pub const FRAME_DATA: u8 = 0;
pub const FRAME_CREDIT: u8 = 1;
pub const FRAME_CLOSE: u8 = 2;
pub const FRAME_RESIZE: u8 = 3;
pub const FRAME_FOCUS: u8 = 4;
pub const FRAME_FACTS: u8 = 5;
pub const FRAME_AUTO_INPUT: u8 = 6;

#[derive(Debug, Clone)]
pub struct RecordedFrame {
    pub channel: u32,
    pub kind: u8,
    pub payload: Vec<u8>,
}

pub struct FakeState {
    pub core_id: String,
    pub core_name: String,
    pub version: String,
    pub methods: Vec<String>,
    pub features: Vec<String>,
    pub terminals: Vec<Value>,
    pub templates: Vec<Value>,
    pub machines: Vec<Value>,
    pub docs: HashMap<String, (Value, u64)>,
    pub seq: u64,
    pub next_term_num: u64,
    pub subscribers: Vec<mpsc::UnboundedSender<Value>>,
    pub recorded_frames: Vec<RecordedFrame>,
    pub attach_outputs: HashMap<String, Vec<u8>>,
    pub echo_attach_input: bool,
    pub response_delay_ms: u64,
    pub attach_senders: HashMap<u32, mpsc::UnboundedSender<RecordedFrame>>,
}

impl Default for FakeState {
    fn default() -> Self {
        Self {
            core_id: "fake-core-0001".to_string(),
            core_name: "local".to_string(),
            version: "0.1.21".to_string(),
            methods: vec![
                "hello",
                "server.status",
                "terminal.run",
                "templates.list",
                "terminal.list",
                "terminal.get",
                "terminal.kill",
                "terminal.rm",
                "terminal.keep",
                "terminal.label",
                "terminal.read",
                "terminal.attach",
                "terminal.send",
                "terminal.keys",
                "terminal.interrupt",
                "doc.get",
                "doc.set",
                "events.subscribe",
                "machine.list",
            ]
            .into_iter()
            .map(String::from)
            .collect(),
            features: vec![
                "framed",
                "attach",
                "events",
                "status",
                "shell",
                "resize_notifications",
                "automatic_input",
            ]
            .into_iter()
            .map(String::from)
            .collect(),
            terminals: Vec::new(),
            templates: vec![json!({
                "name": "claude",
                "command": "claude",
                "description": "Claude Code agent"
            })],
            machines: Vec::new(),
            docs: HashMap::new(),
            seq: 1,
            next_term_num: 1,
            subscribers: Vec::new(),
            recorded_frames: Vec::new(),
            attach_outputs: HashMap::new(),
            echo_attach_input: true,
            response_delay_ms: 0,
            attach_senders: HashMap::new(),
        }
    }
}

pub struct FakeHcServer {
    _dir: TempDir,
    socket_path: PathBuf,
    pub state: Arc<Mutex<FakeState>>,
    accept_task: JoinHandle<()>,
}

impl FakeHcServer {
    pub async fn start() -> std::io::Result<Self> {
        let dir = TempDir::new()?;
        let socket_path = dir.path().join("fake-hc.sock");
        let listener = UnixListener::bind(&socket_path)?;
        let state = Arc::new(Mutex::new(FakeState::default()));
        let state_clone = Arc::clone(&state);

        let accept_task = tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                let st = Arc::clone(&state_clone);
                tokio::spawn(async move {
                    let _ = handle_connection(stream, st).await;
                });
            }
        });

        Ok(Self {
            _dir: dir,
            socket_path,
            state,
            accept_task,
        })
    }

    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }

    pub async fn push_event(&self, notif: Value) {
        let mut st = self.state.lock().await;
        st.subscribers.retain(|tx| tx.send(notif.clone()).is_ok());
    }

    pub async fn add_terminal(&self, term: Value) {
        let mut st = self.state.lock().await;
        st.terminals.push(term);
    }

    pub async fn recorded_frames(&self) -> Vec<RecordedFrame> {
        self.state.lock().await.recorded_frames.clone()
    }

    pub async fn send_attach_data(&self, channel: u32, data: Vec<u8>) {
        let st = self.state.lock().await;
        if let Some(tx) = st.attach_senders.get(&channel) {
            let _ = tx.send(RecordedFrame {
                channel,
                kind: FRAME_DATA,
                payload: data,
            });
        }
    }

    pub async fn send_attach_resize(&self, channel: u32, cols: u16, rows: u16) {
        let st = self.state.lock().await;
        if let Some(tx) = st.attach_senders.get(&channel) {
            let mut payload = Vec::with_capacity(4);
            payload.extend_from_slice(&cols.to_le_bytes());
            payload.extend_from_slice(&rows.to_le_bytes());
            let _ = tx.send(RecordedFrame {
                channel,
                kind: FRAME_RESIZE,
                payload,
            });
        }
    }
}

impl Drop for FakeHcServer {
    fn drop(&mut self) {
        self.accept_task.abort();
    }
}

pub fn encode_frame(channel: u32, kind: u8, payload: &[u8]) -> Vec<u8> {
    let mut buf = Vec::with_capacity(9 + payload.len());
    buf.extend_from_slice(&channel.to_le_bytes());
    buf.push(kind);
    buf.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    buf.extend_from_slice(payload);
    buf
}

async fn handle_connection(
    stream: UnixStream,
    state: Arc<Mutex<FakeState>>,
) -> std::io::Result<()> {
    let (read_half, mut write_half) = stream.into_split();
    let mut reader = BufReader::new(read_half);
    let mut line = String::new();
    let mut sub_rx: Option<mpsc::UnboundedReceiver<Value>> = None;

    // Plain JSON-RPC loop until framed mode or subscribe
    loop {
        line.clear();
        tokio::select! {
            n = reader.read_line(&mut line) => {
                let n = n?;
                if n == 0 {
                    return Ok(());
                }
                let req: Value = match serde_json::from_str(line.trim()) {
                    Ok(v) => v,
                    Err(_) => continue,
                };
                let (resp, switch_framed, new_sub) = dispatch_rpc(&req, &state, false).await;
                if let Some(r) = resp {
                    let mut out = serde_json::to_vec(&r).unwrap_or_default();
                    out.push(b'\n');
                    write_half.write_all(&out).await?;
                    write_half.flush().await?;
                }
                if let Some(rx) = new_sub {
                    sub_rx = Some(rx);
                }
                if switch_framed {
                    break;
                }
            }
            Some(notif) = async {
                match sub_rx.as_mut() {
                    Some(rx) => rx.recv().await,
                    None => std::future::pending().await,
                }
            } => {
                let mut out = serde_json::to_vec(&notif).unwrap_or_default();
                out.push(b'\n');
                write_half.write_all(&out).await?;
                write_half.flush().await?;
            }
        }
    }

    // Framed mode: 9-byte LE header
    let (tx_out, mut rx_out) = mpsc::unbounded_channel::<RecordedFrame>();
    let writer_task = tokio::spawn(async move {
        while let Some(frame) = rx_out.recv().await {
            let bytes = encode_frame(frame.channel, frame.kind, &frame.payload);
            if write_half.write_all(&bytes).await.is_err() {
                break;
            }
            if write_half.flush().await.is_err() {
                break;
            }
        }
    });

    let mut next_channel: u32 = 1;
    let mut header = [0u8; 9];
    loop {
        if reader.read_exact(&mut header).await.is_err() {
            break;
        }
        let channel = u32::from_le_bytes([header[0], header[1], header[2], header[3]]);
        let kind = header[4];
        let len = u32::from_le_bytes([header[5], header[6], header[7], header[8]]) as usize;
        let mut payload = vec![0u8; len];
        if len > 0 && reader.read_exact(&mut payload).await.is_err() {
            break;
        }

        if channel == 0 && kind == FRAME_DATA {
            if let Ok(req) = serde_json::from_slice::<Value>(&payload) {
                let method = req.get("method").and_then(|m| m.as_str()).unwrap_or("");
                if method == "terminal.attach" {
                    let ch = next_channel;
                    next_channel += 1;
                    let term_addr = req
                        .get("params")
                        .and_then(|p| p.get("terminal"))
                        .and_then(|t| t.as_str())
                        .unwrap_or("t1")
                        .to_string();
                    let resize_notifs = req
                        .get("params")
                        .and_then(|p| p.get("resize_notifications"))
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);

                    let (term_obj, init_paint) = {
                        let mut st = state.lock().await;
                        st.attach_senders.insert(ch, tx_out.clone());
                        let t = st
                            .terminals
                            .iter()
                            .find(|t| t.get("address").and_then(|a| a.as_str()) == Some(&term_addr))
                            .cloned()
                            .unwrap_or_else(|| make_terminal(&term_addr, "sh"));
                        let paint = st.attach_outputs.get(&term_addr).cloned();
                        (t, paint)
                    };

                    let resp = json!({
                        "jsonrpc": "2.0",
                        "id": req.get("id").cloned().unwrap_or(Value::Null),
                        "result": {
                            "channel": ch,
                            "terminal": term_obj,
                            "resize_notifications": resize_notifs,
                            "automatic_input": true
                        }
                    });
                    let _ = tx_out.send(RecordedFrame {
                        channel: 0,
                        kind: FRAME_DATA,
                        payload: serde_json::to_vec(&resp).unwrap_or_default(),
                    });
                    if let Some(paint) = init_paint {
                        let _ = tx_out.send(RecordedFrame {
                            channel: ch,
                            kind: FRAME_DATA,
                            payload: paint,
                        });
                    }
                    continue;
                }

                let (resp, _, new_sub) = dispatch_rpc(&req, &state, true).await;
                if let Some(mut r) = resp {
                    if method == "events.subscribe" {
                        let ch = next_channel;
                        next_channel += 1;
                        if let Some(res) = r.get_mut("result").and_then(|v| v.as_object_mut()) {
                            res.insert("channel".to_string(), json!(ch));
                        }
                        if let Some(mut rx) = new_sub {
                            let tx_ev = tx_out.clone();
                            tokio::spawn(async move {
                                while let Some(ev) = rx.recv().await {
                                    let mut bytes = serde_json::to_vec(&ev).unwrap_or_default();
                                    bytes.push(b'\n');
                                    if tx_ev
                                        .send(RecordedFrame {
                                            channel: ch,
                                            kind: FRAME_DATA,
                                            payload: bytes,
                                        })
                                        .is_err()
                                    {
                                        break;
                                    }
                                }
                            });
                        }
                    }
                    let _ = tx_out.send(RecordedFrame {
                        channel: 0,
                        kind: FRAME_DATA,
                        payload: serde_json::to_vec(&r).unwrap_or_default(),
                    });
                }
            }
        } else {
            let mut st = state.lock().await;
            st.recorded_frames.push(RecordedFrame {
                channel,
                kind,
                payload: payload.clone(),
            });
            if kind == FRAME_DATA && st.echo_attach_input {
                let _ = tx_out.send(RecordedFrame {
                    channel,
                    kind: FRAME_DATA,
                    payload,
                });
            }
        }
    }

    writer_task.abort();
    Ok(())
}

async fn dispatch_rpc(
    req: &Value,
    state: &Arc<Mutex<FakeState>>,
    _framed: bool,
) -> (Option<Value>, bool, Option<mpsc::UnboundedReceiver<Value>>) {
    let id = req.get("id").cloned().unwrap_or(Value::Null);
    let method = req.get("method").and_then(|m| m.as_str()).unwrap_or("");
    let params = req.get("params").cloned().unwrap_or_else(|| json!({}));

    let delay = { state.lock().await.response_delay_ms };
    if delay > 0 {
        tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
    }

    let mut st = state.lock().await;
    match method {
        "hello" => {
            let gen = params
                .get("generation")
                .and_then(|g| g.as_u64())
                .unwrap_or(1);
            if gen != 1 {
                return (
                    Some(json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "error": {
                            "code": -32000,
                            "message": format!("protocol generation {gen} not supported"),
                            "data": { "kind": "unsupported_generation", "hint": "this core speaks generation 1" }
                        }
                    })),
                    false,
                    None,
                );
            }
            let want_framed = params
                .get("framed")
                .and_then(|f| f.as_bool())
                .unwrap_or(false);
            let resp = json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "core": { "id": st.core_id, "name": st.core_name },
                    "version": st.version,
                    "generation": 1,
                    "framed": want_framed,
                    "methods": st.methods,
                    "features": st.features,
                    "keeps_running_after_logout": true
                }
            });
            (Some(resp), want_framed, None)
        }
        "server.status" => (
            Some(json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "core": { "id": st.core_id, "name": st.core_name },
                    "version": st.version,
                    "pid": 12345,
                    "uptime_ms": 5000,
                    "live": st.terminals.iter().filter(|t| t["state"] == "live").count(),
                    "finished": st.terminals.iter().filter(|t| t["state"] == "finished").count()
                }
            })),
            false,
            None,
        ),
        "terminal.list" => (
            Some(json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "terminals": st.terminals,
                    "hidden_finished": 0,
                    "machines": st.machines
                }
            })),
            false,
            None,
        ),
        "terminal.get" => {
            let addr = params
                .get("terminal")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if let Some(t) = st
                .terminals
                .iter()
                .find(|t| t["address"] == addr || t["name"] == addr)
            {
                (
                    Some(json!({ "jsonrpc": "2.0", "id": id, "result": t })),
                    false,
                    None,
                )
            } else {
                (
                    Some(json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "error": { "code": -32000, "message": "not_found", "data": { "kind": "not_found" } }
                    })),
                    false,
                    None,
                )
            }
        }
        "terminal.run" => {
            let num = st.next_term_num;
            st.next_term_num += 1;
            let addr = format!("t{num}");
            let argv = params.get("argv").cloned().unwrap_or_else(|| json!(["sh"]));
            let mut t = make_terminal(&addr, "sh");
            t["argv"] = argv;
            if let Some(labels) = params.get("labels") {
                t["labels"] = labels.clone();
            }
            if let Some(name) = params.get("name") {
                t["name"] = name.clone();
            }
            if let Some(size) = params.get("size") {
                if let Some(cols) = size.get("cols").and_then(|v| v.as_u64()) {
                    t["cols"] = json!(cols);
                }
                if let Some(rows) = size.get("rows").and_then(|v| v.as_u64()) {
                    t["rows"] = json!(rows);
                }
            }
            st.terminals.push(t.clone());
            (
                Some(json!({ "jsonrpc": "2.0", "id": id, "result": t })),
                false,
                None,
            )
        }
        "terminal.kill" => {
            let addr = params
                .get("terminal")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if let Some(t) = st.terminals.iter_mut().find(|t| t["address"] == addr) {
                t["state"] = json!("finished");
                t["reason"] = json!("killed");
                let res = t.clone();
                (
                    Some(json!({ "jsonrpc": "2.0", "id": id, "result": res })),
                    false,
                    None,
                )
            } else {
                (
                    Some(json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "error": { "code": -32000, "message": "not_found", "data": { "kind": "not_found" } }
                    })),
                    false,
                    None,
                )
            }
        }
        "terminal.rm" => {
            let addr = params
                .get("terminal")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if st
                .terminals
                .iter()
                .any(|t| t["address"] == addr && t["state"] == "live")
            {
                (
                    Some(json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "error": { "code": -32000, "message": "live", "data": { "kind": "live" } }
                    })),
                    false,
                    None,
                )
            } else {
                st.terminals.retain(|t| t["address"] != addr);
                (
                    Some(json!({ "jsonrpc": "2.0", "id": id, "result": { "removed": addr } })),
                    false,
                    None,
                )
            }
        }
        "terminal.label" => {
            let addr = params
                .get("terminal")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if let Some(t) = st.terminals.iter_mut().find(|t| t["address"] == addr) {
                if let Some(new_labels) = params.get("labels").and_then(|l| l.as_object()) {
                    if let Some(obj) = t["labels"].as_object_mut() {
                        for (k, v) in new_labels {
                            if v.is_null() {
                                obj.remove(k);
                            } else {
                                obj.insert(k.clone(), v.clone());
                            }
                        }
                    }
                }
                let res = t.clone();
                (
                    Some(json!({ "jsonrpc": "2.0", "id": id, "result": res })),
                    false,
                    None,
                )
            } else {
                (
                    Some(json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "error": { "code": -32000, "message": "not_found", "data": { "kind": "not_found" } }
                    })),
                    false,
                    None,
                )
            }
        }
        "templates.list" => (
            Some(json!({ "jsonrpc": "2.0", "id": id, "result": { "templates": st.templates } })),
            false,
            None,
        ),
        "machine.list" => (
            Some(json!({ "jsonrpc": "2.0", "id": id, "result": { "machines": st.machines } })),
            false,
            None,
        ),
        "doc.get" => {
            let name = params.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let (val, ver) = st.docs.get(name).cloned().unwrap_or((Value::Null, 0));
            (
                Some(
                    json!({ "jsonrpc": "2.0", "id": id, "result": { "value": val, "version": ver } }),
                ),
                false,
                None,
            )
        }
        "doc.set" => {
            let name = params
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let version = params.get("version").and_then(|v| v.as_u64()).unwrap_or(0);
            let value = params.get("value").cloned().unwrap_or(Value::Null);
            let cur_ver = st.docs.get(&name).map(|(_, v)| *v).unwrap_or(0);
            if cur_ver != version {
                (
                    Some(json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "error": { "code": -32000, "message": "version conflict", "data": { "kind": "conflict" } }
                    })),
                    false,
                    None,
                )
            } else {
                let new_ver = cur_ver + 1;
                st.docs.insert(name, (value, new_ver));
                (
                    Some(json!({ "jsonrpc": "2.0", "id": id, "result": { "version": new_ver } })),
                    false,
                    None,
                )
            }
        }
        "events.subscribe" => {
            let (tx, rx) = mpsc::unbounded_channel();
            st.subscribers.push(tx);
            (
                Some(json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "channel": Value::Null,
                        "seq": st.seq,
                        "terminals": st.terminals
                    }
                })),
                false,
                Some(rx),
            )
        }
        _ => (
            Some(json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": { "code": -32601, "message": format!("method not found: {method}") }
            })),
            false,
            None,
        ),
    }
}

pub fn make_terminal(address: &str, cmd: &str) -> Value {
    json!({
        "address": address,
        "argv": [cmd],
        "cols": 80,
        "created_ms": 1700000000000u64,
        "cwd": "/tmp",
        "die_with_parent": false,
        "exit_code": null,
        "finished_ms": null,
        "identity": "ghostty",
        "keep": false,
        "labels": {},
        "name": null,
        "notify": false,
        "parent": null,
        "pid": 1000,
        "reason": null,
        "reported": false,
        "rm": false,
        "rows": 24,
        "screen": "exact",
        "signal": null,
        "state": "live",
        "status": null,
        "status_ms": null,
        "template": null,
        "title": "",
        "turn": 0
    })
}
