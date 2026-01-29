use crate::config::*;
use crate::download::{download_file, extract_archive, find_executable_in_extracted};
use crate::platform::{find_asset_for_platform, get_system_info};
use crate::tool_id::ToolIdentifier;
use crate::types::*;
use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use std::fs;
use std::path::PathBuf;
use walkdir::WalkDir;

pub fn list_installed_tools(config: &ToolerConfig) {
    use console::style;
    println!("--- Installed Tooler Tools ---");
    if config.tools.is_empty() {
        println!("  No tools installed yet.");
        return;
    }

    let mut tools: Vec<_> = config.tools.values().collect();
    tools.sort_by_key(|t| &t.repo);

    let now = Utc::now();

    for info in tools {
        let pin_status = if info.pinned { "📌 " } else { "" };
        let install_type_emoji = match info.install_type.as_str() {
            "archive" => "📦",
            "binary" => "🚀",
            "python-venv" => "🐍",
            _ => "🛠️",
        };

        let arch = if info.executable_path.contains("__") {
            info.executable_path
                .split("__")
                .nth(2)
                .and_then(|s| s.split('/').next())
                .unwrap_or("unknown")
        } else {
            "unknown"
        };

        let (age_str, is_eligible) = match info.installed_at.parse::<DateTime<Utc>>() {
            Ok(installed_at) => {
                let duration = now - installed_at;
                let days = duration.num_days();
                let s = if days > 0 {
                    format!("{}d", days)
                } else if duration.num_hours() > 0 {
                    format!("{}h", duration.num_hours())
                } else {
                    format!("{}m", duration.num_minutes())
                };
                let eligible = days >= config.settings.update_check_days as i64;
                (s, eligible)
            }
            Err(_) => ("unknown".to_string(), false),
        };

        let age_colored = if is_eligible {
            style(&age_str).red().bold()
        } else {
            style(&age_str).green()
        };

        let update_note = if is_eligible && !info.pinned {
            format!(" {}", style("(! stale)").yellow().italic())
        } else {
            "".to_string()
        };

        let forge_emoji = match info.forge {
            Forge::GitHub => "🐙",
            Forge::Url => "🔗",
        };

        println!(
            "  - {} ({}) {}{}{}[{} | {} | {}]{}",
            info.repo,
            info.version,
            forge_emoji,
            pin_status,
            install_type_emoji,
            info.install_type,
            arch,
            age_colored,
            update_note
        );
        println!("    Path:    {}\n", info.executable_path);
    }
    println!("------------------------------");
}

