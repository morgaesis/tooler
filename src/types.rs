use serde::{Deserialize, Serialize};
use std::cmp::PartialEq;
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolInfo {
    pub tool_name: String,
    pub repo: String,
    pub version: String,
    pub executable_path: String,
    pub install_type: String,
    #[serde(default = "default_pinned")]
    pub pinned: bool,
    pub installed_at: String,
    pub last_accessed: String,
}

fn default_pinned() -> bool {
    false
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolerSettings {
    #[serde(default = "default_update_check_days")]
    pub update_check_days: i32,
    #[serde(default = "default_auto_shim")]
    pub auto_shim: bool,
    #[serde(default = "default_auto_update")]
    pub auto_update: bool,
    #[serde(default = "default_shim_dir")]
    pub shim_dir: String,
}

fn default_update_check_days() -> i32 {
    60
}
fn default_auto_shim() -> bool {
    false
}
fn default_auto_update() -> bool {
    true
}
fn default_shim_dir() -> String {
    dirs::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(".local")
        .join("share")
        .join("tooler")
        .join("shims")
        .to_string_lossy()
        .to_string()
}

impl Default for ToolerSettings {
    fn default() -> Self {
        Self {
            update_check_days: default_update_check_days(),
            auto_shim: default_auto_shim(),
            auto_update: default_auto_update(),
            shim_dir: default_shim_dir(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ToolerConfig {
    pub tools: HashMap<String, ToolInfo>,
    pub settings: ToolerSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlatformInfo {
    pub os: String,
    pub arch: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AssetInfo {
    pub name: String,
    pub download_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GitHubRelease {
    pub tag_name: String,
    pub assets: Vec<GitHubAsset>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GitHubAsset {
    pub name: String,
    pub browser_download_url: String,
}
