use crate::types::*;
use anyhow::{Context, Result};

use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;

pub const APP_NAME: &str = "tooler";
pub const CONFIG_DIR_NAME: &str = "tooler";
pub const TOOLS_DIR_NAME: &str = "tools";
pub const CONFIG_FILE_NAME: &str = "config.json";

pub fn get_user_data_dir() -> Result<PathBuf> {
    if let Ok(env_path) = std::env::var("TOOLER_DATA_DIR") {
        let path = PathBuf::from(env_path);
        fs::create_dir_all(&path)?;
        return Ok(path);
    }
    #[cfg(windows)]
    let base_data_dir = dirs::data_local_dir()
        .ok_or_else(|| anyhow::anyhow!("Could not determine data directory"))?;
    #[cfg(not(windows))]
    let base_data_dir =
        dirs::data_dir().ok_or_else(|| anyhow::anyhow!("Could not determine data directory"))?;

    let path = base_data_dir.join(APP_NAME);
    tracing::debug!("User data directory: {}", path.display());
    fs::create_dir_all(&path)?;
    Ok(path)
}

pub fn get_user_config_dir() -> Result<PathBuf> {
    if let Ok(env_path) = std::env::var("TOOLER_CONFIG_DIR") {
        let path = PathBuf::from(env_path);
        fs::create_dir_all(&path)?;
        return Ok(path);
    }
    let path = dirs::config_dir()
        .ok_or_else(|| anyhow::anyhow!("Could not determine config directory"))?
        .join(CONFIG_DIR_NAME);
    fs::create_dir_all(&path)?;
    Ok(path)
}

pub fn get_tooler_config_file_path() -> Result<PathBuf> {
    if let Ok(env_path) =
        std::env::var("TOOLER_CONFIG").or_else(|_| std::env::var("TOOLER_CONFIG_PATH"))
    {
        return Ok(PathBuf::from(env_path));
    }
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
    load_tool_configs_from_path(&config_path, |name| std::env::var(name).ok())
}

fn load_tool_configs_from_path(
    config_path: &std::path::Path,
    environment: impl Fn(&str) -> Option<String>,
) -> Result<ToolerConfig> {
    let mut config = if config_path.exists() {
        let content = fs::read_to_string(config_path)
            .with_context(|| format!("Could not read config file at {}", config_path.display()))?;

        if content.trim().is_empty() {
            ToolerConfig::default()
        } else {
            match serde_json::from_str(&content) {
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
                            if !partial_config.settings.bin_dir.is_empty() {
                                default_config.settings.bin_dir = partial_config.settings.bin_dir;
                            }
                            default_config.settings.auto_shim = partial_config.settings.auto_shim;
                            default_config.settings.auto_update =
                                partial_config.settings.auto_update;
                        }
                        default_config
                    } else {
                        // Malformed JSON - fail with original error
                        return Err(e).with_context(|| "Could not parse config file as JSON");
                    }
                }
            }
        }
    } else {
        ToolerConfig::default()
    };

    apply_environment_overrides(&mut config, environment);

    Ok(config)
}

fn apply_environment_overrides(
    config: &mut ToolerConfig,
    environment: impl Fn(&str) -> Option<String>,
) {
    if let Some(days) = environment("TOOLER_UPDATE_CHECK_DAYS") {
        if let Ok(days) = days.parse::<i32>() {
            config.settings.update_check_days = days;
        }
    }

    if let Some(auto_shim) = environment("TOOLER_AUTO_SHIM") {
        config.settings.auto_shim = auto_shim.to_lowercase() == "true" || auto_shim == "1";
    }

    if let Some(auto_update) = environment("TOOLER_AUTO_UPDATE") {
        config.settings.auto_update = auto_update.to_lowercase() == "true" || auto_update == "1";
    }

    if let Some(parse_release_body) = environment("TOOLER_PARSE_RELEASE_BODY") {
        if let Some(policy) = ReleaseBodyPolicy::parse(&parse_release_body) {
            config.settings.parse_release_body = policy;
        }
    }

    if let Some(bin_dir) = environment("TOOLER_BIN_DIR") {
        config.settings.bin_dir = bin_dir;
    }
}

