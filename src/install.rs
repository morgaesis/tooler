use crate::config::*;
use crate::download::{download_file, extract_archive};
use crate::platform::{find_asset_for_platform, get_system_info};
use crate::tool_id::ToolIdentifier;
use crate::types::*;
use anyhow::{anyhow, Result};
use chrono::Utc;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

pub async fn get_gh_release_info(
    repo_full_name: &str,
    version: Option<&str>,
) -> Result<GitHubRelease> {
    let version = version.unwrap_or("latest");
    let url = if version == "latest" {
        format!(
            "https://api.github.com/repos/{}/releases/latest",
            repo_full_name
        )
    } else {
        // Smart version handling: don't add 'v' prefix for non-numeric versions like "tip", "master"
        // but preserve existing 'v' prefixes and add 'v' for numeric versions
        let version = if version.starts_with('v')
            || version.chars().next().is_some_and(|c| c.is_ascii_digit())
        {
            version
        } else {
            &format!("v{}", version)
        };
        format!(
            "https://api.github.com/repos/{}/releases/tags/{}",
            repo_full_name, version
        )
    };

    tracing::debug!("Fetching GitHub release info from: {}", url);

    let client = reqwest::Client::new();
    let mut request = client
        .get(&url)
        .header("Accept", "application/vnd.github.v3+json")
        .header("User-Agent", "tooler/0.1.0");

    if let Ok(token) = std::env::var("GITHUB_TOKEN") {
        request = request.header("Authorization", format!("token {}", token));
        tracing::debug!("Using GITHUB_TOKEN");
    }

    let response = request.send().await?;
    if !response.status().is_success() {
        let status = response.status();
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Unable to read error response".to_string());
        return Err(anyhow!(
            "GitHub API request failed: {} - {}",
            status,
            error_text
        ));
    }

    let release: GitHubRelease = response.json().await?;
    Ok(release)
}

