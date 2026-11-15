use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::{mpsc, oneshot, Mutex};
use tokio::task::JoinHandle;

use crate::hc::error::{HcError, HcResult};
use crate::hc::transport::connect_unix;
use crate::hc::types::HelloResult;

const MANDATORY_METHODS: &[&str] = &["terminal.list", "terminal.attach", "events.subscribe"];

pub struct RpcClient {
    hello: HelloResult,
    next_id: AtomicU64,
    pending: Arc<Mutex<HashMap<u64, oneshot::Sender<HcResult<Value>>>>>,
    out_tx: mpsc::UnboundedSender<Vec<u8>>,
    reader_task: JoinHandle<()>,
    writer_task: JoinHandle<()>,
}

impl RpcClient {
    pub async fn connect(socket_path: &Path) -> HcResult<Self> {
        let stream = connect_unix(socket_path).await?;
        let (read_half, mut write_half) = stream.into_split();
        let mut reader = BufReader::new(read_half);

        // Perform synchronous hello handshake before spawning background tasks
        let hello_req = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "hello",
            "params": { "generation": 1 }
        });
        let mut req_bytes = serde_json::to_vec(&hello_req)?;
        req_bytes.push(b'\n');
        write_half.write_all(&req_bytes).await?;
        write_half.flush().await?;

        let mut line = String::new();
        let n = reader.read_line(&mut line).await?;
        if n == 0 {
            return Err(HcError::Protocol(
                "connection closed during hello handshake".to_string(),
            ));
        }

        let resp: Value = serde_json::from_str(line.trim())?;
        if let Some(err) = resp.get("error") {
            let code = err.get("code").and_then(|c| c.as_i64()).unwrap_or(-32000);
            let msg = err
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("hello failed")
                .to_string();
            let data = err.get("data").cloned();
            return Err(HcError::from_rpc_error(code, msg, data));
        }

        let hello: HelloResult = serde_json::from_value(
            resp.get("result")
                .cloned()
                .ok_or_else(|| HcError::Protocol("missing hello result".to_string()))?,
        )?;

        let missing: Vec<String> = MANDATORY_METHODS
            .iter()
            .filter(|m| !hello.methods.iter().any(|hm| hm == *m))
            .map(|m| (*m).to_string())
            .collect();
        if !missing.is_empty() {
            return Err(HcError::MissingMethods(missing));
        }

        let pending: Arc<Mutex<HashMap<u64, oneshot::Sender<HcResult<Value>>>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let (out_tx, mut out_rx) = mpsc::unbounded_channel::<Vec<u8>>();

        let writer_task = tokio::spawn(async move {
            while let Some(bytes) = out_rx.recv().await {
                if write_half.write_all(&bytes).await.is_err() {
                    break;
                }
                if write_half.flush().await.is_err() {
                    break;
                }
            }
        });

        let pending_reader = Arc::clone(&pending);
        let reader_task = tokio::spawn(async move {
            let mut buf = String::new();
            loop {
                buf.clear();
                match reader.read_line(&mut buf).await {
                    Ok(0) | Err(_) => {
                        let mut map = pending_reader.lock().await;
                        for (_, tx) in map.drain() {
                            let _ = tx
                                .send(Err(HcError::Protocol("RPC connection closed".to_string())));
                        }
                        break;
                    }
                    Ok(_) => {
                        let parsed: Value = match serde_json::from_str(buf.trim()) {
                            Ok(v) => v,
                            Err(_) => continue,
                        };
                        if let Some(id) = parsed.get("id").and_then(|v| v.as_u64()) {
                            let tx = pending_reader.lock().await.remove(&id);
                            if let Some(tx) = tx {
                                let res = extract_rpc_result(&parsed);
                                let _ = tx.send(res);
                            }
                        }
                    }
                }
            }
        });

        Ok(Self {
            hello,
            next_id: AtomicU64::new(2),
            pending,
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
        self.pending.lock().await.insert(id, tx);

        let req = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });
        let mut bytes = serde_json::to_vec(&req)?;
        bytes.push(b'\n');

        if self.out_tx.send(bytes).is_err() {
            self.pending.lock().await.remove(&id);
            return Err(HcError::Protocol("RPC writer closed".to_string()));
        }

        rx.await
            .map_err(|_| HcError::Protocol("RPC response channel dropped".to_string()))?
    }
}

impl Drop for RpcClient {
    fn drop(&mut self) {
        self.reader_task.abort();
        self.writer_task.abort();
    }
}

pub(crate) fn extract_rpc_result(msg: &Value) -> HcResult<Value> {
    if let Some(err) = msg.get("error") {
        let code = err.get("code").and_then(|c| c.as_i64()).unwrap_or(-32000);
        let message = err
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("RPC error")
            .to_string();
        let data = err.get("data").cloned();
        return Err(HcError::from_rpc_error(code, message, data));
    }
    Ok(msg.get("result").cloned().unwrap_or(Value::Null))
}