pub fn save_tool_configs(config: &ToolerConfig) -> Result<()> {
    save_tool_configs_to_path(config, &get_tooler_config_file_path()?)
}

pub fn save_tool_configs_to_path(config: &ToolerConfig, path: &std::path::Path) -> Result<()> {
    let config_dir = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("Invalid config path"))?;

    fs::create_dir_all(config_dir)?;

    let content = serde_json::to_string_pretty(config)?;
    atomically_replace_with(path, |file| {
        file.write_all(content.as_bytes())
            .with_context(|| format!("Could not write temporary config for {}", path.display()))
    })
}

fn atomically_replace_with(
    path: &std::path::Path,
    write_content: impl FnOnce(&mut File) -> Result<()>,
) -> Result<()> {
    let config_dir = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("Invalid config path"))?;
    let mut temporary = tempfile::NamedTempFile::new_in(config_dir).with_context(|| {
        format!(
            "Could not create temporary config in {}",
            config_dir.display()
        )
    })?;
    write_content(temporary.as_file_mut())?;
    temporary
        .as_file_mut()
        .sync_all()
        .with_context(|| format!("Could not sync temporary config for {}", path.display()))?;
    temporary
        .persist(path)
        .map_err(|error| error.error)
        .with_context(|| format!("Could not atomically replace config at {}", path.display()))?;

    #[cfg(unix)]
    File::open(config_dir)
        .and_then(|directory| directory.sync_all())
        .with_context(|| format!("Could not sync config directory {}", config_dir.display()))?;

    Ok(())
}