pub async fn install_or_update_tool(
    config: &mut ToolerConfig,
    tool_name: &str,
    repo_full_name: &str,
    version: Option<&str>,
    force_update: bool,
    asset_override: Option<&str>,
) -> Result<PathBuf> {
    // Prevent installing a tool that would conflict with tooler-shim
    if tool_name.to_lowercase() == "tooler-shim" {
        return Err(anyhow!(
            "Cannot install tool named 'tooler-shim' as it conflicts with the tooler shim system"
        ));
    }

    let system_info = get_system_info();
    let requested_version = version.unwrap_or("latest");

    let release_info = get_gh_release_info(repo_full_name, Some(requested_version)).await?;
    let actual_version = &release_info.tag_name;

    let tool_identifier = ToolIdentifier::parse(&format!("{}@{}", repo_full_name, actual_version))
        .map_err(|e| anyhow!("Failed to parse tool identifier: {}", e))?;
    let tool_key = tool_identifier.config_key();

    let tool_install_base_dir = get_tooler_tools_dir()?.join(format!(
        "{}__{}",
        repo_full_name.replace('/', "__"),
        system_info.arch
    ));
    let tool_version_dir = tool_install_base_dir.join(actual_version);

    tracing::debug!(
        "Tool installation base directory: {}",
        tool_install_base_dir.display()
    );
    tracing::debug!("Tool version directory: {}", tool_version_dir.display());
    tracing::debug!("Looking for tool with key: {}", tool_key);

    // Check if already installed
    if !force_update {
        if let Some(current_info) = config.tools.get(&tool_key) {
            tracing::debug!("Found tool info: {:?}", current_info);
            tracing::debug!(
                "Checking if executable exists at: {}",
                current_info.executable_path
            );

            // If asset_override is provided, check if the specific asset exists
            if let Some(asset_name) = asset_override {
                let expected_asset_path = tool_version_dir.join(asset_name);
                if expected_asset_path.exists() {
                    tracing::info!(
                        "Tool {} {} is already installed with asset '{}'.",
                        tool_name,
                        actual_version,
                        asset_name
                    );
                    return Ok(PathBuf::from(&current_info.executable_path));
                } else {
                    tracing::info!(
                        "Asset '{}' for {} {} not found. Re-downloading...",
                        asset_name,
                        tool_name,
                        actual_version
                    );
                }
            } else if Path::new(&current_info.executable_path).exists() {
                tracing::info!(
                    "Tool {} {} is already installed.",
                    tool_name,
                    actual_version
                );
                return Ok(PathBuf::from(&current_info.executable_path));
            } else {
                tracing::warn!(
                    "Installation for {} {} is corrupted. Re-installing.",
                    tool_name,
                    actual_version
                );
            }
        } else {
            tracing::debug!("Tool not found in config with key: {}", tool_key);
        }
    }

    tracing::info!("Installing/Updating {} {}...", tool_name, actual_version);

    // Find suitable asset
    let asset_info = if let Some(asset_name) = asset_override {
        let asset = release_info
            .assets
            .iter()
            .find(|a| a.name == asset_name)
            .ok_or_else(|| {
                anyhow!(
                    "Specified asset '{}' not found in release assets",
                    asset_name
                )
            })?;
        Some(AssetInfo {
            name: asset.name.clone(),
            download_url: asset.browser_download_url.clone(),
        })
    } else {
        find_asset_for_platform(
            &release_info.assets,
            repo_full_name,
            &system_info.os,
            &system_info.arch,
        )?
    };

    let asset_info = asset_info.ok_or_else(|| {
        anyhow!(
            "No suitable asset found for {} {} for your platform",
            repo_full_name,
            actual_version
        )
    })?;

    // Clean up existing installation
    if tool_version_dir.exists() {
        fs::remove_dir_all(&tool_version_dir)?;
    }
    fs::create_dir_all(&tool_version_dir)?;

    let executable_path = if asset_info.name.to_lowercase().ends_with(".whl") {
        install_python_tool(&tool_version_dir, &asset_info.name, tool_name).await?
    } else if asset_info.name.to_lowercase().ends_with(".tar.gz")
        || asset_info.name.to_lowercase().ends_with(".zip")
        || asset_info.name.to_lowercase().ends_with(".tar.xz")
        || asset_info.name.to_lowercase().ends_with(".tgz")
    {
        let temp_dir = TempDir::new()?;
        let temp_download_path = temp_dir.path().join(&asset_info.name);

        download_file(&asset_info.download_url, &temp_download_path).await?;

        // Cache downloaded file
        let cached_asset_path = tool_version_dir.join(&asset_info.name);
        fs::copy(&temp_download_path, &cached_asset_path)?;

        extract_archive(&temp_download_path, &tool_version_dir, tool_name)?
    } else {
        // Direct executable
        let temp_dir = TempDir::new()?;
        let temp_download_path = temp_dir.path().join(&asset_info.name);

        download_file(&asset_info.download_url, &temp_download_path).await?;

        let final_binary_name = if system_info.os == "windows" {
            format!("{}.exe", tool_name)
        } else {
            tool_name.to_string()
        };

        let move_target_path = tool_version_dir.join(final_binary_name);
        fs::rename(&temp_download_path, &move_target_path)?;

        // Make executable on Unix-like systems
        if system_info.os != "windows" {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mut perms = fs::metadata(&move_target_path)?.permissions();
                perms.set_mode(0o755);
                fs::set_permissions(&move_target_path, perms)?;
            }
        }

        tracing::info!(
            "Installed direct executable to: {}",
            move_target_path.display()
        );
        move_target_path
    };

    // Update config
    let install_type = if asset_info.name.to_lowercase().ends_with(".whl") {
        "python-venv".to_string()
    } else if asset_info.name.to_lowercase().ends_with(".tar.gz")
        || asset_info.name.to_lowercase().ends_with(".zip")
        || asset_info.name.to_lowercase().ends_with(".tar.xz")
        || asset_info.name.to_lowercase().ends_with(".tgz")
    {
        "archive".to_string()
    } else {
        "binary".to_string()
    };

    let tool_info = ToolInfo {
        tool_name: tool_name.to_lowercase(),
        repo: repo_full_name.to_string(),
        version: actual_version.trim_start_matches('v').to_string(),
        executable_path: executable_path.to_string_lossy().to_string(),
        install_type,
        pinned: version.is_some()
            && requested_version != "latest"
            && requested_version != "default",
        installed_at: Utc::now().to_rfc3339(),
        last_accessed: Utc::now().to_rfc3339(),
    };

    config.tools.insert(tool_key, tool_info);
    save_tool_configs(config)?;

    tracing::info!(
        "Successfully installed {} {} to {}",
        tool_name,
        actual_version,
        executable_path.display()
    );

    // Add pinning suggestion if asset was explicitly selected
    if let Some(asset_name) = asset_override {
        tracing::info!(
            "Successfully installed {}@{} using asset '{}'.",
            repo_full_name,
            actual_version,
            asset_name
        );
        tracing::info!(
            "To use this asset by default in the future, run:\n  tooler pin {}@{}",
            repo_full_name,
            asset_name
        );
    }

    Ok(executable_path)
}

