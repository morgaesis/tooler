use crate::types::*;
use anyhow::Result;
use std::collections::HashMap;

pub fn get_system_info() -> PlatformInfo {
    let os = std::env::consts::OS.to_string();
    let arch = std::env::consts::ARCH.to_string();

    let normalized_arch = match arch.as_str() {
        "x86_64" => "amd64".to_string(),
        "aarch64" => "arm64".to_string(),
        "arm" => "arm".to_string(),
        _ => arch,
    };

    PlatformInfo {
        os,
        arch: normalized_arch,
    }
}

pub fn find_asset_for_platform(
    assets: &[GitHubAsset],
    _repo_full_name: &str,
    system_os: &str,
    system_arch: &str,
) -> Result<Option<AssetInfo>> {
    tracing::trace!(
        "Looking for assets matching OS: '{}', ARCH: '{}'",
        system_os,
        system_arch
    );

    let _default_os = vec![system_os.to_string()];
    let _default_arch = vec![system_arch.to_string()];

    let os_aliases = [
        ("linux", vec!["linux", "unknown-linux", "pc-linux"]),
        ("darwin", vec!["darwin", "macos", "osx"]),
        ("windows", vec!["windows", "win", "cygwin"]),
    ];

    let arch_aliases = [
        ("x86_64", vec!["amd64", "x64", "x86_64"]),
        ("aarch64", vec!["arm64", "aarch64"]),
        ("arm64", vec!["arm64", "aarch64"]), // Add arm64 as key since get_system_info normalizes aarch64 to arm64
        ("arm", vec!["arm", "armv7"]),
    ];

    tracing::trace!("Available arch aliases: {:?}", arch_aliases);

    let archive_exts = vec![".tar.gz", ".zip", ".tar.xz", ".tgz"];
    let package_exts = vec![".apk", ".deb", ".rpm"];
    let invalid_exts = vec![
        ".sha256", ".asc", ".sig", ".pem", ".pub", ".md", ".txt", ".pom", ".xml", ".json", ".whl",
    ];

    let mut candidates = categorize_assets(
        assets,
        &os_aliases,
        &arch_aliases,
        &archive_exts,
        &package_exts,
        &invalid_exts,
    );

    // Debug: print all candidates
    for (category, asset_list) in &candidates {
        if !asset_list.is_empty() {
            tracing::trace!("Category '{}': {} assets", category, asset_list.len());
            for asset in asset_list {
                tracing::trace!("  - {}", asset.name);
            }
        }
    }

    let priority_order = vec![
        "os_arch_archive",
        "os_arch_binary",
        "os_arch_package",
        "os_only_archive",
        "os_only_binary",
        "os_only_package",
        "arch_only_archive",
        "arch_only_binary",
        "arch_only_package",
    ];

    for category in priority_order {
        if let Some(asset_list) = candidates.remove(category) {
            // Filter assets by exact OS and architecture match
            let matching_assets: Vec<&GitHubAsset> = asset_list
                .iter()
                .filter(|asset| {
                    let name_lower = asset.name.to_lowercase();
                    let os_match = os_aliases.iter().any(|(os, aliases)| {
                        os == &system_os && aliases.iter().any(|alias| name_lower.contains(alias))
                    });
                    let arch_match = arch_aliases.iter().any(|(arch, aliases)| {
                        arch == &system_arch
                            && aliases.iter().any(|alias| name_lower.contains(alias))
                    });

                    tracing::trace!(
                        "Asset '{}': os_match={}, arch_match={}",
                        asset.name,
                        os_match,
                        arch_match
                    );

                    // For os_arch_* categories, both must match
                    if category.starts_with("os_arch_") {
                        os_match && arch_match
                    } else if category.starts_with("os_only_") {
                        os_match
                    } else if category.starts_with("arch_only_") {
                        arch_match
                    } else {
                        false
                    }
                })
                .collect();

            tracing::trace!(
                "Category '{}': {} matching assets out of {}",
                category,
                matching_assets.len(),
                asset_list.len()
            );

            if let Some(asset) = matching_assets.first() {
                tracing::info!("Found best match: '{}'", asset.name);
                return Ok(Some(AssetInfo {
                    name: asset.name.clone(),
                    download_url: asset.browser_download_url.clone(),
                }));
            }

            // If no exact match, fall back to first asset in category
            if let Some(asset) = asset_list.first() {
                tracing::info!("Found best match (fallback): '{}'", asset.name);
                return Ok(Some(AssetInfo {
                    name: asset.name.clone(),
                    download_url: asset.browser_download_url.clone(),
                }));
            }
        }
    }

    // Fallback to .whl files
    for asset in assets {
        if asset.name.to_lowercase().ends_with(".whl") {
            tracing::warn!("Falling back to Python wheel");
            return Ok(Some(AssetInfo {
                name: asset.name.clone(),
                download_url: asset.browser_download_url.clone(),
            }));
        }
    }

    tracing::error!("No suitable asset found after all checks");
    Ok(None)
}

fn categorize_assets(
    assets: &[GitHubAsset],
    os_aliases: &[(&str, Vec<&str>)],
    arch_aliases: &[(&str, Vec<&str>)],
    archive_exts: &[&str],
    package_exts: &[&str],
    invalid_exts: &[&str],
) -> HashMap<String, Vec<GitHubAsset>> {
    let mut candidates = HashMap::new();

    for category in &[
        "os_arch_archive",
        "os_arch_binary",
        "os_arch_package",
        "os_only_archive",
        "os_only_binary",
        "os_only_package",
        "arch_only_archive",
        "arch_only_binary",
        "arch_only_package",
    ] {
        candidates.insert(category.to_string(), Vec::new());
    }

    for asset in assets {
        let name_lower = asset.name.to_lowercase();

        if invalid_exts.iter().any(|ext| name_lower.ends_with(ext)) {
            continue;
        }

        let has_os = os_aliases
            .iter()
            .any(|(_, aliases)| aliases.iter().any(|alias| name_lower.contains(alias)));
        let has_arch = arch_aliases
            .iter()
            .any(|(_, aliases)| aliases.iter().any(|alias| name_lower.contains(alias)));

        let is_archive = archive_exts.iter().any(|ext| name_lower.ends_with(ext));
        let is_package = package_exts.iter().any(|ext| name_lower.ends_with(ext));
        let is_binary = !is_archive && !is_package;

        let category = match (has_os, has_arch) {
            (true, true) => {
                if is_archive {
                    "os_arch_archive"
                } else if is_binary {
                    "os_arch_binary"
                } else {
                    "os_arch_package"
                }
            }
            (true, false) => {
                if is_archive {
                    "os_only_archive"
                } else if is_binary {
                    "os_only_binary"
                } else {
                    "os_only_package"
                }
            }
            (false, true) => {
                if is_archive {
                    "arch_only_archive"
                } else if is_binary {
                    "arch_only_binary"
                } else {
                    "arch_only_package"
                }
            }
            (false, false) => continue,
        };

        if let Some(list) = candidates.get_mut(category) {
            list.push(asset.clone());
        }
    }

    candidates
}