pub fn normalize_key(key: &str) -> String {
    let mut result = String::with_capacity(key.len());
    let mut last_was_separator = true;

    for c in key.chars() {
        if c == '-' || c == '_' {
            if !last_was_separator {
                result.push('_');
                last_was_separator = true;
            }
        } else if c.is_ascii_uppercase() {
            if !last_was_separator {
                result.push('_');
            }
            result.push(c.to_ascii_lowercase());
            last_was_separator = false;
        } else {
            result.push(c);
            last_was_separator = false;
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_key() {
        assert_eq!(normalize_key("update-check-days"), "update_check_days");
        assert_eq!(normalize_key("update_check_days"), "update_check_days");
        assert_eq!(normalize_key("updateCheckDays"), "update_check_days");
        assert_eq!(normalize_key("UpdateCheckDays"), "update_check_days");
        assert_eq!(normalize_key("autoShim"), "auto_shim");
        assert_eq!(normalize_key("auto-shim"), "auto_shim");
        assert_eq!(normalize_key("bin-dir"), "bin_dir");
        assert_eq!(normalize_key("bin_dir"), "bin_dir");
    }

    #[test]
    fn test_config_default() {
        let config = ToolerConfig::default();
        assert!(config.tools.is_empty());
        assert_eq!(config.settings.update_check_days, 60);
        assert!(config.settings.auto_shim);
        #[cfg(windows)]
        assert!(config.settings.bin_dir.contains("tooler"));
        #[cfg(not(windows))]
        assert!(config.settings.bin_dir.contains(".local"));
        assert_eq!(config.settings.parse_release_body, ReleaseBodyPolicy::Ask);
    }

    #[test]
    fn test_parse_release_body_legacy_booleans() {
        let always: ToolerConfig = serde_json::from_str(
            r#"{
                "tools": {},
                "aliases": {},
                "settings": {
                    "parse_release_body": true
                }
            }"#,
        )
        .unwrap();
        assert_eq!(
            always.settings.parse_release_body,
            ReleaseBodyPolicy::Always
        );

        let never: ToolerConfig = serde_json::from_str(
            r#"{
                "tools": {},
                "aliases": {},
                "settings": {
                    "parse_release_body": false
                }
            }"#,
        )
        .unwrap();
        assert_eq!(never.settings.parse_release_body, ReleaseBodyPolicy::Never);
    }

    #[test]
    fn test_env_overrides_apply_without_existing_config_file() {
        let temp_dir = tempfile::tempdir().unwrap();
        let config_path = temp_dir.path().join("config").join("config.json");
        let bin_dir = temp_dir.path().join("bin");

        let config = load_tool_configs_from_path(&config_path, |name| {
            (name == "TOOLER_BIN_DIR").then(|| bin_dir.to_string_lossy().into_owned())
        })
        .unwrap();
        assert_eq!(config.settings.bin_dir, bin_dir.to_string_lossy());
        assert!(!config_path.exists());
    }

    #[test]
    fn test_environment_overrides() {
        let mut config = ToolerConfig::default();
        apply_environment_overrides(&mut config, |name| {
            match name {
                "TOOLER_UPDATE_CHECK_DAYS" => Some("7"),
                "TOOLER_AUTO_SHIM" => Some("false"),
                "TOOLER_AUTO_UPDATE" => Some("TRUE"),
                "TOOLER_PARSE_RELEASE_BODY" => Some("never"),
                "TOOLER_BIN_DIR" => Some("custom-bin"),
                _ => None,
            }
            .map(str::to_owned)
        });
        assert_eq!(config.settings.update_check_days, 7);
        assert!(!config.settings.auto_shim);
        assert!(config.settings.auto_update);
        assert_eq!(config.settings.parse_release_body, ReleaseBodyPolicy::Never);
        assert_eq!(config.settings.bin_dir, "custom-bin");

        apply_environment_overrides(&mut config, |name| {
            matches!(
                name,
                "TOOLER_UPDATE_CHECK_DAYS" | "TOOLER_PARSE_RELEASE_BODY"
            )
            .then(|| "invalid".to_owned())
        });
        assert_eq!(config.settings.update_check_days, 7);
        assert_eq!(config.settings.parse_release_body, ReleaseBodyPolicy::Never);
        assert_eq!(config.settings.bin_dir, "custom-bin");
    }

    #[test]
    fn test_explicit_config_paths_are_independent() {
        let directory = tempfile::tempdir().unwrap();
        let sentinel_path = directory.path().join("caller.json");
        let sentinel = b"caller config must remain unchanged";
        fs::write(&sentinel_path, sentinel).unwrap();
        std::thread::scope(|scope| {
            for days in [3, 9] {
                let path = directory.path().join(format!("{days}/config.json"));
                scope.spawn(move || {
                    let mut config = ToolerConfig::default();
                    config.settings.update_check_days = days;
                    save_tool_configs_to_path(&config, &path).unwrap();
                    let loaded = load_tool_configs_from_path(&path, |_| None).unwrap();
                    assert_eq!(loaded.settings.update_check_days, days);
                    let overridden = load_tool_configs_from_path(&path, |name| {
                        (name == "TOOLER_UPDATE_CHECK_DAYS").then(|| "15".to_owned())
                    })
                    .unwrap();
                    assert_eq!(overridden.settings.update_check_days, 15);
                });
            }
        });
        assert_eq!(fs::read(&sentinel_path).unwrap(), sentinel);
    }

    #[test]
    fn test_failed_temporary_write_preserves_existing_config() {
        let temp_dir = tempfile::tempdir().unwrap();
        let config_path = temp_dir.path().join("config.json");
        let original = b"{\"authoritative\":true}\n";
        fs::write(&config_path, original).unwrap();

        let result = atomically_replace_with(&config_path, |file| {
            file.write_all(b"{\"partial\"")?;
            Err(anyhow::anyhow!("injected write failure"))
        });

        assert!(result.is_err());
        assert_eq!(fs::read(&config_path).unwrap(), original);
    }
}