pub async fn check_for_updates(config: &mut ToolerConfig) -> Result<()> {
    if config.settings.update_check_days <= 0 {
        return Ok(());
    }

    tracing::info!(
        "Checking for tools not updated in >{} days...",
        config.settings.update_check_days
    );
    let now = Utc::now();
    let mut updates_found = Vec::new();
    let mut keys_to_update = Vec::new();
    let mut tools_to_auto_update = Vec::new();

    let stale_tools: Vec<(String, String, String, String)> = config
        .tools
        .iter()
        .filter_map(|(key, info)| {
            if info.pinned {
                return None;
            }
            let check_time = info.last_checked.as_deref().unwrap_or(&info.last_accessed);

            if let Ok(last_checked) = check_time.parse::<DateTime<Utc>>() {
                let days_since_check = (now - last_checked).num_days();
                if days_since_check > config.settings.update_check_days as i64 {
                    return Some((
                        key.clone(),
                        info.tool_name.clone(),
                        info.repo.clone(),
                        info.version.clone(),
                    ));
                }
            }
            None
        })
        .collect();

    if stale_tools.is_empty() {
        tracing::info!("No stale unpinned tools found to check for updates.");
        return Ok(());
    }

    for (key, name, repo, version) in stale_tools {
        let tool_info = config.tools.get(&key).unwrap();

        match tool_info.forge {
            Forge::GitHub => {
                tracing::info!(
                    "Checking for GitHub update for {} (current: {})...",
                    repo,
                    version
                );
                if let Ok(release) = get_gh_release_info(&repo, Some("latest")).await {
                    let current_clean = version.trim_start_matches('v');
                    let latest_clean = release.tag_name.trim_start_matches('v');

                    if latest_clean != current_clean {
                        if config.settings.auto_update {
                            tools_to_auto_update.push((name, repo.clone(), release.tag_name));
                        } else {
                            updates_found.push(format!(
                                "Tool {} ({}) has update: {} -> {}",
                                name, repo, version, release.tag_name
                            ));
                        }
                    }
                    keys_to_update.push(key);
                }
            }
            Forge::Url => {
                if let Some(url) = &tool_info.original_url {
                    tracing::info!("Checking for URL update for {} at {}...", name, url);
                    if let Ok(versions) = discover_url_versions(url).await {
                        if let Some(latest) = versions.last() {
                            let current_clean = version.trim_start_matches('v');
                            let latest_clean = latest.trim_start_matches('v');

                            if latest_clean != current_clean {
                                let new_url = url.replace(version.as_str(), latest);
                                if config.settings.auto_update {
                                    tools_to_auto_update.push((name, new_url, latest.clone()));
                                } else {
                                    updates_found.push(format!(
                                        "Tool {} (URL) has update: {} -> {} (URL: {})",
                                        name, version, latest, new_url
                                    ));
                                }
                            }
                        }
                    }
                    keys_to_update.push(key);
                }
            }
        }
    }

    if !tools_to_auto_update.is_empty() {
        eprintln!(
            "Auto-updating {} stale tools...",
            tools_to_auto_update.len()
        );
    }
    for (_name, repo, _version) in tools_to_auto_update {
        match install_or_update_tool(config, &repo, true, None).await {
            Ok(_) => tracing::info!("{} auto-updated successfully", repo),
            Err(e) => tracing::error!("Failed to auto-update {}: {}", repo, e),
        }
    }

    for key in keys_to_update {
        if let Some(tool_info) = config.tools.get_mut(&key) {
            tool_info.last_checked = Some(now.to_rfc3339());
        }
    }

    if !updates_found.is_empty() {
        save_tool_configs(config)?;
        eprintln!("\n--- Tool Updates Available ---");
        for msg in updates_found {
            eprintln!("  {}", msg);
        }
        eprintln!("To update, run `tooler update [repo/tool]` or `tooler update all`.");
        eprintln!("----------------------------\n");
    }

    Ok(())
}

