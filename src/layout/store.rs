use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

use crate::hc::error::{HcError, HcErrorKind, HcResult};
use crate::hc::rpc::RpcClient;
use crate::layout::panel::SpaceLayout;
use crate::layout::tree::SplitTree;

pub const LAYOUT_DOC_NAME: &str = "htui.layout";
const MAX_CAS_RETRIES: usize = 3;

#[derive(Deserialize)]
#[serde(untagged)]
enum SpaceLayoutWire {
    MultiPanel(SpaceLayout),
    LegacyTree(SplitTree),
}

impl From<SpaceLayoutWire> for SpaceLayout {
    fn from(wire: SpaceLayoutWire) -> Self {
        match wire {
            SpaceLayoutWire::MultiPanel(mut space) => {
                space.ensure_valid_active_panel();
                space
            }
            SpaceLayoutWire::LegacyTree(tree) => SpaceLayout::from_tree(tree),
        }
    }
}

fn deserialize_spaces_map<'de, D>(deserializer: D) -> Result<HashMap<String, SpaceLayout>, D::Error>
where
    D: Deserializer<'de>,
{
    let raw = HashMap::<String, SpaceLayoutWire>::deserialize(deserializer)?;
    Ok(raw.into_iter().map(|(k, v)| (k, v.into())).collect())
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct LayoutDoc {
    #[serde(deserialize_with = "deserialize_spaces_map")]
    pub spaces: HashMap<String, SpaceLayout>,
}

pub struct LayoutStore {
    fallback_path: PathBuf,
    use_doc_api: bool,
}

impl LayoutStore {
    pub fn new(fallback_path: PathBuf, hello_methods: &[String]) -> Self {
        let has_get = hello_methods.iter().any(|m| m == "doc.get");
        let has_set = hello_methods.iter().any(|m| m == "doc.set");
        Self {
            fallback_path,
            use_doc_api: has_get && has_set,
        }
    }

    pub fn uses_doc_api(&self) -> bool {
        self.use_doc_api
    }

    pub async fn load(&self, rpc: Option<&RpcClient>) -> HcResult<(LayoutDoc, u64)> {
        if self.use_doc_api {
            if let Some(client) = rpc {
                match client.doc_get(LAYOUT_DOC_NAME, None).await {
                    Ok(res) => {
                        let doc = if res.value.is_null() {
                            LayoutDoc::default()
                        } else {
                            serde_json::from_value(res.value).unwrap_or_default()
                        };
                        return Ok((doc, res.version));
                    }
                    Err(e) if matches!(e.kind(), HcErrorKind::Other(_)) => {}
                    Err(e) => return Err(e),
                }
            }
        }
        Ok((Self::load_local(&self.fallback_path), 0))
    }

    /// Saves changed spaces using CAS (`doc.get`/`doc.set`) with up to 3 retries on conflict,
    /// performing a per-space merge, and falling back to local file on exhausted retries or missing doc API.
    pub async fn save_space(
        &self,
        rpc: Option<&RpcClient>,
        space_key: &str,
        layout: &SpaceLayout,
    ) -> HcResult<u64> {
        self.mutate_doc(rpc, |doc| {
            doc.spaces.insert(space_key.to_string(), layout.clone());
        })
        .await
    }

    /// Removes `space_key` from `htui.layout` via CAS (`doc.get`/`doc.set`) and from the local fallback file.
    pub async fn remove_space(&self, rpc: Option<&RpcClient>, space_key: &str) -> HcResult<u64> {
        self.mutate_doc(rpc, |doc| {
            doc.spaces.remove(space_key);
        })
        .await
    }

    /// Atomically prunes any space in `removed_spaces` and any space whose `SpaceLayout` has no terminals.
    pub async fn prune_empty_spaces(
        &self,
        rpc: Option<&RpcClient>,
        removed_spaces: &HashSet<String>,
    ) -> HcResult<u64> {
        self.mutate_doc(rpc, |doc| {
            doc.spaces
                .retain(|sp, space| !removed_spaces.contains(sp) && space.has_terminals());
        })
        .await
    }

    async fn mutate_doc<F>(&self, rpc: Option<&RpcClient>, mut f: F) -> HcResult<u64>
    where
        F: FnMut(&mut LayoutDoc),
    {
        if self.use_doc_api {
            if let Some(client) = rpc {
                for _attempt in 0..MAX_CAS_RETRIES {
                    let cur = client.doc_get(LAYOUT_DOC_NAME, None).await?;
                    let mut doc: LayoutDoc = if cur.value.is_null() {
                        LayoutDoc::default()
                    } else {
                        serde_json::from_value(cur.value).unwrap_or_default()
                    };
                    f(&mut doc);
                    let val: Value = serde_json::to_value(&doc)?;
                    match client
                        .doc_set(LAYOUT_DOC_NAME, val, cur.version, None)
                        .await
                    {
                        Ok(res) => {
                            let mut local = Self::load_local(&self.fallback_path);
                            f(&mut local);
                            let _ = Self::save_local(&self.fallback_path, &local);
                            return Ok(res.version);
                        }
                        Err(e) if e.kind() == HcErrorKind::Conflict => continue,
                        Err(e) => return Err(e),
                    }
                }
            }
        }

        // Fallback to local file
        let mut local = Self::load_local(&self.fallback_path);
        f(&mut local);
        Self::save_local(&self.fallback_path, &local)?;
        Ok(0)
    }

    pub fn load_local(path: &Path) -> LayoutDoc {
        match std::fs::read_to_string(path) {
            Ok(raw) => serde_json::from_str(&raw).unwrap_or_default(),
            Err(_) => LayoutDoc::default(),
        }
    }

    pub fn save_local(path: &Path, doc: &LayoutDoc) -> HcResult<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(HcError::Io)?;
        }
        let raw = serde_json::to_string_pretty(doc)?;
        std::fs::write(path, raw).map_err(HcError::Io)?;
        Ok(())
    }
}
