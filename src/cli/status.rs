use std::io::Write;
use std::path::Path;

use serde_json::json;

use crate::hc::error::HcErrorKind;
use crate::hc::rpc::RpcClient;

pub async fn run_status(
    socket_path: &Path,
    json: bool,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> i32 {
    let client = match RpcClient::connect(socket_path).await {
        Ok(c) => c,
        Err(e) => {
            let _ = writeln!(err, "error: {e}");
            return if e.kind() == HcErrorKind::SocketUnreachable {
                3
            } else {
                1
            };
        }
    };

    let status = match client.server_status().await {
        Ok(s) => s,
        Err(e) => {
            let _ = writeln!(err, "error: {e}");
            return 1;
        }
    };

    let machines = client
        .machine_list()
        .await
        .map(|m| m.machines)
        .unwrap_or_default();

    if json {
        let payload = json!({
            "status": status,
            "machines": machines,
        });
        if let Ok(s) = serde_json::to_string_pretty(&payload) {
            if let Err(e) = writeln!(out, "{s}") {
                if e.kind() == std::io::ErrorKind::BrokenPipe {
                    return 0;
                }
                return 1;
            }
        }
        return 0;
    }

    let _ = writeln!(
        out,
        "core: {} ({}) v{} pid={} up={}s live={} finished={}",
        status.core.name,
        status.core.id,
        status.version,
        status.pid,
        status.uptime_ms / 1000,
        status.live,
        status.finished
    );
    for m in &machines {
        let _ = writeln!(out, "machine: {} ({}) state={}", m.name, m.target, m.state);
    }
    0
}
