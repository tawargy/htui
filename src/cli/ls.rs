use std::io::Write;
use std::path::Path;

use crate::cli::parse_label_filter;
use crate::hc::error::HcErrorKind;
use crate::hc::rpc::RpcClient;
use crate::model::address::TerminalAddress;
use crate::model::tree::TerminalEntry;

pub async fn run_ls(
    socket_path: &Path,
    json: bool,
    all: bool,
    space_filter: Option<&str>,
    label_filters: &[String],
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> i32 {
    let mut parsed_labels = Vec::new();
    for raw in label_filters {
        match parse_label_filter(raw) {
            Ok(kv) => parsed_labels.push(kv),
            Err(code) => {
                let _ = writeln!(err, "invalid --label parameter: {raw} (expected k=v)");
                return code;
            }
        }
    }

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

    let mut list = match client.terminal_list(all).await {
        Ok(l) => l,
        Err(e) => {
            let _ = writeln!(err, "error: {e}");
            return 1;
        }
    };

    list.terminals.retain(|t| {
        if let Some(sp) = space_filter {
            let entry = TerminalEntry {
                terminal: t.clone(),
            };
            if entry.space_name() != sp {
                return false;
            }
        }
        for (k, v) in &parsed_labels {
            if t.labels.get(k).map(String::as_str) != Some(v.as_str()) {
                return false;
            }
        }
        true
    });

    if json {
        match serde_json::to_string_pretty(&list) {
            Ok(s) => {
                if let Err(e) = writeln!(out, "{s}") {
                    if e.kind() == std::io::ErrorKind::BrokenPipe {
                        return 0;
                    }
                    return 1;
                }
            }
            Err(e) => {
                let _ = writeln!(err, "error: {e}");
                return 1;
            }
        }
        return 0;
    }

    for m in &list.machines {
        if m.state != "connected" {
            let _ = writeln!(err, "warning: machine '{}' is {}", m.name, m.state);
        }
    }

    let _ = writeln!(
        out,
        "{:<12} {:<14} {:<10} {:<18} {:<10} {:<10} {:<8}",
        "MACHINE", "SPACE", "ADDRESS", "NAME", "STATUS", "STATE", "AGE"
    );
    let local_name = client.hello().core.name.clone();
    for t in &list.terminals {
        let entry = TerminalEntry {
            terminal: t.clone(),
        };
        let parsed = TerminalAddress::parse(&t.address);
        let machine = parsed.machine.unwrap_or_else(|| local_name.clone());
        let space = entry.space_name();
        let name = entry.display_name();
        let status = entry.status_label();
        let state = &t.state;
        let age = "-";
        if let Err(e) = writeln!(
            out,
            "{:<12} {:<14} {:<10} {:<18} {:<10} {:<10} {:<8}",
            machine, space, t.address, name, status, state, age
        ) {
            if e.kind() == std::io::ErrorKind::BrokenPipe {
                return 0;
            }
            return 1;
        }
    }
    0
}
