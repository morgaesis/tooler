mod common;

use common::{CommandOutput, TestContext};
use std::fs;
use std::path::PathBuf;
#[cfg(not(windows))]
use std::process::Command;

fn write_dummy_tool(ctx: &TestContext, dir_name: &str, tool_name: &str) -> PathBuf {
    let dummy_tool_dir = ctx._temp_dir.path().join(dir_name);
    fs::create_dir_all(&dummy_tool_dir).expect("Failed to create dummy tool dir");

    #[cfg(windows)]
    let dummy_tool = dummy_tool_dir.join(format!("{tool_name}.cmd"));
    #[cfg(not(windows))]
    let dummy_tool = dummy_tool_dir.join(tool_name);

    #[cfg(windows)]
    fs::write(&dummy_tool, "@echo off\r\necho dummy tool %*\r\n")
        .expect("Failed to write dummy tool");

    #[cfg(not(windows))]
    {
        use std::os::unix::fs::PermissionsExt;

        fs::write(&dummy_tool, "#!/bin/sh\necho dummy tool \"$@\"\n")
            .expect("Failed to write dummy tool");
        let mut permissions = fs::metadata(&dummy_tool)
            .expect("Failed to stat dummy tool")
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&dummy_tool, permissions)
            .expect("Failed to make dummy tool executable");
    }

    dummy_tool
}

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
        .assert_stdout_contains("Usage: tooler")
        .assert_stdout_contains("--output <OUTPUTS>");

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
fn test_log_destination_writes_default_logfile() {
    let ctx = TestContext::new();
    let state_dir = ctx._temp_dir.path().join("state").join("tooler");
    let log_file = state_dir.join("tooler.log");

    let output: CommandOutput = ctx
        .cmd()
        .env("TOOLER_STATE_DIR", &state_dir)
        .args(["-v", "--output", "logfile", "version"])
        .output()
        .expect("Failed to run tooler")
        .into();

    output.assert_success().assert_stdout_contains("tooler");

    assert!(
        state_dir.exists(),
        "log directory was not created at {}",
        state_dir.display()
    );
    assert!(
        log_file.exists(),
        "log file was not created at {}",
        log_file.display()
    );

    let log_content = fs::read_to_string(&log_file).expect("Failed to read tooler log file");
    assert!(
        log_content.contains("Logging initialized"),
        "log file did not contain startup log event\nActual log content: {}",
        log_content
    );
}

#[test]
fn test_output_logfile_accepts_explicit_path() {
    let ctx = TestContext::new();
    let log_file = ctx._temp_dir.path().join("logs").join("tooler.log");
    let output_arg = format!("logfile={}", log_file.to_string_lossy());

    let output: CommandOutput = ctx
        .cmd()
        .args(["-v", "--output", &output_arg, "version"])
        .output()
        .expect("Failed to run tooler")
        .into();

    output.assert_success().assert_stdout_contains("tooler");

    let log_content = fs::read_to_string(&log_file).expect("Failed to read explicit log file");
    assert!(
        log_content.contains("Logging initialized"),
        "explicit log file did not contain startup log event\nActual log content: {}",
        log_content
    );
}

