use serde::{Deserialize, Deserializer, Serialize};
use std::cmp::PartialEq;
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum Forge {
    #[serde(rename = "github")]
    #[default]
    GitHub,
    #[serde(rename = "url")]
    Url,
}

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
    #[serde(default)]
    pub last_checked: Option<String>,
    #[serde(default)]
    pub forge: Forge,
    #[serde(default)]
    pub original_url: Option<String>,
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
    #[serde(default = "default_bin_dir", alias = "shim_dir")]
    pub bin_dir: String,
    #[serde(
        default = "default_parse_release_body",
        deserialize_with = "deserialize_release_body_policy"
    )]
    pub parse_release_body: ReleaseBodyPolicy,
}

fn default_update_check_days() -> i32 {
    60
}
fn default_auto_shim() -> bool {
    true
}
fn default_auto_update() -> bool {
    true
}
fn default_bin_dir() -> String {
    #[cfg(windows)]
    {
        return dirs::data_local_dir()
            .unwrap_or_else(|| {
                dirs::home_dir()
                    .unwrap_or_else(|| std::path::PathBuf::from("."))
                    .join("AppData")
                    .join("Local")
            })
            .join("tooler")
            .join("bin")
            .to_string_lossy()
            .to_string();
    }

    #[cfg(not(windows))]
    {
        dirs::home_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join(".local")
            .join("share")
            .join("tooler")
            .join("bin")
            .to_string_lossy()
            .to_string()
    }
}
fn default_parse_release_body() -> ReleaseBodyPolicy {
    ReleaseBodyPolicy::Ask
}

impl Default for ToolerSettings {
    fn default() -> Self {
        Self {
            update_check_days: default_update_check_days(),
            auto_shim: default_auto_shim(),
            auto_update: default_auto_update(),
            bin_dir: default_bin_dir(),
            parse_release_body: default_parse_release_body(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum ReleaseBodyPolicy {
    Always,
    Never,
    #[default]
    Ask,
}

impl ReleaseBodyPolicy {
    pub fn parse(value: &str) -> Option<Self> {
        match value.to_ascii_lowercase().as_str() {
            "true" | "1" | "yes" | "y" | "always" => Some(Self::Always),
            "false" | "0" | "no" | "n" | "never" => Some(Self::Never),
            "ask" | "prompt" => Some(Self::Ask),
            _ => None,
        }
    }
}

impl std::fmt::Display for ReleaseBodyPolicy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Always => write!(f, "always"),
            Self::Never => write!(f, "never"),
            Self::Ask => write!(f, "ask"),
        }
    }
}

fn deserialize_release_body_policy<'de, D>(deserializer: D) -> Result<ReleaseBodyPolicy, D::Error>
where
    D: Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    match value {
        serde_json::Value::Bool(true) => Ok(ReleaseBodyPolicy::Always),
        serde_json::Value::Bool(false) => Ok(ReleaseBodyPolicy::Never),
        serde_json::Value::String(s) => ReleaseBodyPolicy::parse(&s)
            .ok_or_else(|| serde::de::Error::custom("expected ask, always, never, true, or false")),
        _ => Err(serde::de::Error::custom(
            "expected ask, always, never, true, or false",
        )),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ToolerConfig {
    pub tools: HashMap<String, ToolInfo>,
    #[serde(default)]
    pub aliases: HashMap<String, String>,
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
    #[serde(default)]
    pub body: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GitHubAsset {
    pub name: String,
    pub browser_download_url: String,
}