async fn install_python_tool(
    tool_dir: &Path,
    wheel_path: &str,
    tool_name: &str,
) -> Result<PathBuf> {
    tracing::info!("Setting up Python environment for {}...", tool_name);

    let venv_path = tool_dir.join(".venv");

    // Create virtual environment
    let output = Command::new("python3")
        .args(["-m", "venv", &venv_path.to_string_lossy()])
        .output()?;

    if !output.status.success() {
        return Err(anyhow!(
            "Failed to create virtual environment: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let pip_exec = if cfg!(windows) {
        venv_path.join("Scripts").join("pip.exe")
    } else {
        venv_path.join("bin").join("pip")
    };

    // Upgrade pip
    let output = Command::new(&pip_exec)
        .args(["install", "--upgrade", "pip"])
        .output()?;

    if !output.status.success() {
        return Err(anyhow!(
            "Failed to upgrade pip: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    // Install wheel
    tracing::info!("Installing local wheel {}...", wheel_path);
    let output = Command::new(&pip_exec)
        .args(["install", wheel_path])
        .output()?;

    if !output.status.success() {
        return Err(anyhow!(
            "Failed to install wheel: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    // Create shim script
    let shim_path = tool_dir.join(tool_name);
    let shim_content = if cfg!(windows) {
        format!(
            "@echo off\r\n\"%~dp0\\.venv\\Scripts\\{}.exe\" %*\r\n",
            tool_name
        )
    } else {
        format!(
            "#!/bin/sh\nexec \"$(dirname \"$0\")/.venv/bin/{}\" \"$@\"\n",
            tool_name
        )
    };

    fs::write(&shim_path, shim_content)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&shim_path)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&shim_path, perms)?;
    }

    tracing::info!("Created shim script at: {}", shim_path.display());
    Ok(shim_path)
}

pub fn find_tool_executable<'a>(
    config: &'a ToolerConfig,
    tool_query: &str,
) -> Option<&'a ToolInfo> {
    tracing::debug!("Finding tool executable for query: {}", tool_query);

    let tool_identifier = ToolIdentifier::parse(tool_query).ok()?;
    let tool_key = tool_identifier.config_key();

    tracing::debug!("Parsed tool identifier: {:?}", tool_identifier);
    tracing::debug!("Looking for tool with key: {}", tool_key);

    if tool_identifier.is_pinned() {
        // Check if it's an exact version match first
        if let Some(exact_match) = config.tools.get(&tool_key) {
            return Some(exact_match);
        }

        // If exact match not found, try matching by repo name with any version
        // This handles cases like @latest when you have a specific version installed
        let matching_tool = config
            .tools
            .values()
            .find(|info| info.repo.to_lowercase() == tool_identifier.full_repo().to_lowercase());

        if let Some(exact_match) = matching_tool {
            tracing::debug!("Found tool by repo match: {}", exact_match.repo);
            return Some(exact_match);
        }

        // Also try matching by tool name only (last part of repo)
        let matching_by_name = config.tools.values().find(|info| {
            info.repo
                .to_lowercase()
                .ends_with(&format!("/{}", tool_identifier.tool_name().to_lowercase()))
                || info.repo.to_lowercase() == tool_identifier.tool_name().to_lowercase()
        });

        if let Some(exact_match) = matching_by_name {
            tracing::debug!("Found tool by name match: {}", exact_match.repo);
            return Some(exact_match);
        }

        // For backwards compatibility, also check the old : format
        let old_key = format!(
            "{}:{}",
            tool_identifier.full_repo(),
            tool_identifier.api_version()
        );
        if let Some(exact_match) = config.tools.get(&old_key) {
            return Some(exact_match);
        }

        // If not found, try semver matching for partial versions
        if let Some(requested_version) = &tool_identifier.version {
            let matching_tools: Vec<&'a ToolInfo> = config.tools
                .values()
                .filter(|info| {
                    // Match by tool name/repo first
                    let name_matches = info.tool_name.to_lowercase() == tool_identifier.tool_name().to_lowercase() ||
                        (tool_identifier.author != "unknown" &&
                         info.repo.to_lowercase() == tool_identifier.full_repo().to_lowercase()) ||
                        info.repo.to_lowercase().ends_with(&format!("/{}", tool_identifier.tool_name().to_lowercase())) ||
                        info.repo.to_lowercase() == tool_identifier.tool_name().to_lowercase() ||
                        // Also match if requested tool name is part of repo
                        info.repo.to_lowercase().contains(&format!("/{}", tool_identifier.tool_name().to_lowercase()));

                    tracing::trace!("Name match check for {}: {} (repo: {})",
                        tool_identifier.tool_name(), name_matches, info.repo);

                    if !name_matches {
                        return false;
                    }

                    // Use version field from ToolInfo
                    version_matches(requested_version, &info.version)
                })
                .collect();

            tracing::debug!(
                "Found {} matching tools for version {}",
                matching_tools.len(),
                requested_version
            );

            // Return highest version that matches
            if !matching_tools.is_empty() {
                return find_highest_version(matching_tools);
            }
        }

        // If no semver match found, try exact match again (for non-semver versions like "master")
        config.tools.get(&tool_key)
    } else {
        // Find matching tools for unpinned queries
        let matching_tools: Vec<&'a ToolInfo> = config.tools
            .values()
            .filter(|info| {
                // Match by tool name (e.g., "k9s" matches "derailed/k9s")
                info.tool_name.to_lowercase() == tool_identifier.tool_name().to_lowercase() ||
                // Match by full repo if specified (e.g., "derailed/k9s" matches "derailed/k9s")
                (tool_identifier.author != "unknown" &&
                 info.repo.to_lowercase() == tool_identifier.full_repo().to_lowercase()) ||
                // Match by repo name alone (e.g., "k9s" matches repo "k9s")
                info.repo.to_lowercase().ends_with(&format!("/{}", tool_identifier.tool_name().to_lowercase())) ||
                info.repo.to_lowercase() == tool_identifier.tool_name().to_lowercase() ||
                // Also match if requested tool name is part of repo
                info.repo.to_lowercase().contains(&format!("/{}", tool_identifier.tool_name().to_lowercase()))
            })
            .collect();

        tracing::debug!("Found {} matching tools", matching_tools.len());

        // Return the most recently accessed tool
        matching_tools
            .into_iter()
            .max_by_key(|info| &info.last_accessed)
    }
}

/// Check if a requested version matches an existing version
/// Supports semver ranges (e.g., "1.5" matches "1.5.2", "1.5.0") and exact string matches
fn version_matches(requested: &str, existing: &str) -> bool {
    // Clean versions (remove 'v' prefix if present)
    let requested_clean = requested.trim_start_matches('v');
    let existing_clean = existing.trim_start_matches('v');

    // If they're exactly the same, it's a match
    if requested_clean == existing_clean {
        return true;
    }

    // Try to parse as semver
    let req_parse = semver::Version::parse(requested_clean);
    let exist_parse = semver::Version::parse(existing_clean);

    if let (Ok(req_semver), Ok(exist_semver)) = (req_parse, exist_parse) {
        let req_parts = requested_clean.split('.').count();

        // For partial versions like "1.5", match any 1.5.x
        if req_parts <= 2 {
            req_semver.major == exist_semver.major && req_semver.minor == exist_semver.minor
        } else {
            // For full versions, exact match
            req_semver == exist_semver
        }
    } else {
        // Try using version requirements for partial matching
        if requested_clean.split('.').count() <= 2 {
            if let Ok(req_req) = semver::VersionReq::parse(requested_clean) {
                if let Ok(exist_semver) = semver::Version::parse(existing_clean) {
                    return req_req.matches(&exist_semver);
                }
            }
        }

        // Non-semver versions (like "master", "tip", etc.) - exact match only
        requested_clean == existing_clean
    }
}

/// Find highest version among matching tools
fn find_highest_version(tools: Vec<&ToolInfo>) -> Option<&ToolInfo> {
    tools.into_iter().max_by(|a, b| {
        let a_version = &a.version;
        let b_version = &b.version;

        // Clean versions
        let a_clean = a_version.trim_start_matches('v');
        let b_clean = b_version.trim_start_matches('v');

        // Try to compare as semver
        match (
            semver::Version::parse(a_clean),
            semver::Version::parse(b_clean),
        ) {
            (Ok(a_semver), Ok(b_semver)) => a_semver.cmp(&b_semver),
            _ => {
                // Fall back to string comparison for non-semver versions
                a_clean.cmp(b_clean)
            }
        }
    })
}

pub fn pin_tool(config: &mut ToolerConfig, tool_query: &str) -> Result<()> {
    let tool_identifier =
        ToolIdentifier::parse(tool_query).map_err(|e| anyhow!("Invalid tool identifier: {}", e))?;

    // Find the tool in config using the exact version key
    let tool_key = tool_identifier.config_key();

    if let Some(mut tool_info) = config.tools.remove(&tool_key) {
        // Mark the tool as pinned
        tool_info.pinned = true;
        config.tools.insert(tool_key, tool_info.clone());

        // Also update @latest entry to point to this pinned version
        let latest_key = tool_identifier.default_config_key();
        if let Some(mut latest_tool) = config.tools.remove(&latest_key) {
            latest_tool.pinned = true;
            latest_tool.version = tool_info.version.clone();
            latest_tool.executable_path = tool_info.executable_path.clone();
            config.tools.insert(latest_key, latest_tool);
        }

        save_tool_configs(config)?;
        tracing::info!(
            "Successfully pinned {} to version {}",
            tool_identifier.full_repo(),
            tool_info.version
        );
        Ok(())
    } else {
        Err(anyhow!(
            "Tool '{}' not found. Install it first with 'tooler install {}'",
            tool_query,
            tool_query
        ))
    }
}

pub fn remove_tool(config: &mut ToolerConfig, tool_query: &str) -> Result<()> {
    // Prevent removing tooler-shim
    if tool_query.to_lowercase() == "tooler-shim" {
        return Err(anyhow!(
            "Cannot remove 'tooler-shim' as it is part of the tooler system"
        ));
    }

    let tool_identifier =
        ToolIdentifier::parse(tool_query).map_err(|e| anyhow!("Invalid tool identifier: {}", e))?;
    let keys_to_remove: Vec<String> = config
        .tools
        .keys()
        .filter(|k| {
            k.as_str() == tool_identifier.config_key()
                || (!tool_query.contains('@') && !tool_query.contains(':') && {
                    let info = &config.tools[k.as_str()];
                    info.repo.to_lowercase() == tool_query.to_lowercase()
                })
        })
        .cloned()
        .collect();

    if keys_to_remove.is_empty() {
        return Err(anyhow!("Tool '{}' not found", tool_query));
    }

    for key in keys_to_remove {
        if let Some(info) = config.tools.remove(&key) {
            // Remove all architecture directories for this tool
            let tool_base_dir = get_tooler_tools_dir()?.join(info.repo.replace('/', "__"));

            // Try to remove the specific version directory first
            if tool_base_dir.join(&info.version).exists() {
                tracing::info!(
                    "Removing directory: {}",
                    tool_base_dir.join(&info.version).display()
                );
                fs::remove_dir_all(tool_base_dir.join(&info.version))?;
            }

            // Also check for architecture-specific directories
            if let Ok(entries) = fs::read_dir(tool_base_dir.parent().unwrap_or(&tool_base_dir)) {
                for entry in entries.flatten() {
                    let entry_path = entry.path();
                    if entry_path.is_dir() {
                        let dir_name = entry_path.file_name().unwrap_or_default().to_string_lossy();
                        if dir_name.starts_with(&format!("{}__", info.repo.replace('/', "__"))) {
                            let version_dir = entry_path.join(&info.version);
                            if version_dir.exists() {
                                tracing::info!(
                                    "Removing architecture-specific directory: {}",
                                    version_dir.display()
                                );
                                fs::remove_dir_all(&version_dir)?;
                            }
                        }
                    }
                }
            }
        }
    }

    save_tool_configs(config)?;
    tracing::info!("Tool(s) for '{}' removed successfully", tool_query);
    Ok(())
}