#[test]
fn test_auto_shim_creation() {
    let ctx = TestContext::new();
    let dummy_tool = write_dummy_tool(&ctx, "auto-shim-tools", "dummy-tool");

    // Create a dummy config entry to simulate an installed tool
    let config_content = serde_json::json!({
        "tools": {
            "dummy/tool@latest": {
                "tool_name": "dummy-tool",
                "repo": "dummy/tool",
                "version": "1.0.0",
                "executable_path": dummy_tool,
                "install_type": "binary",
                "pinned": true,
                "installed_at": "2024-01-01T00:00:00Z",
                "last_accessed": "2024-01-01T00:00:00Z",
                "forge": "github"
            }
        },
        "settings": {
            "update_check_days": 60,
            "auto_shim": true,
            "auto_update": true,
            "bin_dir": ctx.bin_dir
        }
    })
    .to_string();

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
    #[cfg(windows)]
    let shim_script = ctx.bin_dir.join("tooler-shim.cmd");
    #[cfg(not(windows))]
    let shim_script = ctx.bin_dir.join("tooler-shim");
    assert!(
        shim_script.exists(),
        "tooler-shim script was not created at {}",
        shim_script.display()
    );

    // Verify tool shim exists
    #[cfg(windows)]
    let tool_shim = ctx.bin_dir.join("dummy-tool.cmd");
    #[cfg(not(windows))]
    let tool_shim = ctx.bin_dir.join("dummy-tool");
    assert!(
        tool_shim.exists(),
        "tool shim was not created at {}",
        tool_shim.display()
    );

    #[cfg(not(windows))]
    {
        let shim_content =
            fs::read_to_string(&shim_script).expect("Failed to read generated shim script");
        assert!(
            shim_content.contains(ctx.bin_path.to_string_lossy().as_ref()),
            "tooler-shim should dispatch through the absolute tooler path that created it"
        );

        let output: CommandOutput = Command::new(&tool_shim)
            .env("TOOLER_CONFIG_PATH", &ctx.config_path)
            .env("TOOLER_BIN_DIR", &ctx.bin_dir)
            .env("HOME", ctx._temp_dir.path())
            .env("XDG_DATA_HOME", ctx._temp_dir.path().join("data"))
            .env("XDG_CONFIG_HOME", ctx._temp_dir.path().join("config"))
            .env("PATH", "/nonexistent")
            .arg("version")
            .output()
            .expect("Failed to run tool shim")
            .into();

        output
            .assert_success()
            .assert_stdout_contains("dummy tool version");

        let broken_content = shim_content.replace(
            ctx.bin_path.to_string_lossy().as_ref(),
            "/nonexistent/tooler",
        );
        fs::write(&shim_script, broken_content).expect("Failed to write broken shim script");
        let shim_log = ctx._temp_dir.path().join("shim.log");
        let output: CommandOutput = Command::new(&tool_shim)
            .env("TOOLER_CONFIG_PATH", &ctx.config_path)
            .env("TOOLER_BIN_DIR", &ctx.bin_dir)
            .env("HOME", ctx._temp_dir.path())
            .env("XDG_DATA_HOME", ctx._temp_dir.path().join("data"))
            .env("XDG_CONFIG_HOME", ctx._temp_dir.path().join("config"))
            .env("TOOLER_SHIM_LOG", &shim_log)
            .env("PATH", "/nonexistent")
            .arg("version")
            .output()
            .expect("Failed to run broken tool shim")
            .into();

        assert!(!output.status.success(), "broken shim should fail");
        output.assert_output_contains("details logged");
        let log_content = fs::read_to_string(&shim_log).expect("Failed to read shim log");
        assert!(log_content.contains("tool=dummy-tool"));
        assert!(log_content.contains("tooler-binary-missing-or-not-executable"));
    }
}

#[test]
fn test_info_respects_config_entry_when_binary_is_missing() {
    let ctx = TestContext::new();

    let missing_path = ctx
        ._temp_dir
        .path()
        .join("data/tools/github/infisical__infisical__arm64/infisical-cli/v0.41.90/infisical");
    let config_content = serde_json::json!({
        "tools": {
            "infisical/infisical@latest": {
                "tool_name": "infisical",
                "repo": "infisical/infisical",
                "version": "0.41.90",
                "executable_path": missing_path,
                "install_type": "archive",
                "pinned": false,
                "installed_at": "2026-04-24T14:52:57.383004330+00:00",
                "last_accessed": "2026-05-21T22:42:46.660429251+00:00",
                "last_checked": "2026-04-24T14:52:57.383007905+00:00",
                "forge": "github",
                "original_url": null
            }
        },
        "settings": {
            "update_check_days": 60,
            "auto_shim": true,
            "auto_update": true,
            "bin_dir": ctx.bin_dir
        }
    })
    .to_string();

    fs::write(&ctx.config_path, config_content).expect("Failed to write mock config");

    let output: CommandOutput = ctx
        .cmd()
        .args(["info", "infisical"])
        .output()
        .expect("Failed to run tooler info")
        .into();

    output
        .assert_success()
        .assert_stdout_contains("Repository:    infisical/infisical")
        .assert_stdout_contains("Version:       0.41.90")
        .assert_stdout_contains(&missing_path.to_string_lossy());
}

