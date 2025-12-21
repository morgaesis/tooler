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
    pub update_check_days: i32,
    pub auto_shim: bool,
    pub shim_dir: String,
}

impl Default for ToolerSettings {
    fn default() -> Self {
        Self {
            update_check_days: 60,
            auto_shim: false,
            shim_dir: dirs::home_dir()
                .unwrap_or_else(|| std::path::PathBuf::from("."))
                .join(".local")
                .join("bin")
                .to_string_lossy()
                .to_string(),
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