pub async fn install_or_update_tool(
    config: &mut ToolerConfig,
    tool_id: &str,
    is_update: bool,
    asset_name: Option<&str>,
) -> Result<PathBuf> {
    let tool_identifier = ToolIdentifier::parse(tool_id).map_err(|e| anyhow!(e))?;
    let system_info = get_system_info();

    // 1. Get release info
    let release_info = match tool_identifier.forge {
        Forge::GitHub => {
            get_gh_release_info(
                &tool_identifier.full_repo(),
                tool_identifier.version.as_deref(),
            )
            .await?
        }
        Forge::Url => GitHubRelease {
            tag_name: tool_identifier
                .version
                .clone()
                .unwrap_or_else(|| "unknown".to_string()),
            assets: vec![],
        },
    };

    // 2. Determine architecture-specific download URL
    let (download_url, original_url) = match tool_identifier.forge {
        Forge::GitHub => {
            let asset_url = if let Some(name) = asset_name {
                release_info
                    .assets
                    .iter()
                    .find(|a| a.name == name)
                    .map(|a| a.browser_download_url.clone())
                    .ok_or_else(|| anyhow!("Asset {} not found in release", name))?
            } else {
                find_asset_for_platform(
                    &release_info.assets,
                    &tool_identifier.full_repo(),
                    &system_info.os,
                    &system_info.arch,
                )?
                .map(|a| a.download_url)
                .ok_or_else(|| anyhow!("No suitable asset found for platform"))?
            };
            (asset_url, None)
        }
        Forge::Url => (
            tool_identifier
                .url
                .clone()
                .ok_or_else(|| anyhow!("No URL provided"))?,
            tool_identifier.url.clone(),
        ),
    };

    // 3. Setup paths
    let tools_dir = get_tooler_tools_dir()?;
    let forge_name = match tool_identifier.forge {
        Forge::GitHub => "github",
        Forge::Url => "url",
    };

    let tool_dir_name = format!(
        "{}__{}__{}",
        tool_identifier.author,
        tool_identifier.tool_name(),
        system_info.arch
    );
    let tool_install_dir = tools_dir.join(forge_name).join(&tool_dir_name);
    let version_dir = tool_install_dir.join(&release_info.tag_name);

    // 4. Download and extract
    if version_dir.exists() && !is_update {
        tracing::info!(
            "Tool {} v{} already installed",
            tool_id,
            release_info.tag_name
        );
        let archive_name = download_url.split('/').last().unwrap_or("unknown");
        let archive_path = version_dir.join(archive_name);
        if let Some(exec_path) = find_executable_in_extracted(
            &version_dir,
            &tool_identifier.tool_name(),
            &tool_identifier.full_repo(),
            &system_info.os,
            &archive_path,
        ) {
            return Ok(exec_path);
        }
    }

    fs::create_dir_all(&version_dir)?;
    let archive_name = download_url.split('/').last().unwrap_or("unknown");
    let archive_path = version_dir.join(archive_name);

    download_file(&download_url, &archive_path).await?;

    let executable_path = if archive_name.ends_with(".zip")
        || archive_name.ends_with(".tar.gz")
        || archive_name.ends_with(".tgz")
        || archive_name.ends_with(".tar.xz")
    {
        extract_archive(
            &archive_path,
            &version_dir,
            &tool_identifier.tool_name(),
            &tool_identifier.full_repo(),
        )?
    } else {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&archive_path)?.permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&archive_path, perms)?;
        }
        archive_path.clone()
    };

    // 5. Update config
    let tool_info = ToolInfo {
        tool_name: tool_identifier.tool_name().to_lowercase(),
        repo: tool_identifier.full_repo(),
        version: release_info.tag_name,
        executable_path: executable_path.to_string_lossy().to_string(),
        install_type: if archive_name.contains('.') {
            archive_name
                .split('.')
                .last()
                .unwrap_or("binary")
                .to_string()
        } else {
            "binary".to_string()
        },
        pinned: tool_identifier.is_pinned(),
        installed_at: Utc::now().to_rfc3339(),
        last_accessed: Utc::now().to_rfc3339(),
        last_checked: Some(Utc::now().to_rfc3339()),
        forge: tool_identifier.forge.clone(),
        original_url,
    };

    let key = tool_identifier.config_key();
    config.tools.insert(key, tool_info);
    save_tool_configs(config)?;

    Ok(executable_path)
}

pub async fn get_gh_release_info(repo: &str, version: Option<&str>) -> Result<GitHubRelease> {
    let url = if let Some(v) = version {
        if v == "latest" {
            format!("https://api.github.com/repos/{}/releases/latest", repo)
        } else {
            format!("https://api.github.com/repos/{}/releases/tags/{}", repo, v)
        }
    } else {
        format!("https://api.github.com/repos/{}/releases/latest", repo)
    };

    let client = reqwest::Client::new();
    let response = client
        .get(&url)
        .header("User-Agent", "tooler")
        .send()
        .await?;

    if !response.status().is_success() {
        return Err(anyhow!(
            "Failed to get release info for {}: {}",
            repo,
            response.status()
        ));
    }

    let release: GitHubRelease = response.json().await?;
    Ok(release)
}

pub async fn discover_url_versions(_url: &str) -> Result<Vec<String>> {
    Ok(vec![])
}

pub fn pin_tool(config: &mut ToolerConfig, tool_id: &str) -> Result<()> {
    let tool_identifier = ToolIdentifier::parse(tool_id).map_err(|e| anyhow!(e))?;
    if tool_identifier.version.is_none() {
        return Err(anyhow!(
            "Version must be specified for pinning: tool@version"
        ));
    }

    let key = tool_identifier.config_key();
    if let Some(mut tool_info) = config.tools.remove(&key) {
        tool_info.pinned = true;
        let version = tool_info.version.clone();
        config.tools.insert(key, tool_info.clone());

        // Also update @latest entry to point to this pinned version
        let latest_key = tool_identifier.default_config_key();
        if let Some(mut latest_tool) = config.tools.remove(&latest_key) {
            latest_tool.pinned = true;
            latest_tool.version = tool_info.version.clone();
            latest_tool.executable_path = tool_info.executable_path.clone();
            config.tools.insert(latest_key, latest_tool);
        }

        save_tool_configs(config)?;
        tracing::info!("Tool {} pinned to version {}", tool_id, version);
        Ok(())
    } else {
        Err(anyhow!("Tool {} not found", tool_id))
    }
}