#[test]
#[cfg(unix)]
fn test_stale_direct_url_entry_keeps_url_reinstall_target() {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    let ctx = TestContext::new();
    let listener = TcpListener::bind("127.0.0.1:0").expect("failed to bind test server");
    let url = format!(
        "http://127.0.0.1:{}/kubectl",
        listener.local_addr().unwrap().port()
    );

    thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let mut request = [0_u8; 1024];
            let _ = stream.read(&mut request);
            let body = b"#!/bin/sh\necho kubectl fixture \"$@\"\n";
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: application/octet-stream\r\n\r\n",
                body.len()
            );
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.write_all(body);
        }
    });

    let missing_path = ctx
        ._temp_dir
        .path()
        .join("data/tools/url/direct__kubectl__arm64/v1.31.0/kubectl");
    let config_content = format!(
        r#"{{
        "tools": {{
            "kubectl@latest": {{
                "tool_name": "kubectl",
                "repo": "kubectl",
                "version": "v1.31.0",
                "executable_path": "{}",
                "install_type": "binary",
                "pinned": false,
                "installed_at": "2026-04-24T14:52:57Z",
                "last_accessed": "2026-05-21T22:42:46Z",
                "last_checked": "2026-04-24T14:52:57Z",
                "forge": "url",
                "original_url": "{}"
            }}
        }},
        "settings": {{
            "update_check_days": 0,
            "auto_shim": false,
            "auto_update": false,
            "bin_dir": "{}"
        }}
    }}"#,
        missing_path.to_string_lossy(),
        url,
        ctx.bin_dir.to_string_lossy()
    );

    fs::write(&ctx.config_path, config_content).expect("Failed to write mock config");

    let output: CommandOutput = ctx
        .cmd()
        .args(["run", &url, "--version"])
        .output()
        .expect("Failed to run tooler")
        .into();

    output
        .assert_success()
        .assert_stdout_contains("kubectl fixture --version");

    let output: CommandOutput = ctx
        .cmd()
        .args(["run", "kubectl", "version"])
        .output()
        .expect("Failed to run recovered tool")
        .into();

    output
        .assert_success()
        .assert_stdout_contains("kubectl fixture version");
}

#[test]
fn test_update_check_triggers() {
    let ctx = TestContext::new();
    let dummy_tool = write_dummy_tool(&ctx, "update-check-tools", "k9s");

    // Mock a stale tool (last_accessed > 10 days ago, with settings.update_check_days = 5)
    let config_content = serde_json::json!({
        "tools": {
            "derailed/k9s@latest": {
                "tool_name": "k9s",
                "repo": "derailed/k9s",
                "version": "0.50.0",
                "executable_path": dummy_tool,
                "install_type": "binary",
                "pinned": false,
                "installed_at": "2024-01-01T00:00:00Z",
                "last_accessed": "2024-01-01T00:00:00Z",
                "forge": "github"
            }
        },
        "settings": {
            "update_check_days": 5,
            "auto_shim": true,
            "auto_update": false,
            "bin_dir": ctx.bin_dir
        }
    })
    .to_string();

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
    output
        .assert_output_contains("Checking if derailed/k9s@latest needs update (threshold: 5 days)");
    output.assert_output_contains("Checking for GitHub update for derailed/k9s");
}
