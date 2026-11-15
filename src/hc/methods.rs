use std::collections::HashMap;

use serde_json::{json, Value};

use crate::hc::error::HcResult;
use crate::hc::rpc::RpcClient;
use crate::hc::types::{
    DocGetResult, DocSetResult, ListResult, MachineListResult, ReadResult, RemoveResult, RunParams,
    StatusResult, TemplatesResult, Terminal,
};

impl RpcClient {
    pub async fn server_status(&self) -> HcResult<StatusResult> {
        let val = self.call("server.status", json!({})).await?;
        Ok(serde_json::from_value(val)?)
    }

    pub async fn terminal_list(&self, all: bool) -> HcResult<ListResult> {
        let val = self.call("terminal.list", json!({ "all": all })).await?;
        Ok(serde_json::from_value(val)?)
    }

    pub async fn terminal_get(&self, terminal: &str) -> HcResult<Terminal> {
        let val = self
            .call("terminal.get", json!({ "terminal": terminal }))
            .await?;
        Ok(serde_json::from_value(val)?)
    }

    pub async fn terminal_run(&self, params: RunParams) -> HcResult<Terminal> {
        let val = self
            .call("terminal.run", serde_json::to_value(params)?)
            .await?;
        Ok(serde_json::from_value(val)?)
    }

    pub async fn terminal_kill(&self, terminal: &str) -> HcResult<Terminal> {
        let val = self
            .call("terminal.kill", json!({ "terminal": terminal }))
            .await?;
        Ok(serde_json::from_value(val)?)
    }

    pub async fn terminal_rm(&self, terminal: &str) -> HcResult<RemoveResult> {
        let val = self
            .call("terminal.rm", json!({ "terminal": terminal }))
            .await?;
        Ok(serde_json::from_value(val)?)
    }

    pub async fn terminal_keep(&self, terminal: &str, keep: bool) -> HcResult<Terminal> {
        let val = self
            .call(
                "terminal.keep",
                json!({ "terminal": terminal, "keep": keep }),
            )
            .await?;
        Ok(serde_json::from_value(val)?)
    }

    pub async fn terminal_label(
        &self,
        terminal: &str,
        labels: HashMap<String, Option<String>>,
    ) -> HcResult<Terminal> {
        let val = self
            .call(
                "terminal.label",
                json!({ "terminal": terminal, "labels": labels }),
            )
            .await?;
        Ok(serde_json::from_value(val)?)
    }

    pub async fn terminal_read(
        &self,
        terminal: &str,
        history: Option<usize>,
    ) -> HcResult<ReadResult> {
        let val = self
            .call(
                "terminal.read",
                json!({ "terminal": terminal, "history": history }),
            )
            .await?;
        Ok(serde_json::from_value(val)?)
    }

    pub async fn terminal_send(&self, terminal: &str, text: &str) -> HcResult<Value> {
        self.call(
            "terminal.send",
            json!({ "terminal": terminal, "text": text }),
        )
        .await
    }

    pub async fn terminal_keys(&self, terminal: &str, keys: Vec<String>) -> HcResult<Value> {
        self.call(
            "terminal.keys",
            json!({ "terminal": terminal, "keys": keys }),
        )
        .await
    }

    pub async fn terminal_interrupt(&self, terminal: &str) -> HcResult<Value> {
        self.call("terminal.interrupt", json!({ "terminal": terminal }))
            .await
    }

    pub async fn templates_list(&self) -> HcResult<TemplatesResult> {
        let val = self.call("templates.list", json!({})).await?;
        Ok(serde_json::from_value(val)?)
    }

    pub async fn machine_list(&self) -> HcResult<MachineListResult> {
        let val = self.call("machine.list", json!({})).await?;
        Ok(serde_json::from_value(val)?)
    }

    pub async fn doc_get(&self, name: &str, on: Option<&str>) -> HcResult<DocGetResult> {
        let val = self
            .call("doc.get", json!({ "name": name, "on": on }))
            .await?;
        Ok(serde_json::from_value(val)?)
    }

    pub async fn doc_set(
        &self,
        name: &str,
        value: Value,
        version: u64,
        on: Option<&str>,
    ) -> HcResult<DocSetResult> {
        let val = self
            .call(
                "doc.set",
                json!({ "name": name, "value": value, "version": version, "on": on }),
            )
            .await?;
        Ok(serde_json::from_value(val)?)
    }
}