pub fn remove_tool(config: &mut ToolerConfig, key: &str) -> Result<()> {
    if config.tools.remove(key).is_some() {
        save_tool_configs(config)?;
        tracing::info!("Tool {} removed", key);
        Ok(())
    } else {
        Err(anyhow!("Tool {} not found", key))
    }
}

#[allow(dead_code)]
pub(crate) fn find_highest_version<'a>(tools: Vec<&'a ToolInfo>) -> Option<&'a ToolInfo> {
    tools.into_iter().max_by(|a, b| {
        let v_a = semver::Version::parse(&a.version.trim_start_matches('v'))
            .unwrap_or_else(|_| semver::Version::new(0, 0, 0));
        let v_b = semver::Version::parse(&b.version.trim_start_matches('v'))
            .unwrap_or_else(|_| semver::Version::new(0, 0, 0));
        v_a.cmp(&v_b)
    })
}

pub fn find_tool_entry<'a>(
    config: &'a ToolerConfig,
    tool_query: &str,
) -> Option<(&'a String, &'a ToolInfo)> {
    let tool_identifier = ToolIdentifier::parse(tool_query).ok()?;
    let tool_key = tool_identifier.config_key();

    // 1. Try exact match (including version if specified)
    if tool_identifier.is_pinned() {
        let requested_version = tool_identifier.version.as_ref().unwrap();

        if config.tools.get(&tool_key).is_some() {
            return config.tools.get_key_value(&tool_key);
        }

        // Semver match for partial versions
        let matching_tools: Vec<(&String, &ToolInfo)> = config
            .tools
            .iter()
            .filter(|(_, info)| {
                // Tightened matching: must match repo name or full repo path exactly
                let name_matches = info.tool_name.to_lowercase()
                    == tool_identifier.tool_name().to_lowercase()
                    || info.repo.to_lowercase() == tool_identifier.full_repo().to_lowercase();

                if !name_matches {
                    return false;
                }
                version_matches(requested_version, &info.version)
            })
            .collect();

        if !matching_tools.is_empty() {
            return matching_tools.into_iter().max_by(|a, b| {
                let v_a = semver::Version::parse(&a.1.version.trim_start_matches('v'))
                    .unwrap_or_else(|_| semver::Version::new(0, 0, 0));
                let v_b = semver::Version::parse(&b.1.version.trim_start_matches('v'))
                    .unwrap_or_else(|_| semver::Version::new(0, 0, 0));
                v_a.cmp(&v_b)
            });
        }

        config.tools.get_key_value(&tool_key)
    } else {
        // Unqualified name or unpinned: find most recently accessed matching tool
        config
            .tools
            .iter()
            .filter(|(_, info)| {
                // Tightened matching
                info.tool_name.to_lowercase() == tool_identifier.tool_name().to_lowercase()
                    || info.repo.to_lowercase() == tool_identifier.full_repo().to_lowercase()
            })
            .max_by_key(|(_, info)| &info.last_accessed)
    }
}

pub fn find_tool_executable<'a>(
    config: &'a ToolerConfig,
    tool_query: &str,
) -> Option<&'a ToolInfo> {
    find_tool_entry(config, tool_query).map(|(_, info)| info)
}

