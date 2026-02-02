mod common;

use common::{CommandOutput, TestContext};

#[test]
#[cfg(feature = "e2e")]
fn e2e_run_nektos_act_version() {
    let ctx = TestContext::new();

    // Example: tooler run nektos/act@v0.2.79 --version
    let output: CommandOutput = ctx
        .cmd()
        .args(["run", "nektos/act@v0.2.79", "--version"])
        .output()
        .expect("Failed to run tooler")
        .into();

    output
        .assert_success()
        .assert_stdout_contains("act version 0.2.79");
}

#[test]
#[cfg(feature = "e2e")]
fn e2e_run_infisical_complex_tag() {
    let ctx = TestContext::new();

    // Example: tooler run infisical/infisical@infisical-cli/v0.41.90 --version
    let output: CommandOutput = ctx
        .cmd()
        .args([
            "run",
            "infisical/infisical@infisical-cli/v0.41.90",
            "--version",
        ])
        .output()
        .expect("Failed to run tooler")
        .into();

    output.assert_success().assert_stdout_contains("0.41.90");
}

#[test]
#[cfg(feature = "e2e")]
fn e2e_config_lifecycle() {
    let ctx = TestContext::new();

    // Example: tooler config show
    let _: CommandOutput = ctx
        .cmd()
        .args(["config", "show"])
        .output()
        .expect("Failed to run config show")
        .into();

    // Example: tooler config set auto-shim=true
    let _: CommandOutput = ctx
        .cmd()
        .args(["config", "set", "auto-shim=true"])
        .output()
        .expect("Failed to set config")
        .into();

    // Example: tooler config set auto-update true
    let _: CommandOutput = ctx
        .cmd()
        .args(["config", "set", "auto-update", "true"])
        .output()
        .expect("Failed to set config")
        .into();

    // Example: tooler config get update-check-days
    let output: CommandOutput = ctx
        .cmd()
        .args(["config", "get", "update-check-days"])
        .output()
        .expect("Failed to get config")
        .into();
    output.assert_success().assert_stdout_contains("60"); // default

    // Example: tooler config unset auto-shim
    let _: CommandOutput = ctx
        .cmd()
        .args(["config", "unset", "auto-shim"])
        .output()
        .expect("Failed to unset config")
        .into();
}

#[test]
#[cfg(feature = "e2e")]
fn e2e_run_with_asset_selection() {
    let ctx = TestContext::new();

    // Example: tooler run argoproj/argo-cd --asset argocd-linux-amd64
    // We only run this on linux amd64 as specified in the example
    if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        let output: CommandOutput = ctx
            .cmd()
            .args([
                "run",
                "argoproj/argo-cd",
                "--asset",
                "argocd-linux-amd64",
                "version",
                "--client",
            ])
            .output()
            .expect("Failed to run tooler with asset")
            .into();

        output.assert_success().assert_stdout_contains("argocd:");
    }
}

#[test]
#[cfg(feature = "e2e")]
fn e2e_run_short_name_after_install() {
    let ctx = TestContext::new();

    // First install nektos/act
    let _: CommandOutput = ctx
        .cmd()
        .args(["run", "nektos/act@v0.2.79", "--version"])
        .output()
        .expect("Failed to install act")
        .into();

    // Example: tooler run act --help
    let output: CommandOutput = ctx
        .cmd()
        .args(["run", "act", "--help"])
        .output()
        .expect("Failed to run tooler by short name")
        .into();

    output.assert_success().assert_stdout_contains("act");
}

#[test]
#[cfg(feature = "e2e")]
fn e2e_run_without_version_defaults_to_latest() {
    let ctx = TestContext::new();

    // Test running without version (should default to latest)
    // This verifies the fix for the "default" version bug
    let output: CommandOutput = ctx
        .cmd()
        .args(["run", "Yakitrak/obsidian-cli", "--version"])
        .output()
        .expect("Failed to run tooler without explicit version")
        .into();

    output
        .assert_success()
        .assert_stdout_contains("obsidian-cli version");
}
