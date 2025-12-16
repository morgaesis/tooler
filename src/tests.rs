#[cfg(test)]
mod tests {
    use crate::config;
    use crate::platform;
    use crate::types::ToolerConfig;

    #[test]
    fn test_normalize_key() {
        assert_eq!(config::normalize_key("update-check-days"), "update_check_days");
        assert_eq!(config::normalize_key("autoShim"), "auto_shim");
        assert_eq!(config::normalize_key("shim-dir"), "shim_dir");
    }

    #[test]
    fn test_platform_info() {
        let info = platform::get_system_info();
        assert!(!info.os.is_empty());
        assert!(!info.arch.is_empty());
    }

    #[test]
    fn test_config_default() {
        let config = ToolerConfig::default();
        assert!(config.tools.is_empty());
        assert_eq!(config.settings.update_check_days, 60);
        assert!(!config.settings.auto_shim);
        assert!(config.settings.shim_dir.contains(".local"));
    }
}