pub fn try_recover_tool(tool_query: &str) -> Result<Option<ToolInfo>> {
    let tool_id = ToolIdentifier::parse(tool_query).map_err(|e| anyhow!(e))?;
    let tools_dir = get_tooler_tools_dir()?;
    let system_info = get_system_info();
    let tool_name_lower = tool_id.tool_name().to_lowercase();

    let mut scan_dirs = vec![tools_dir.clone()];
    for forge in &["github", "url"] {
        scan_dirs.push(tools_dir.join(forge));
    }

    for forge_dir in scan_dirs {
        if !forge_dir.exists() {
            continue;
        }

        let Ok(entries) = fs::read_dir(&forge_dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }

            let dir_name = path.file_name().unwrap_or_default().to_string_lossy();
            let parts: Vec<&str> = dir_name.split("__").collect();

            let (author, repo, arch) = match parts.len() {
                3 => (parts[0], parts[1], Some(parts[2])),
                2 => (parts[0], parts[1], None),
                1 => ("unknown", parts[0], None),
                _ => continue,
            };

            let repo_matches = repo.to_lowercase() == tool_name_lower
                || tool_id.full_repo().replace("/", "__") == dir_name.as_ref()
                || tool_id.full_repo() == format!("{}/{}", author, repo);

            if repo_matches {
                if let Some(a) = arch {
                    if a != system_info.arch {
                        continue;
                    }
                }

                // Recursively find directories that look like versions
                let mut version_candidates = Vec::new();
                let re_version = regex::Regex::new(r"v?\d+\.\d+").unwrap();

                for entry in WalkDir::new(&path).into_iter().filter_map(|e| e.ok()) {
                    if entry.path().is_dir() {
                        let name = entry.file_name().to_string_lossy();
                        if re_version.is_match(&name) {
                            version_candidates.push((name.to_string(), entry.path().to_path_buf()));
                        }
                    }
                }

                // If no version-like dir found, try top-level subdirs
                if version_candidates.is_empty() {
                    if let Ok(subs) = fs::read_dir(&path) {
                        for sub in subs.flatten() {
                            if sub.path().is_dir() {
                                version_candidates.push((
                                    sub.file_name().to_string_lossy().to_string(),
                                    sub.path(),
                                ));
                            }
                        }
                    }
                }

                // Sort and pick latest version
                version_candidates.sort_by(|a, b| {
                    let v_a = semver::Version::parse(&a.0.trim_start_matches('v'))
                        .unwrap_or_else(|_| semver::Version::new(0, 0, 0));
                    let v_b = semver::Version::parse(&b.0.trim_start_matches('v'))
                        .unwrap_or_else(|_| semver::Version::new(0, 0, 0));
                    v_a.cmp(&v_b)
                });

                if let Some((version, ver_path)) = version_candidates.last() {
                    if let Some(exec_path) = find_executable_in_extracted(
                        ver_path,
                        repo,
                        &format!("{}/{}", author, repo),
                        &system_info.os,
                        &PathBuf::new(),
                    ) {
                        let clean_version = version.trim_start_matches('v').to_string();
                        let forge_val = if forge_dir.ends_with("url") {
                            Forge::Url
                        } else {
                            Forge::GitHub
                        };

                        // Deduce install type
                        let install_type = if ver_path.join(".venv").exists() {
                            "python-venv".to_string()
                        } else {
                            // Standalone binary or archive?
                            let file_count = fs::read_dir(ver_path).map(|r| r.count()).unwrap_or(0);
                            if file_count <= 3 {
                                // Usually binary, license, readme
                                "binary".to_string()
                            } else {
                                "archive".to_string()
                            }
                        };

                        return Ok(Some(ToolInfo {
                            tool_name: repo.to_lowercase(),
                            repo: if author == "direct" || author == "unknown" {
                                repo.to_string()
                            } else {
                                format!("{}/{}", author, repo)
                            },
                            version: clean_version,
                            executable_path: exec_path.to_string_lossy().to_string(),
                            install_type,
                            pinned: false,
                            installed_at: Utc::now().to_rfc3339(),
                            last_accessed: Utc::now().to_rfc3339(),
                            last_checked: Some(Utc::now().to_rfc3339()),
                            forge: forge_val,
                            original_url: None,
                        }));
                    }
                }
            }
        }
    }

    Ok(None)
}

pub(crate) fn version_matches(requested: &str, existing: &str) -> bool {
    let requested_clean = requested.trim_start_matches('v');
    let existing_clean = existing.trim_start_matches('v');

    if requested_clean == existing_clean {
        return true;
    }

    let req_parse = semver::Version::parse(requested_clean);
    let exist_parse = semver::Version::parse(existing_clean);

    if let (Ok(req_semver), Ok(exist_semver)) = (req_parse, exist_parse) {
        let req_parts = requested_clean.split('.').count();
        if req_parts <= 2 {
            req_semver.major == exist_semver.major && req_semver.minor == exist_semver.minor
        } else {
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
