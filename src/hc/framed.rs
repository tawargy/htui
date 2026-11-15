use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::sync::{mpsc, oneshot, Mutex};
use tokio::task::JoinHandle;

use crate::hc::error::{HcError, HcResult};
use crate::hc::rpc::extract_rpc_result;
use crate::hc::transport::connect_unix;
use crate::hc::types::{AttachResult, HelloResult};

/// Empirically verified frame kinds for `herdr-core` 0.1.21.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum FrameKind {
    Data = 0,
    Credit = 1,
    Close = 2,
    Resize = 3,
    Focus = 4,
    Facts = 5,
    AutomaticInput = 6,
    Unknown(u8),
}

impl From<u8> for FrameKind {
    fn from(v: u8) -> Self {
        match v {
            0 => Self::Data,
            1 => Self::Credit,
            2 => Self::Close,
            3 => Self::Resize,
            4 => Self::Focus,
            5 => Self::Facts,
            6 => Self::AutomaticInput,
            other => Self::Unknown(other),
        }
    }
}

impl From<FrameKind> for u8 {
    fn from(k: FrameKind) -> Self {
        match k {
            FrameKind::Data => 0,
            FrameKind::Credit => 1,
            FrameKind::Close => 2,
            FrameKind::Resize => 3,
            FrameKind::Focus => 4,
            FrameKind::Facts => 5,
            FrameKind::AutomaticInput => 6,
            FrameKind::Unknown(v) => v,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub channel: u32,
    pub kind: FrameKind,
    pub payload: Vec<u8>,
}

impl Frame {
    pub fn data(channel: u32, payload: Vec<u8>) -> Self {
        Self {
            channel,
            kind: FrameKind::Data,
            payload,
        }
    }

    pub fn credit(channel: u32, bytes: u32) -> Self {
        Self {
            channel,
            kind: FrameKind::Credit,
            payload: bytes.to_le_bytes().to_vec(),
        }
    }

    pub fn close(channel: u32) -> Self {
        Self {
            channel,
            kind: FrameKind::Close,
            payload: Vec::new(),
        }
    }

    pub fn resize(channel: u32, cols: u16, rows: u16) -> Self {
        let mut payload = Vec::with_capacity(4);
        payload.extend_from_slice(&cols.to_le_bytes());
        payload.extend_from_slice(&rows.to_le_bytes());
        Self {
            channel,
            kind: FrameKind::Resize,
            payload,
        }
    }

    pub fn parse_resize(&self) -> Option<(u16, u16)> {
        if self.kind != FrameKind::Resize || self.payload.len() < 4 {
            return None;
        }
        let cols = u16::from_le_bytes([self.payload[0], self.payload[1]]);
        let rows = u16::from_le_bytes([self.payload[2], self.payload[3]]);
        Some((cols, rows))
    }

    /// Encodes as 9-byte Little-Endian header + payload:
    /// `[channel: u32 LE][kind: u8][length: u32 LE][payload...]`
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(9 + self.payload.len());
        out.extend_from_slice(&self.channel.to_le_bytes());
        out.push(u8::from(self.kind));
        out.extend_from_slice(&(self.payload.len() as u32).to_le_bytes());
        out.extend_from_slice(&self.payload);
        out
    }
}

struct ChannelRegistry {
    active: HashMap<u32, mpsc::UnboundedSender<Frame>>,
    pending: HashMap<u32, Vec<Frame>>,
}

impl ChannelRegistry {
    fn new() -> Self {
        Self {
            active: HashMap::new(),
            pending: HashMap::new(),
        }
    }
}

pub struct ChannelHandle {
    pub channel: u32,
    pub rx: mpsc::UnboundedReceiver<Frame>,
    out_tx: mpsc::UnboundedSender<Frame>,
    channels: Arc<Mutex<ChannelRegistry>>,
}

impl ChannelHandle {
    pub fn send_data(&self, bytes: &[u8]) -> HcResult<()> {
        self.out_tx
            .send(Frame::data(self.channel, bytes.to_vec()))
            .map_err(|_| HcError::ChannelClosed(self.channel))
    }

    pub fn send_resize(&self, cols: u16, rows: u16) -> HcResult<()> {
        self.out_tx
            .send(Frame::resize(self.channel, cols, rows))
            .map_err(|_| HcError::ChannelClosed(self.channel))
    }

    pub fn send_credit(&self, bytes: u32) -> HcResult<()> {
        self.out_tx
            .send(Frame::credit(self.channel, bytes))
            .map_err(|_| HcError::ChannelClosed(self.channel))
    }

    pub async fn close(self) -> HcResult<()> {
        let mut reg = self.channels.lock().await;
        reg.active.remove(&self.channel);
        reg.pending.remove(&self.channel);
        drop(reg);
        self.out_tx
            .send(Frame::close(self.channel))
            .map_err(|_| HcError::ChannelClosed(self.channel))
    }
}

pub struct FramedClient {
    hello: HelloResult,
    next_id: AtomicU64,
    pending_rpc: Arc<Mutex<HashMap<u64, oneshot::Sender<HcResult<Value>>>>>,
    channels: Arc<Mutex<ChannelRegistry>>,
    out_tx: mpsc::UnboundedSender<Frame>,
    reader_task: JoinHandle<()>,
    writer_task: JoinHandle<()>,
}

impl FramedClient {
    pub async fn connect(socket_path: &Path) -> HcResult<Self> {
        let stream = connect_unix(socket_path).await?;
        let (read_half, mut write_half) = stream.into_split();
        let mut reader = BufReader::new(read_half);

        let hello_req = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "hello",
            "params": {
                "generation": 1,
                "framed": true,
                "features": ["resize_notifications"]
            }
        });
        let mut req_bytes = serde_json::to_vec(&hello_req)?;
        req_bytes.push(b'\n');
        write_half.write_all(&req_bytes).await?;
        write_half.flush().await?;

