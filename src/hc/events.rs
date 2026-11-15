use std::path::Path;
use std::time::Duration;

use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::unix::{OwnedReadHalf, OwnedWriteHalf};

use crate::hc::error::{HcError, HcResult};
use crate::hc::rpc::extract_rpc_result;
use crate::hc::transport::connect_unix;
use crate::hc::types::{EventPayload, HeartbeatPayload, MissedPayload, SubscribeResult};

pub const DEFAULT_HEARTBEAT_TIMEOUT: Duration = Duration::from_secs(45);

#[derive(Debug, Clone, PartialEq)]
pub enum EventMessage {
    Event(EventPayload),
    Heartbeat(HeartbeatPayload),
    Missed(MissedPayload),
}

pub struct EventStream {
    reader: BufReader<OwnedReadHalf>,
    _writer: OwnedWriteHalf,
    last_seq: u64,
    heartbeat_timeout: Duration,
}

impl EventStream {
    pub async fn subscribe(socket_path: &Path) -> HcResult<(SubscribeResult, Self)> {
        Self::subscribe_with_timeout(socket_path, DEFAULT_HEARTBEAT_TIMEOUT).await
    }

    pub async fn subscribe_with_timeout(
        socket_path: &Path,
        heartbeat_timeout: Duration,
    ) -> HcResult<(SubscribeResult, Self)> {
        let stream = connect_unix(socket_path).await?;
        let (read_half, mut write_half) = stream.into_split();
        let mut reader = BufReader::new(read_half);

        // 1. hello
        let hello_req = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "hello",
            "params": { "generation": 1 }
        });
        let mut bytes = serde_json::to_vec(&hello_req)?;
        bytes.push(b'\n');
        write_half.write_all(&bytes).await?;
        write_half.flush().await?;

        let mut line = String::new();
        if reader.read_line(&mut line).await? == 0 {
            return Err(HcError::Protocol("closed during event hello".to_string()));
        }
        let hello_resp: Value = serde_json::from_str(line.trim())?;
        let _ = extract_rpc_result(&hello_resp)?;

        // 2. events.subscribe
        let sub_req = json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "events.subscribe",
            "params": {}
        });
        let mut sub_bytes = serde_json::to_vec(&sub_req)?;
        sub_bytes.push(b'\n');
        write_half.write_all(&sub_bytes).await?;
        write_half.flush().await?;

        line.clear();
        if reader.read_line(&mut line).await? == 0 {
            return Err(HcError::Protocol("closed during subscribe".to_string()));
        }
        let sub_resp: Value = serde_json::from_str(line.trim())?;
        let result_val = extract_rpc_result(&sub_resp)?;
        let snapshot: SubscribeResult = serde_json::from_value(result_val)?;
        let last_seq = snapshot.seq;

        Ok((
            snapshot,
            Self {
                reader,
                _writer: write_half,
                last_seq,
                heartbeat_timeout,
            },
        ))
    }

    pub fn last_seq(&self) -> u64 {
        self.last_seq
    }

    pub async fn next_event(&mut self) -> HcResult<Option<EventMessage>> {
        let mut line = String::new();
        loop {
            line.clear();
            let read_fut = self.reader.read_line(&mut line);
            let n = match tokio::time::timeout(self.heartbeat_timeout, read_fut).await {
                Ok(res) => res?,
                Err(_) => return Err(HcError::HeartbeatTimeout),
            };
            if n == 0 {
                return Ok(None);
            }

            let val: Value = match serde_json::from_str(line.trim()) {
                Ok(v) => v,
                Err(_) => continue,
            };
            let method = val.get("method").and_then(|m| m.as_str()).unwrap_or("");
            let params = val.get("params").cloned().unwrap_or(Value::Null);

            match method {
                "event" => {
                    let ev: EventPayload = serde_json::from_value(params)?;
                    if ev.seq <= self.last_seq {
                        continue;
                    }
                    self.last_seq = ev.seq;
                    return Ok(Some(EventMessage::Event(ev)));
                }
                "heartbeat" => {
                    let hb: HeartbeatPayload = serde_json::from_value(params)?;
                    return Ok(Some(EventMessage::Heartbeat(hb)));
                }
                "missed" => {
                    let missed: MissedPayload = serde_json::from_value(params)?;
                    if missed.seq > self.last_seq {
                        self.last_seq = missed.seq;
                    }
                    return Ok(Some(EventMessage::Missed(missed)));
                }
                _ => continue,
            }
        }
    }
}
