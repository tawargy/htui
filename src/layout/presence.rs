use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::hc::error::{HcErrorKind, HcResult};
use crate::hc::rpc::RpcClient;

pub const PRESENCE_DOC_NAME: &str = "htui.presence";
pub const LEASE_STALE_MS: u64 = 60_000;
pub const FOREIGN_RESIZE_INHIBIT_MS: u64 = 10_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PresenceLease {
    pub instance_id: String,
    pub terminal: String,
    pub updated_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct PresenceDoc {
    pub leases: HashMap<String, PresenceLease>,
}

pub struct PresenceManager {
    pub instance_id: String,
}

impl PresenceManager {
    pub fn new(instance_id: String) -> Self {
        Self { instance_id }
    }

    pub fn can_auto_promote(
        &self,
        doc: &PresenceDoc,
        terminal: &str,
        now_ms: u64,
        last_foreign_size_ms: Option<u64>,
    ) -> bool {
        if let Some(sz_ms) = last_foreign_size_ms {
            if now_ms.saturating_sub(sz_ms) <= FOREIGN_RESIZE_INHIBIT_MS {
                return false;
            }
        }

        for lease in doc.leases.values() {
            if lease.terminal == terminal
                && lease.instance_id != self.instance_id
                && now_ms.saturating_sub(lease.updated_ms) <= LEASE_STALE_MS
            {
                return false;
            }
        }

        true
    }

    pub async fn acquire_or_renew(
        &self,
        rpc: &RpcClient,
        terminal: &str,
        now_ms: u64,
    ) -> HcResult<()> {
        for _ in 0..3 {
            let cur = rpc.doc_get(PRESENCE_DOC_NAME, None).await?;
            let mut doc: PresenceDoc = if cur.value.is_null() {
                PresenceDoc::default()
            } else {
                serde_json::from_value(cur.value).unwrap_or_default()
            };
            doc.leases
                .retain(|_, l| now_ms.saturating_sub(l.updated_ms) <= LEASE_STALE_MS);
            let key = format!("{}:{terminal}", self.instance_id);
            doc.leases.insert(
                key,
                PresenceLease {
                    instance_id: self.instance_id.clone(),
                    terminal: terminal.to_string(),
                    updated_ms: now_ms,
                },
            );
            let val: Value = serde_json::to_value(&doc)?;
            match rpc.doc_set(PRESENCE_DOC_NAME, val, cur.version, None).await {
                Ok(_) => return Ok(()),
                Err(e) if e.kind() == HcErrorKind::Conflict => continue,
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }

    pub async fn release_all(&self, rpc: &RpcClient) -> HcResult<()> {
        for _ in 0..3 {
            let cur = rpc.doc_get(PRESENCE_DOC_NAME, None).await?;
            if cur.value.is_null() {
                return Ok(());
            }
            let mut doc: PresenceDoc = serde_json::from_value(cur.value).unwrap_or_default();
            doc.leases.retain(|_, l| l.instance_id != self.instance_id);
            let val: Value = serde_json::to_value(&doc)?;
            match rpc.doc_set(PRESENCE_DOC_NAME, val, cur.version, None).await {
                Ok(_) => return Ok(()),
                Err(e) if e.kind() == HcErrorKind::Conflict => continue,
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }
}