        let mut line = String::new();
        let n = reader.read_line(&mut line).await?;
        if n == 0 {
            return Err(HcError::Protocol(
                "connection closed during framed hello handshake".to_string(),
            ));
        }

        let resp: Value = serde_json::from_str(line.trim())?;
        let result_val = extract_rpc_result(&resp)?;
        let hello: HelloResult = serde_json::from_value(result_val)?;
        if !hello.framed {
            return Err(HcError::Protocol(
                "core declined framed mode in hello response".to_string(),
            ));
        }

        let pending_rpc: Arc<Mutex<HashMap<u64, oneshot::Sender<HcResult<Value>>>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let channels: Arc<Mutex<ChannelRegistry>> = Arc::new(Mutex::new(ChannelRegistry::new()));
        let (out_tx, mut out_rx) = mpsc::unbounded_channel::<Frame>();

        let writer_task = tokio::spawn(async move {
            while let Some(frame) = out_rx.recv().await {
                let bytes = frame.encode();
                if write_half.write_all(&bytes).await.is_err() {
                    break;
                }
                if write_half.flush().await.is_err() {
                    break;
                }
            }
        });

        let pending_reader = Arc::clone(&pending_rpc);
        let channels_reader = Arc::clone(&channels);
        let reader_task = tokio::spawn(async move {
            let mut hdr = [0u8; 9];
            loop {
                if reader.read_exact(&mut hdr).await.is_err() {
                    let mut map = pending_reader.lock().await;
                    for (_, tx) in map.drain() {
                        let _ = tx.send(Err(HcError::Protocol(
                            "framed connection closed".to_string(),
                        )));
                    }
                    let mut reg = channels_reader.lock().await;
                    reg.active.clear();
                    reg.pending.clear();
                    break;
                }
                let channel = u32::from_le_bytes([hdr[0], hdr[1], hdr[2], hdr[3]]);
                let kind = FrameKind::from(hdr[4]);
                let len = u32::from_le_bytes([hdr[5], hdr[6], hdr[7], hdr[8]]) as usize;
                let mut payload = vec![0u8; len];
                if len > 0 && reader.read_exact(&mut payload).await.is_err() {
                    break;
                }

                if channel == 0 && kind == FrameKind::Data {
                    if let Ok(parsed) = serde_json::from_slice::<Value>(&payload) {
                        if let Some(id) = parsed.get("id").and_then(|v| v.as_u64()) {
                            let tx = pending_reader.lock().await.remove(&id);
                            if let Some(tx) = tx {
                                let _ = tx.send(extract_rpc_result(&parsed));
                            }
                        }
                    }
                } else {
                    let is_close = kind == FrameKind::Close;
                    let frame = Frame {
                        channel,
                        kind,
                        payload,
                    };
                    let mut reg = channels_reader.lock().await;
                    if let Some(tx) = reg.active.get(&channel) {
                        let _ = tx.send(frame);
                    } else {
                        reg.pending.entry(channel).or_default().push(frame);
                    }
                    if is_close {
                        reg.active.remove(&channel);
                    }
                }
            }
        });

        Ok(Self {
            hello,
            next_id: AtomicU64::new(2),
            pending_rpc,
            channels,
            out_tx,
            reader_task,
            writer_task,
        })
    }

    pub fn hello(&self) -> &HelloResult {
        &self.hello
    }

    pub async fn call(&self, method: &str, params: Value) -> HcResult<Value> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        self.pending_rpc.lock().await.insert(id, tx);

        let req = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });
        let bytes = serde_json::to_vec(&req)?;
        if self.out_tx.send(Frame::data(0, bytes)).is_err() {
            self.pending_rpc.lock().await.remove(&id);
            return Err(HcError::Protocol("framed writer closed".to_string()));
        }

        rx.await
            .map_err(|_| HcError::Protocol("framed RPC response dropped".to_string()))?
    }

    pub async fn attach(
        &self,
        terminal: &str,
        cols: u16,
        rows: u16,
        observe: bool,
        resize_notifications: bool,
    ) -> HcResult<(AttachResult, ChannelHandle)> {
        let val = self
            .call(
                "terminal.attach",
                json!({
                    "terminal": terminal,
                    "cols": cols,
                    "rows": rows,
                    "observe": observe,
                    "resize_notifications": resize_notifications,
                    "history": 1000,
                }),
            )
            .await?;
        let res: AttachResult = serde_json::from_value(val)?;
        let (tx, rx) = mpsc::unbounded_channel();
        {
            let mut reg = self.channels.lock().await;
            if let Some(buffered) = reg.pending.remove(&res.channel) {
                for frame in buffered {
                    let _ = tx.send(frame);
                }
            }
            reg.active.insert(res.channel, tx);
        }
        let handle = ChannelHandle {
            channel: res.channel,
            rx,
            out_tx: self.out_tx.clone(),
            channels: Arc::clone(&self.channels),
        };
        Ok((res, handle))
    }
}

impl Drop for FramedClient {
    fn drop(&mut self) {
        self.reader_task.abort();
        self.writer_task.abort();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_roundtrip_9_byte_le_header() {
        let f = Frame::resize(7, 120, 40);
        let encoded = f.encode();
        assert_eq!(encoded.len(), 13);
        assert_eq!(&encoded[0..4], &7u32.to_le_bytes());
        assert_eq!(encoded[4], 3u8);
        assert_eq!(&encoded[5..9], &4u32.to_le_bytes());
        assert_eq!(f.parse_resize(), Some((120, 40)));
    }
}
