use std::fs;
use std::process::Command;
use tempfile::tempdir;

#[test]
fn test_e2e_self_healing() {
    let root = tempdir().expect("failed to create temp dir");
    let config_dir = root.path().join("config");
    let data_dir = root.path().join("data");

    fs::create_dir_all(&config_dir).unwrap();
    fs::create_dir_all(&data_dir).unwrap();

    // Create mock binary directory
    let arch = if cfg!(target_arch = "aarch64") {
        "arm64"
    } else {
        "amd64"
    };
    let tool_dir = data_dir.join(format!("tools/github/myauthor__mytool__{}/v1.0.0", arch));
    fs::create_dir_all(&tool_dir).unwrap();

    let binary_path = tool_dir.join("mytool");
    let binary_content = "#!/bin/bash\necho \"mytool version 1.0.0\"";
    fs::write(&binary_path, binary_content).unwrap();

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&binary_path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&binary_path, perms).unwrap();
    }

    // Run tooler via cargo run
    let output = Command::new("cargo")
        .args(["run", "--quiet", "--", "mytool", "--version"])
        .env("TOOLER_CONFIG", config_dir.join("config.json"))
        .env("TOOLER_DATA_DIR", &data_dir)
        .output()
        .expect("failed to execute tooler");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(stderr.contains("Recovered tool mytool (v1.0.0)"));
    assert!(stdout.contains("mytool version 1.0.0"));

    // Verify config was healed
    let healed_config_path = config_dir.join("config.json");
    let healed_content =
        fs::read_to_string(&healed_config_path).expect("failed to read healed config");
    assert!(
        healed_content.contains("myauthor/mytool@latest")
            || healed_content.contains("myauthor/mytool")
    );

    // Verify deduced install type
    assert!(healed_content.contains("\"install_type\": \"binary\""));
}

#[test]
fn test_no_aggressive_fuzzy_matching() {
    let root = tempdir().expect("failed to create temp dir");
    let config_dir = root.path().join("config");
    let data_dir = root.path().join("data");

    fs::create_dir_all(&config_dir).unwrap();
    fs::create_dir_all(&data_dir).unwrap();

    // Create minikube installation
    let arch = if cfg!(target_arch = "aarch64") {
        "arm64"
    } else {
        "amd64"
    };
    let tool_dir = data_dir.join(format!(
        "tools/github/kubernetes__minikube__{}/v1.31.0",
        arch
    ));
    fs::create_dir_all(&tool_dir).unwrap();
    fs::write(tool_dir.join("minikube"), "#!/bin/bash\necho minikube").unwrap();

    // Run with 'mkb' - should NOT match minikube
    let output = Command::new("cargo")
        .args(["run", "--quiet", "--", "mkb", "--version"])
        .env("TOOLER_CONFIG", config_dir.join("config.json"))
        .env("TOOLER_DATA_DIR", &data_dir)
        .output()
        .expect("failed to execute tooler");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.contains("minikube"));
}
