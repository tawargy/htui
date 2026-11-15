use std::path::Path;

use crate::hc::error::HcErrorKind;
use crate::hc::rpc::RpcClient;
use crate::model::tree::TerminalEntry;

pub async fn validate_space_exists(
    socket_path: &Path,
    space: &str,
    create: bool,
) -> Result<(), i32> {
    let client = RpcClient::connect(socket_path).await.map_err(|e| {
        if e.kind() == HcErrorKind::SocketUnreachable {
            3
        } else {
            1
        }
    })?;
    if create {
        return Ok(());
    }
    let list = client.terminal_list(true).await.map_err(|_| 1i32)?;
    let exists = list.terminals.iter().any(|t| {
        let entry = TerminalEntry {
            terminal: t.clone(),
        };
        entry.space_name() == space
    });
    if exists {
        Ok(())
    } else {
        Err(4)
    }
}
