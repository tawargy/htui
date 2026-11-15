use std::io::Write;
use std::path::Path;

use serde_json::json;

use crate::hc::error::HcErrorKind;
use crate::hc::events::{EventMessage, EventStream};

pub async fn run_events(
    socket_path: &Path,
    json_mode: bool,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> i32 {
    let (snap, mut stream) = match EventStream::subscribe(socket_path).await {
        Ok(res) => res,
        Err(e) => {
            let _ = writeln!(err, "error: {e}");
            return if e.kind() == HcErrorKind::SocketUnreachable {
                3
            } else {
                1
            };
        }
    };

    let snap_line = if json_mode {
        serde_json::to_string(
            &json!({ "type": "snapshot", "seq": snap.seq, "terminals": snap.terminals }),
        )
        .unwrap_or_default()
    } else {
        format!(
            "snapshot seq={} terminals={}",
            snap.seq,
            snap.terminals.len()
        )
    };
    if let Err(e) = writeln!(out, "{snap_line}") {
        if e.kind() == std::io::ErrorKind::BrokenPipe {
            return 0;
        }
        return 1;
    }
    let _ = out.flush();

    loop {
        match stream.next_event().await {
            Ok(Some(msg)) => {
                let line = match msg {
                    EventMessage::Event(ev) => {
                        if json_mode {
                            serde_json::to_string(&ev).unwrap_or_default()
                        } else {
                            format!(
                                "event seq={} type={} terminal={}",
                                ev.seq,
                                ev.event_type,
                                ev.terminal.as_deref().unwrap_or("-")
                            )
                        }
                    }
                    EventMessage::Heartbeat(hb) => {
                        if json_mode {
                            serde_json::to_string(&json!({ "type": "heartbeat", "seq": hb.seq }))
                                .unwrap_or_default()
                        } else {
                            format!("heartbeat seq={}", hb.seq)
                        }
                    }
                    EventMessage::Missed(m) => {
                        if json_mode {
                            serde_json::to_string(&json!({ "type": "missed", "seq": m.seq, "terminals": m.terminals }))
                                .unwrap_or_default()
                        } else {
                            format!("missed seq={} terminals={}", m.seq, m.terminals.len())
                        }
                    }
                };
                if let Err(e) = writeln!(out, "{line}") {
                    if e.kind() == std::io::ErrorKind::BrokenPipe {
                        return 0;
                    }
                    return 1;
                }
                let _ = out.flush();
            }
            Ok(None) => return 0,
            Err(e) => {
                let _ = writeln!(err, "error: {e}");
                return 1;
            }
        }
    }
}
