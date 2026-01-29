mod common;

use common::{CommandOutput, TestContext};
use std::fs;

#[test]
fn test_help_and_version() {
    let ctx = TestContext::new();

    // Test --help
    let output: CommandOutput = ctx
        .cmd()
        .arg("--help")
        .output()
        .expect("Failed to run tooler")
        .into();

    output
        .assert_success()
        .assert_stdout_contains("A CLI tool manager for GitHub Releases")
        .assert_stdout_contains("Usage: tooler");

    // Test version
    let output: CommandOutput = ctx
        .cmd()
        .arg("version")
        .output()
        .expect("Failed to run tooler")
        .into();

    output.assert_success().assert_stdout_contains("tooler");
}

#[test]
fn test_config_show_formats() {
    let ctx = TestContext::new();

    // Set a config value first to ensure there's something to show
    ctx.cmd()
        .args(["config", "set", "update-check-days", "42"])
        .output()
        .expect("Failed to set config");

    // Test JSON output
    let output: CommandOutput = ctx
        .cmd()
        .args(["config", "show", "--format", "json"])
        .output()
        .expect("Failed to run tooler")
        .into();

    output.assert_success();
    let _: serde_json::Value =
        serde_json::from_str(&output.stdout).expect("Output was not valid JSON");
    output.assert_stdout_contains("\"bin_dir\":");

    // Test YAML output
    let output: CommandOutput = ctx
        .cmd()
        .args(["config", "show", "--format", "yaml"])
        .output()
        .expect("Failed to run tooler")
        .into();

    output.assert_success();
    let _: serde_yaml::Value =
        serde_yaml::from_str(&output.stdout).expect("Output was not valid YAML");
    output.assert_stdout_contains("bin_dir:");
}

#[test]
fn test_auto_shim_creation() {
    let ctx = TestContext::new();

    // Create a dummy config entry to simulate an installed tool
    // We'll use 'tooler' itself as the executable for the dummy entry
    let bin_path = ctx.bin_path.to_str().unwrap();
    let config_content = format!(
        r#"{{
        "tools": {{
            "dummy/tool@latest": {{
                "tool_name": "dummy-tool",
                "repo": "dummy/tool",
                "version": "1.0.0",
                "executable_path": "{}",
                "install_type": "binary",
                "pinned": true,
                "installed_at": "2024-01-01T00:00:00Z",
                "last_accessed": "2024-01-01T00:00:00Z",
                "forge": "github"
            }}
        }},
        "settings": {{
            "update_check_days": 60,
            "auto_shim": true,
            "auto_update": true,
            "bin_dir": "{}"
        }}
    }}"#,
        bin_path,
        ctx.bin_dir.to_str().unwrap()
    );

    fs::write(&ctx.config_path, config_content).expect("Failed to write mock config");

    // Run the dummy tool, which should trigger shim creation
    // Note: Since it's pinned, it won't check for updates
    let output: CommandOutput = ctx
        .cmd()
        .args(["run", "dummy-tool", "version"])
        .output()
        .expect("Failed to run dummy tool")
        .into();

    output.assert_success();

    // Verify shim script exists
    let shim_script = ctx.bin_dir.join("tooler-shim");
    assert!(
        shim_script.exists(),
        "tooler-shim script was not created at {}",
        shim_script.display()
    );

    // Verify tool symlink exists
    let tool_shim = ctx.bin_dir.join("dummy-tool");
    assert!(
        tool_shim.exists(),
        "tool shim symlink was not created at {}",
        tool_shim.display()
    );
}

#[test]
fn test_update_check_triggers() {
    let ctx = TestContext::new();

    // Mock a stale tool (last_accessed > 10 days ago, with settings.update_check_days = 5)
    let bin_path = ctx.bin_path.to_str().unwrap();
    let config_content = format!(
        r#"{{
        "tools": {{
            "derailed/k9s@latest": {{
                "tool_name": "k9s",
                "repo": "derailed/k9s",
                "version": "0.50.0",
                "executable_path": "{}",
                "install_type": "binary",
                "pinned": false,
                "installed_at": "2024-01-01T00:00:00Z",
                "last_accessed": "2024-01-01T00:00:00Z",
                "forge": "github"
            }}
        }},
        "settings": {{
            "update_check_days": 5,
            "auto_shim": true,
            "auto_update": false,
            "bin_dir": "{}"
        }}
    }}"#,
        bin_path,
        ctx.bin_dir.to_str().unwrap()
    );

    fs::write(&ctx.config_path, config_content).expect("Failed to write mock config");

    // Run with -v to see the update check log
    let output: CommandOutput = ctx
        .cmd()
        .env("TOOLER_UPDATE_CHECK_DAYS", "5") // Override the context default
        .args(["-v", "run", "k9s", "version", "--short"])
        .output()
        .expect("Failed to run tool")
        .into();

    // It should check for updates. Even if it fails (no network), it should log the attempt.
    output.assert_stdout_contains("Checking for tools not updated in >5 days");
    output.assert_stdout_contains("Checking for GitHub update for derailed/k9s");
}
