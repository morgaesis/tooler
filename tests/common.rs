use std::path::PathBuf;
use std::process::{Command, Output};
use std::str;
use tempfile::TempDir;

// Test helper types and methods are conditionally compiled with the "e2e" feature,
// but we keep them in the module for discoverability and future test expansion.
// The warnings are suppressed to keep CI clean while maintaining the API for e2e tests.
#[allow(dead_code)]
pub struct TestContext {
    pub _temp_dir: TempDir,
    pub config_path: PathBuf,
    pub bin_dir: PathBuf,
    pub bin_path: PathBuf,
}

#[allow(dead_code)]
impl TestContext {
    pub fn new() -> Self {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let config_path = temp_dir.path().join("config.json");
        let bin_dir = temp_dir.path().join("bin");

        let bin_path = PathBuf::from(env!("CARGO_BIN_EXE_tooler"));

        Self {
            _temp_dir: temp_dir,
            config_path,
            bin_dir,
            bin_path,
        }
    }

    pub fn cmd(&self) -> Command {
        let mut cmd = Command::new(&self.bin_path);
        cmd.env("TOOLER_CONFIG_PATH", &self.config_path);
        cmd.env("TOOLER_BIN_DIR", &self.bin_dir);
        // We will set HOME and XDG_DATA_HOME to our temp dir to isolate data/config
        cmd.env("HOME", self._temp_dir.path());
        cmd.env("XDG_DATA_HOME", self._temp_dir.path().join("data"));
        cmd.env("XDG_CONFIG_HOME", self._temp_dir.path().join("config"));
        cmd
    }
}

// CommandOutput provides test assertion helpers for e2e tests.
// These methods are only used when the "e2e" feature is enabled,
// but we keep them available for test development and debugging.
#[allow(dead_code)]
pub struct CommandOutput {
    pub stdout: String,
    pub stderr: String,
    pub status: std::process::ExitStatus,
}

impl From<Output> for CommandOutput {
    fn from(output: Output) -> Self {
        Self {
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            status: output.status,
        }
    }
}

// Assertion helpers for e2e tests - used to verify command output.
// Only active when "e2e" feature is enabled, but kept available for future tests.
#[allow(dead_code)]
impl CommandOutput {
    pub fn assert_success(&self) -> &Self {
        if !self.status.success() {
            panic!(
                "Command failed with status {:?}\nstdout: {}\nstderr: {}",
                self.status.code(),
                self.stdout,
                self.stderr
            );
        }
        self
    }

    pub fn assert_stdout_contains(&self, text: &str) -> &Self {
        assert!(
            self.stdout.contains(text),
            "Stdout did not contain '{}'\nActual stdout: {}",
            text,
            self.stdout
        );
        self
    }
}
