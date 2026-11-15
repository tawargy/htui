use std::path::{Path, PathBuf};

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Config {
    pub hc: HcConfig,
    pub ui: UiConfig,
    pub attach: AttachConfig,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum AttachMode {
    #[default]
    Auto,
    Active,
    Observe,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct AttachConfig {
    pub mode: AttachMode,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct HcConfig {
    pub socket: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct UiConfig {
    pub show_finished: bool,
    pub show_unassigned: bool,
    pub prefix_timeout_ms: u64,
    pub sidebar_width: u16,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            show_finished: true,
            show_unassigned: true,
            prefix_timeout_ms: 1500,
            sidebar_width: 30,
        }
    }
}

impl Config {
    pub fn config_dir() -> Option<PathBuf> {
        ProjectDirs::from("", "", "herdr-tui").map(|p| p.config_dir().to_path_buf())
    }

    pub fn config_file_path() -> Option<PathBuf> {
        Self::config_dir().map(|d| d.join("config.toml"))
    }

    pub fn layout_fallback_path() -> Option<PathBuf> {
        Self::config_dir().map(|d| d.join("layout.json"))
    }

    pub fn load_default() -> Self {
        match Self::config_file_path() {
            Some(path) => Self::load_from(&path),
            None => Self::default(),
        }
    }

    pub fn load_from(path: &Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(raw) => toml::from_str(&raw).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }
}
