use crate::types::*;
use anyhow::{Context, Result};

use std::fs;
use std::path::PathBuf;

pub const APP_NAME: &str = "tooler";
pub const CONFIG_DIR_NAME: &str = "tooler";
pub const TOOLS_DIR_NAME: &str = "tools";
pub const CONFIG_FILE_NAME: &str = "config.json";

pub fn get_user_data_dir() -> Result<PathBuf> {
    let path = dirs::data_dir()
        .ok_or_else(|| anyhow::anyhow!("Could not determine data directory"))?
        .join(APP_NAME);
    tracing::debug!("User data directory: {}", path.display());
    fs::create_dir_all(&path)?;
    Ok(path)
}

pub fn get_user_config_dir() -> Result<PathBuf> {
    let path = dirs::config_dir()
        .ok_or_else(|| anyhow::anyhow!("Could not determine config directory"))?
        .join(CONFIG_DIR_NAME);
    fs::create_dir_all(&path)?;
    Ok(path)
}

pub fn get_tooler_config_file_path() -> Result<PathBuf> {
    let path = get_user_config_dir()?.join(CONFIG_FILE_NAME);
    tracing::debug!("Config file path: {}", path.display());
    Ok(path)
}

pub fn get_tooler_tools_dir() -> Result<PathBuf> {
    let path = get_user_data_dir()?.join(TOOLS_DIR_NAME);
    tracing::debug!("Tools directory: {}", path.display());
    fs::create_dir_all(&path)?;
    Ok(path)
}

pub fn load_tool_configs() -> Result<ToolerConfig> {
    let config_path = get_tooler_config_file_path()?;

    if !config_path.exists() {
        return Ok(ToolerConfig::default());
    }

    let content = fs::read_to_string(&config_path)
        .with_context(|| format!("Could not read config file at {}", config_path.display()))?;

    let mut config: ToolerConfig = match serde_json::from_str(&content) {
        Ok(config) => config,
        Err(e) => {
            // Check if error is due to missing fields (partial config) vs malformed JSON
            if e.is_data() {
                // Missing fields - try to parse as partial config and merge with defaults
                let mut default_config = ToolerConfig::default();
                if let Ok(partial_config) = serde_json::from_str::<ToolerConfig>(&content) {
                    // Merge any valid settings from partial config
                    if partial_config.settings.update_check_days != 0 {
                        default_config.settings.update_check_days =
                            partial_config.settings.update_check_days;
                    }
                    if !partial_config.settings.shim_dir.is_empty() {
                        default_config.settings.shim_dir = partial_config.settings.shim_dir;
                    }
                    default_config.settings.auto_shim = partial_config.settings.auto_shim;
                }
                default_config
            } else {
                // Malformed JSON - fail with original error
                return Err(e).with_context(|| "Could not parse config file as JSON");
            }
        }
    };

    // Apply environment variable overrides
    if let Ok(days) = std::env::var("TOOLER_UPDATE_CHECK_DAYS") {
        if let Ok(days) = days.parse::<i32>() {
            config.settings.update_check_days = days;
        }
    }

    if let Ok(auto_shim) = std::env::var("TOOLER_AUTO_SHIM") {
        config.settings.auto_shim = auto_shim.to_lowercase() == "true" || auto_shim == "1";
    }

    if let Ok(shim_dir) = std::env::var("TOOLER_SHIM_DIR") {
        config.settings.shim_dir = shim_dir;
    }

    Ok(config)
}

pub fn save_tool_configs(config: &ToolerConfig) -> Result<()> {
    let config_path = get_tooler_config_file_path()?;
    let config_dir = config_path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("Invalid config path"))?;

    fs::create_dir_all(config_dir)?;

    let content = serde_json::to_string_pretty(config)?;
    fs::write(&config_path, content)?;

    Ok(())
}

pub fn normalize_key(key: &str) -> String {
    key.replace('-', "_")
        .chars()
        .map(|c| {
            if c.is_ascii_uppercase() {
                format!("_{}", c.to_lowercase())
            } else {
                c.to_string()
            }
        })
        .collect::<String>()
        .to_lowercase()
}
