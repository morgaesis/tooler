mod cli;
mod config;
mod download;
mod install;
mod platform;
mod tool_id;
mod types;

use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use clap::{CommandFactory, Parser};
use cli::{Cli, Commands, ConfigAction};
use config::{load_tool_configs, normalize_key, save_tool_configs};
use download::is_executable;
use install::{
    check_for_updates, find_all_executables_in_tool_dir, find_tool_entry, find_tool_executable,
    install_or_update_tool, list_installed_tools, pin_tool, reinstall_configured_tool, remove_tool,
};
use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use tool_id::ToolIdentifier;
use types::{ReleaseBodyPolicy, ToolerConfig, ToolerSettings};

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Setup logging
    setup_logging(&cli)?;
    tracing::info!("Logging initialized");

    // Load configuration
    let mut config = load_tool_configs()?;

    match cli.command {
        Commands::External(args) => {
            if args.is_empty() {
                Cli::command().print_help()?;
                return Ok(());
            }
            let tool_id = args[0].clone();
            let tool_args = args[1..].to_vec();
            execute_run(&mut config, tool_id, tool_args, None, None).await?;
        }
        Commands::Run {
            tool_id,
            tool_args,
            asset,
            parse_release_body,
            no_parse_release_body,
        } => {
            let parse_body = if parse_release_body {
                Some(true)
            } else if no_parse_release_body {
                Some(false)
            } else {
                None
            };
            execute_run(&mut config, tool_id, tool_args, asset, parse_body).await?;
        }
        Commands::Version => {
            // Version is handled by clap's #[command(version = ...)] attribute
            // but we keep this for explicit 'tooler version' command
            let version_str = if let Some(tag) = option_env!("TOOLER_GIT_TAG") {
                tag.to_string()
            } else {
                let commit = option_env!("TOOLER_GIT_COMMIT").unwrap_or("unknown");
                let branch = option_env!("TOOLER_GIT_BRANCH").unwrap_or("unknown");
                format!("v{}-{} ({})", env!("CARGO_PKG_VERSION"), commit, branch)
            };
            println!("{} {}", env!("CARGO_PKG_NAME"), version_str);
            return Ok(());
        }
        Commands::List => {
            if let Ok(count) = install::recover_all_installed_tools(&mut config) {
                if count > 0 {
                    tracing::info!("Recovered {} tools from local installation", count);
                }
            }
            list_installed_tools(&config);
        }
        Commands::Remove { tool_id } => {
            let key = find_tool_entry(&config, &tool_id).map(|(k, _)| k.clone());
            if let Some(key) = key {
                remove_tool(&mut config, &key)?;
            } else {
                return Err(anyhow!("Tool '{}' not found in configuration", tool_id));
            }
        }
        Commands::Update { tool_id } => {
            if let Some(tool_id) = tool_id {
                if tool_id == "all" {
                    tracing::info!("Updating all applicable tools...");
                    let mut updated_count = 0;
                    let keys_to_update: Vec<String> = config
                        .tools
                        .keys()
                        .filter(|k| !k.contains(':')) // Only non-version-pinned tools
                        .cloned()
                        .collect();
                    for key in keys_to_update {
                        if let Some(info) = config.tools.get(&key).cloned() {
                            let old_version = info.version.clone();
                            match install_or_update_tool(&mut config, &info.repo, true, None, None)
                                .await
                            {
                                Ok(_) => {
                                    let new_version = config
                                        .tools
                                        .get(&key)
                                        .map(|t| t.version.clone())
                                        .unwrap_or_else(|| "unknown".to_string());
                                    report_update(&info.repo, Some(&old_version), &new_version);
                                    updated_count += 1;
                                }
                                Err(e) => tracing::warn!("Failed to update {}: {}", info.repo, e),
                            }
                        }
                    }
                    eprintln!(
                        "Update process finished. {} tool(s) were checked/updated.",
                        updated_count
                    );
                } else {
                    let existing_tool = find_tool_executable(&config, &tool_id);
                    let old_version = existing_tool.as_ref().map(|t| t.version.clone());
                    let (repo, tool_identifier) = if let Some(tool_info) = existing_tool {
                        (tool_info.repo.clone(), ToolIdentifier::parse(&tool_id).ok())
                    } else {
                        let tool_identifier = match ToolIdentifier::parse(&tool_id) {
                            Ok(id) => id,
                            Err(e) => {
                                if tool_id.starts_with('-') {
                                    eprintln!("\nError: Invalid tool identifier '{}'. It looks like a flag.", tool_id);
                                    eprintln!("Tooler flags (like -v, --quiet) must be placed BEFORE the subcommand: 'tooler {} update ...'", tool_id);
                                    eprintln!("Subcommand flags must be placed AFTER the tool name: 'tooler update <tool> {}'", tool_id);
                                    std::process::exit(1);
                                }
                                return Err(anyhow!("Invalid tool identifier: {}", e));
                            }
                        };
                        (tool_identifier.full_repo(), Some(tool_identifier))
                    };

                    tracing::info!("Attempting to update {}...", repo);
                    match install_or_update_tool(&mut config, &repo, true, None, None).await {
                        Ok(path) => {
                            handle_self_update(&path, &repo)?;
                            let new_version = find_tool_executable(&config, &tool_id)
                                .map(|t| t.version)
                                .unwrap_or_else(|| "unknown".to_string());
                            report_update(&tool_id, old_version.as_deref(), &new_version);
                        }
                        Err(e) => {
                            tracing::error!("Failed to update tool '{}': {}", tool_id, e);
                            if e.to_string().contains("404") {
                                match tool_identifier
                                    .as_ref()
                                    .map(|id| id.forge.clone())
                                    .unwrap_or(types::Forge::GitHub)
                                {
                                    types::Forge::GitHub => {
                                        eprintln!(
                                            "\nError: Tool '{}' not found on GitHub.",
                                            tool_id
                                        );
                                        eprintln!(
                                            "Please check that the repository 'https://github.com/{}' exists.",
                                            repo
                                        );
                                    }
                                    types::Forge::Url => {
                                        eprintln!(
                                            "\nError: Tool '{}' (URL) not found or returned 404.",
                                            tool_id
                                        );
                                        eprintln!(
                                            "Please check that the URL '{}' is still valid.",
                                            repo
                                        );
                                    }
                                }
                            } else {
                                eprintln!("\nError: {}", e);
                            }
                            std::process::exit(1);
                        }
                    }
                }
            } else {
                tracing::error!("Please specify a tool to update or use 'all' to update all tools");
                std::process::exit(1);
            }
        }
        Commands::Pull {
            tool_id,
            asset,
            parse_release_body,
            no_parse_release_body,
        } => {
            let tool_identifier = match ToolIdentifier::parse(&tool_id) {
                Ok(id) => id,
                Err(e) => {
                    if tool_id.starts_with('-') {
                        eprintln!(
                            "\nError: Invalid tool identifier '{}'. It looks like a flag.",
                            tool_id
                        );
                        eprintln!("Tooler flags (like -v, --quiet) must be placed BEFORE the subcommand: 'tooler {} pull ...'", tool_id);
                        eprintln!(
                            "Subcommand flags must be placed AFTER the tool name: 'tooler pull <tool> {}'",
                            tool_id
                        );
                        std::process::exit(1);
                    }
                    return Err(anyhow!("Invalid tool identifier: {}", e));
                }
            };

            let parse_body = if parse_release_body {
                Some(true)
            } else if no_parse_release_body {
                Some(false)
            } else {
                None
            };

            let existing = find_tool_executable(&config, &tool_id);
            let old_version = existing.as_ref().map(|t| t.version.clone());
            let repo_to_pull = if let Some(existing) = existing {
                tracing::info!(
                    "Tool '{}' resolves to repository {}",
                    tool_id,
                    existing.repo
                );
                existing.repo.clone()
            } else {
                tracing::info!("Pulling {}...", tool_id);
                tool_id.clone()
            };

            match install_or_update_tool(
                &mut config,
                &repo_to_pull,
                true,
                asset.as_deref(),
                parse_body,
            )
            .await
            {
                Ok(path) => {
                    handle_self_update(&path, &repo_to_pull)?;
                    let new_version = find_tool_executable(&config, &tool_id)
                        .map(|t| t.version)
                        .unwrap_or_else(|| "unknown".to_string());
                    report_update(&repo_to_pull, old_version.as_deref(), &new_version);
                    tracing::info!("Path: {}", path.display());
                    if config.settings.auto_shim {
                        if let Err(e) = setup_auto_shim(
                            &config.settings.bin_dir,
                            &tool_identifier.tool_name(),
                            &path,
                        ) {
                            tracing::warn!(
                                "auto-shim skipped (bin_dir={}): {}. Pull itself succeeded — set bin-dir to a writable path or disable auto-shim.",
                                config.settings.bin_dir,
                                e
                            );
                        }
                    }
                }
                Err(e) => {
                    if let Some(gh_error) = e.downcast_ref::<install::github::GitHubReleaseError>()
                    {
                        display_github_error(&tool_id, gh_error);
                    } else if tool_identifier.forge == types::Forge::Url {
                        eprintln!("\nError: Tool '{}' could not be fetched from URL.", tool_id);
                        if let Some(url) = &tool_identifier.url {
                            eprintln!("URL: {}", url);
                        }
                    } else {
                        eprintln!("\nError: {}", e);
                    }
                    std::process::exit(1);
                }
            }
        }
        Commands::Config { action } => match action {
            ConfigAction::Get { key } => {
                if let Some(key) = key {
                    let normalized_key = normalize_key(&key);
                    let value = match normalized_key.as_str() {
                        "update_check_days" => config.settings.update_check_days.to_string(),
                        "auto_shim" => config.settings.auto_shim.to_string(),
                        "auto_update" => config.settings.auto_update.to_string(),
                        "parse_release_body" => config.settings.parse_release_body.to_string(),
                        "bin_dir" => config.settings.bin_dir.clone(),
                        _ => format!("Setting '{}' not found", key),
                    };
                    println!("{}", value);
                } else {
                    println!("--- Tooler Settings ---");
                    for (k, v) in &[
                        (
                            "update-check-days",
                            &config.settings.update_check_days.to_string(),
                        ),
                        ("auto-shim", &config.settings.auto_shim.to_string()),
                        ("auto-update", &config.settings.auto_update.to_string()),
                        (
                            "parse-release-body",
                            &config.settings.parse_release_body.to_string(),
                        ),
                        ("bin-dir", &config.settings.bin_dir),
                    ] {
                        println!("  {}: {}", k, v);
                    }
                }
            }
            ConfigAction::Set { args } => {
                let (key, value_str) = if args.len() == 1 {
                    if let Some((k, v)) = args[0].split_once('=') {
                        (k.to_string(), v.to_string())
                    } else {
                        tracing::error!("Invalid format. Use 'key=value' or 'key value'.");
                        std::process::exit(1);
                    }
                } else if args.len() >= 2 {
                    (args[0].clone(), args[1..].join(" "))
                } else {
                    tracing::error!("Invalid format. Use 'key=value' or 'key value'.");
                    std::process::exit(1);
                };

                let normalized_key = normalize_key(&key);
                match normalized_key.as_str() {
                    "update_check_days" => {
                        if let Ok(days) = value_str.parse::<i32>() {
                            config.settings.update_check_days = days;
                            save_tool_configs(&config)?;
                            tracing::info!("Setting '{}' updated to '{}'", normalized_key, days);
                        } else {
                            tracing::error!("Invalid value for '{}'", key);
                        }
                    }
                    "auto_shim" => {
                        let value = value_str.to_lowercase() == "true" || value_str == "1";
                        config.settings.auto_shim = value;
                        save_tool_configs(&config)?;
                        tracing::info!("Setting '{}' updated to '{}'", normalized_key, value);
                    }
                    "auto_update" => {
                        let value = value_str.to_lowercase() == "true" || value_str == "1";
                        config.settings.auto_update = value;
                        save_tool_configs(&config)?;
                        tracing::info!("Setting '{}' updated to '{}'", normalized_key, value);
                    }
                    "parse_release_body" => {
                        let Some(value) = ReleaseBodyPolicy::parse(&value_str) else {
                            tracing::error!(
                                "Invalid value for '{}'. Use ask, always, or never.",
                                key
                            );
                            std::process::exit(1);
                        };
                        config.settings.parse_release_body = value.clone();
                        save_tool_configs(&config)?;
                        tracing::info!("Setting '{}' updated to '{}'", normalized_key, value);
                    }
                    "bin_dir" => {
                        config.settings.bin_dir = value_str.to_string();
                        save_tool_configs(&config)?;
                        tracing::info!("Setting '{}' updated to '{}'", normalized_key, value_str);
                    }
                    _ => {
                        tracing::error!(
                            "'{}' is not a valid configuration setting. Valid settings: update-check-days, auto-shim, auto-update, parse-release-body, bin-dir",
                            key
                        );
                    }
                }
            }
            ConfigAction::Unset { key } => {
                let key = normalize_key(&key);
                match key.as_str() {
                    "update_check_days" => {
                        config.settings.update_check_days =
                            ToolerSettings::default().update_check_days;
                        save_tool_configs(&config)?;
                        tracing::info!("Setting '{}' unset", key);
                    }
                    "auto_shim" => {
                        config.settings.auto_shim = ToolerSettings::default().auto_shim;
                        save_tool_configs(&config)?;
                        tracing::info!("Setting '{}' unset", key);
                    }
                    "auto_update" => {
                        config.settings.auto_update = ToolerSettings::default().auto_update;
                        save_tool_configs(&config)?;
                        tracing::info!("Setting '{}' unset", key);
                    }
                    "parse_release_body" => {
                        config.settings.parse_release_body =
                            ToolerSettings::default().parse_release_body;
                        save_tool_configs(&config)?;
                        tracing::info!("Setting '{}' unset", key);
                    }
                    "bin_dir" => {
                        config.settings.bin_dir = ToolerSettings::default().bin_dir;
                        save_tool_configs(&config)?;
                        tracing::info!("Setting '{}' unset", key);
                    }
                    _ => {
                        tracing::error!(
                            "'{}' is not a valid configuration setting. Valid settings: update-check-days, auto-shim, auto-update, parse-release-body, bin-dir",
                            key
                        );
                    }
                }
            }
            ConfigAction::Show { format } => {
                if format == "json" {
                    let json = serde_json::to_string_pretty(&config)?;
                    println!("{}", json);
                } else if format == "yaml" {
                    let yaml = serde_yaml::to_string(&config)?;
                    println!("{}", yaml);
                } else {
                    println!("--- Tooler Configuration ---");
                    println!("Settings:");
                    println!("  update-check-days: {}", config.settings.update_check_days);
                    println!("  auto-shim: {}", config.settings.auto_shim);
                    println!("  auto-update: {}", config.settings.auto_update);
                    println!(
                        "  parse-release-body: {}",
                        config.settings.parse_release_body
                    );
                    println!("  bin-dir: {}", config.settings.bin_dir);

                    if !config.aliases.is_empty() {
                        println!("\nAliases:");
                        for (name, target) in &config.aliases {
                            println!("  {} -> {}", name, target);
                        }
                    }

                    println!("\nTools: {}", config.tools.len());
                    for (key, info) in &config.tools {
                        println!("  - {}: v{} ({})", key, info.version, info.repo);
                    }
                }
            }
        },
        Commands::Pin { tool_id } => {
            pin_tool(&mut config, &tool_id)?;
        }
        Commands::Alias {
            name,
            target,
            remove,
        } => {
            if remove {
                if config.aliases.remove(&name).is_some() {
                    save_tool_configs(&config)?;
                    tracing::info!("Alias '{}' removed", name);
                } else {
                    tracing::warn!("Alias '{}' not found", name);
                }
            } else if let Some(target) = target {
                config.aliases.insert(name.clone(), target.clone());
                save_tool_configs(&config)?;
                tracing::info!("Alias '{}' set to '{}'", name, target);
            } else if let Some(target) = config.aliases.get(&name) {
                println!("{} -> {}", name, target);
            } else {
                return Err(anyhow!("Alias '{}' not found", name));
            }
        }
        Commands::Info { tool_ids } => {
            let mut any_missing = false;
            let os = platform::get_system_info().os;
            for tool_id in &tool_ids {
                let mut info = find_tool_executable(&config, tool_id);

                // Invalidate stale entries (path missing or non-executable) so recovery runs.
                let resolved_repo = info.as_ref().map(|i| i.repo.clone());
                let mut configured_info = None;
                if let Some(ref i) = info {
                    let p = Path::new(&i.executable_path);
                    if !p.exists() || !is_executable(p, &os) {
                        eprintln!(
                            "Note: cached entry for '{}' points at missing/invalid binary ({}). Attempting recovery...",
                            tool_id, i.executable_path
                        );
                        configured_info = Some(i.clone());
                        info = None;
                    }
                }

                let recovery_target: &str = resolved_repo.as_deref().unwrap_or(tool_id);
                if info.is_none() {
                    if let Ok(Some(recovered)) = install::try_recover_tool(recovery_target) {
                        eprintln!(
                            "Recovered tool {} (v{}) from local installation.",
                            tool_id, recovered.version
                        );
                        let key = ToolIdentifier::parse(&recovered.repo)
                            .map_err(|e| anyhow!(e))?
                            .config_key();
                        config.tools.insert(key, recovered);
                        save_tool_configs(&config)?;
                        info = find_tool_executable(&config, tool_id);
                    }
                }

                if info.is_none() {
                    if let Some(configured) = configured_info {
                        eprintln!(
                            "Note: local recovery did not find a replacement binary; showing configured metadata."
                        );
                        info = Some(configured);
                    }
                }

                if let Some(info) = info {
                    let system_info = platform::get_system_info();
                    let all_binaries =
                        find_all_executables_in_tool_dir(&info.executable_path, &system_info.os);

                    println!("--- Tool Information ({}) ---", tool_id);
                    println!("  Name:          {}", info.tool_name);
                    println!("  Repository:    {}", info.repo);
                    println!("  Version:       {}", info.version);
                    println!("  Installed at:  {}", info.installed_at);
                    println!("  Last accessed: {}", info.last_accessed);
                    println!("  Install type:  {}", info.install_type);
                    println!("  Pinned:        {}", info.pinned);
                    println!("  Binaries:      {}", all_binaries.join(", "));
                    println!("  Path:          {}", info.executable_path);
                    println!("------------------------");
                } else {
                    tracing::error!(
                        "Tool '{}' not found. Try `tooler list` to see installed tools.",
                        tool_id
                    );
                    any_missing = true;
                }
            }
            if any_missing {
                std::process::exit(1);
            }
        }
    }

    Ok(())
}

async fn execute_run(
    config: &mut ToolerConfig,
    tool_id: String,
    tool_args: Vec<String>,
    asset: Option<String>,
    parse_release_body: Option<bool>,
) -> Result<()> {
    let tool_identifier = match ToolIdentifier::parse(&tool_id) {
        Ok(id) => id,
        Err(_) => {
            if tool_id.starts_with('-') {
                if tool_id == "-h" || tool_id == "--help" {
                    let mut cmd = Cli::command();
                    let sub_help = cmd
                        .get_subcommands_mut()
                        .find(|s| s.get_name() == "run")
                        .map(|s| s.render_help());

                    if let Some(help) = sub_help {
                        println!("{}", help);
                        std::process::exit(0);
                    }
                }
                eprintln!(
                    "\nError: Invalid tool identifier '{}'. It looks like a flag.",
                    tool_id
                );
                eprintln!("Tooler flags (like -v, --quiet) must be placed BEFORE the subcommand: 'tooler {} run ...'", tool_id);
                eprintln!(
                    "Subcommand flags must be placed AFTER the tool name: 'tooler run <tool> {}'",
                    tool_id
                );
                std::process::exit(1);
            }
            return Err(anyhow!("Invalid tool identifier: {}", tool_id));
        }
    };

    // Check for updates for this specific tool if not pinned
    if !tool_identifier.is_pinned() {
        if let Some(key) = find_tool_entry(config, &tool_id).map(|(k, _)| k.clone()) {
            check_for_updates(config, Some(&key)).await?;
        }
    }

    let mut tool_info = find_tool_executable(config, &tool_id);

    // Remember the resolved repo before any invalidation, so recovery & install
    // can use the real repo (e.g. "cli/cli") instead of the user's shortname ("gh").
    let resolved_repo: Option<String> = tool_info.as_ref().map(|i| i.repo.clone());
    let configured_reinstall =
        find_tool_entry(config, &tool_id).map(|(key, info)| (key.clone(), info.clone()));

    // Validate tool_info if found
    if let Some(ref info) = tool_info {
        let path = Path::new(&info.executable_path);
        if !path.exists() || !is_executable(path, &platform::get_system_info().os) {
            tracing::warn!(
                "Tool {} found in config but executable is missing or invalid. Attempting recovery...",
                tool_id
            );
            tool_info = None;
        }
    }

    let recovery_target: &str = resolved_repo.as_deref().unwrap_or(&tool_id);

    // Recovery: If tool not found in config, try to discover it locally
    if tool_info.is_none() && asset.is_none() {
        if let Ok(Some(recovered)) = install::try_recover_tool(recovery_target) {
            eprintln!(
                "Recovered tool {} (v{}) from local installation.",
                tool_id, recovered.version
            );
            let key = ToolIdentifier::parse(&recovered.repo)
                .map_err(|e| anyhow!(e))?
                .config_key();

            config.tools.insert(key, recovered);
            save_tool_configs(config)?;
            tool_info = find_tool_executable(config, &tool_id);
        }
    }

    // Install if not found or if asset override is used
    if tool_info.is_none() || asset.is_some() {
        if tool_info.is_none() {
            tracing::info!(
                "Tool {} not found locally or is corrupted. Attempting to install...",
                tool_id
            );
        }
        let install_result = if let Some((key, configured)) = configured_reinstall.as_ref() {
            reinstall_configured_tool(
                config,
                key,
                configured,
                &tool_id,
                asset.as_deref(),
                parse_release_body,
            )
            .await
        } else {
            install_or_update_tool(
                config,
                recovery_target,
                false,
                asset.as_deref(),
                parse_release_body,
            )
            .await
        };

        match install_result {
            Ok(_) => {
                *config = load_tool_configs()?; // Reload config
                tool_info = find_tool_executable(config, &tool_id);
            }
            Err(e) => {
                if let Some(gh_error) = e.downcast_ref::<install::github::GitHubReleaseError>() {
                    display_github_error(&tool_id, gh_error);
                } else if tool_identifier.forge == types::Forge::Url {
                    eprintln!("\nError: Tool '{}' could not be fetched from URL.", tool_id);
                    if let Some(url) = &tool_identifier.url {
                        eprintln!("URL: {}", url);
                    }
                } else {
                    eprintln!("\nError: {}", e);
                }
                std::process::exit(1);
            }
        }
    }

    if let Some(info) = tool_info {
        // Show tool age
        if let Ok(installed_at) = info.installed_at.parse::<DateTime<Utc>>() {
            let now = Utc::now();
            let duration = now - installed_at;
            let days_since_install = duration.num_days();
            let hours = duration.num_hours() % 24;
            let minutes = duration.num_minutes() % 60;
            let seconds = duration.num_seconds() % 60;
            let is_pinned_version =
                info.version != "latest" && !info.version.to_lowercase().contains("latest");

            if is_pinned_version {
                tracing::info!(
                    "{} is {} days old ({}h {}m {}s)",
                    info.repo,
                    days_since_install,
                    hours,
                    minutes,
                    seconds
                );
                if days_since_install > config.settings.update_check_days as i64 {
                    tracing::info!("Tool is version-pinned and not auto-updated");
                }
            } else {
                tracing::info!(
                    "{} is {} days old ({}h {}m {}s)",
                    info.repo,
                    days_since_install,
                    hours,
                    minutes,
                    seconds
                );
            }
        }

        // Create shim if auto_shim is enabled
        if config.settings.auto_shim {
            if let Err(e) = setup_auto_shim(
                &config.settings.bin_dir,
                &tool_identifier.tool_name(),
                Path::new(&info.executable_path),
            ) {
                tracing::warn!(
                    "auto-shim skipped (bin_dir={}): {}. Tool still runs from {}.",
                    config.settings.bin_dir,
                    e,
                    info.executable_path
                );
            }
        }

        // Update last accessed time
        let repo_to_match = info.repo.clone();
        let version_to_match = info.version.clone();
        let executable_path = info.executable_path.clone();

        {
            if let Some(found_info) = config
                .tools
                .values_mut()
                .find(|t| t.repo == repo_to_match && t.version == version_to_match)
            {
                found_info.last_accessed = Utc::now().to_rfc3339();
                save_tool_configs(config)?;
            }
        }

        // Execute tool
        let mut cmd = Command::new(&executable_path);

        cmd.args(&tool_args);
        tracing::debug!("Executing: {:?} {:?}", executable_path, tool_args);
        let mut child = cmd.spawn().map_err(|e| {
            if e.raw_os_error() == Some(8) {
                anyhow!(
                    "Failed to execute '{}': Exec format error.\n\n\
                    Check the file type with 'file {}'",
                    executable_path,
                    executable_path
                )
            } else {
                anyhow!("Failed to execute tool: {}", e)
            }
        })?;
        let status = child.wait()?;
        std::process::exit(status.code().unwrap_or(1));
    }

    tracing::error!("Failed to find or install executable for {}", tool_id);
    std::process::exit(1);
}

fn setup_logging(cli: &Cli) -> Result<()> {
    use tracing_subscriber::EnvFilter;

    let level = if cli.quiet {
        "error"
    } else if cli.verbose == 0 {
        "warn"
    } else if cli.verbose == 1 {
        "info"
    } else {
        "debug"
    };

    let filter = EnvFilter::try_from_default_env()
        .or_else(|_| {
            let env_val = std::env::var("TOOLER_LOG_LEVEL")
                .or_else(|_| std::env::var("LOG_LEVEL"))
                .unwrap_or_else(|_| level.to_string());
            EnvFilter::try_new(env_val)
        })
        .unwrap_or_else(|_| EnvFilter::new(level));

    let destinations = LogDestinations::parse(&log_destination_value(cli))?;
    init_logging_subscriber(filter, destinations)
}

fn log_destination_value(cli: &Cli) -> String {
    cli.log_destination
        .clone()
        .unwrap_or_else(|| "stderr,logfile".to_string())
}

fn init_logging_subscriber(
    filter: tracing_subscriber::EnvFilter,
    destinations: LogDestinations,
) -> Result<()> {
    use tracing_subscriber::fmt::writer::MakeWriterExt;

    match (
        destinations.stderr,
        destinations.stdout,
        destinations.logfile,
    ) {
        (false, false, false) => init_logging_with_writer(filter, io::sink),
        (true, false, false) => init_logging_with_writer(filter, io::stderr),
        (false, true, false) => init_logging_with_writer(filter, io::stdout),
        (true, true, false) => init_logging_with_writer(filter, io::stderr.and(io::stdout)),
        (false, false, true) => init_logging_with_writer(filter, Mutex::new(open_log_file()?)),
        (true, false, true) => {
            init_logging_with_writer(filter, io::stderr.and(Mutex::new(open_log_file()?)))
        }
        (false, true, true) => {
            init_logging_with_writer(filter, io::stdout.and(Mutex::new(open_log_file()?)))
        }
        (true, true, true) => init_logging_with_writer(
            filter,
            io::stderr.and(io::stdout).and(Mutex::new(open_log_file()?)),
        ),
    }
}

fn init_logging_with_writer<W>(filter: tracing_subscriber::EnvFilter, writer: W) -> Result<()>
where
    W: for<'writer> tracing_subscriber::fmt::MakeWriter<'writer> + Send + Sync + 'static,
{
    use tracing_subscriber::fmt;

    fmt()
        .with_env_filter(filter)
        .with_writer(writer)
        .with_target(false)
        .with_thread_ids(false)
        .with_thread_names(false)
        .init();

    Ok(())
}

struct LogDestinations {
    stderr: bool,
    stdout: bool,
    logfile: bool,
}

impl LogDestinations {
    fn parse(value: &str) -> Result<Self> {
        let mut destinations = Self {
            stderr: false,
            stdout: false,
            logfile: false,
        };
        let mut saw_none = false;
        let mut saw_destination = false;

        for raw in value.split(',') {
            let destination = raw.trim().to_ascii_lowercase();
            if destination.is_empty() {
                continue;
            }

            match destination.as_str() {
                "stderr" => {
                    destinations.stderr = true;
                    saw_destination = true;
                }
                "stdout" => {
                    destinations.stdout = true;
                    saw_destination = true;
                }
                "logfile" | "file" => {
                    destinations.logfile = true;
                    saw_destination = true;
                }
                "none" => saw_none = true,
                _ => {
                    return Err(anyhow!(
                        "Invalid log destination '{}'. Use stderr, stdout, logfile, or none.",
                        raw
                    ));
                }
            }
        }

        if saw_none && saw_destination {
            return Err(anyhow!(
                "Log destination 'none' cannot be combined with other destinations"
            ));
        }

        if !saw_none && !saw_destination {
            return Err(anyhow!(
                "At least one log destination is required: stderr, stdout, logfile, or none"
            ));
        }

        Ok(destinations)
    }
}

fn default_log_file_path() -> Result<PathBuf> {
    if let Ok(path) = env::var("TOOLER_LOG_FILE") {
        return Ok(PathBuf::from(path));
    }

    let state_dir = if let Ok(path) = env::var("TOOLER_STATE_DIR") {
        PathBuf::from(path)
    } else if let Ok(path) = env::var("XDG_STATE_HOME") {
        PathBuf::from(path).join("tooler")
    } else {
        #[cfg(windows)]
        {
            dirs::data_local_dir()
                .ok_or_else(|| anyhow!("Could not determine local data directory"))?
                .join("tooler")
        }

        #[cfg(not(windows))]
        {
            let home = env::var("HOME").unwrap_or_else(|_| ".".to_string());
            PathBuf::from(home).join(".local/state/tooler")
        }
    };

    Ok(state_dir.join("tooler.log"))
}

fn open_log_file() -> Result<fs::File> {
    let path = default_log_file_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    Ok(fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?)
}

/// Report the outcome of an install/update to stderr in a default-verbosity
/// visible form. `old` is `None` for a fresh install.
fn report_update(tool_id: &str, old: Option<&str>, new: &str) {
    let normalize = |v: &str| v.trim_start_matches('v').to_string();
    let new_n = normalize(new);
    match old {
        Some(o) if normalize(o) == new_n => {
            eprintln!("{} already at v{}", tool_id, new_n);
        }
        Some(o) => {
            eprintln!("Updated {}: v{} -> v{}", tool_id, normalize(o), new_n);
        }
        None => {
            eprintln!("Installed {} v{}", tool_id, new_n);
        }
    }
}

/// Set up shim + symlinks for a tool. Best-effort: returns an error on the first
/// I/O failure so the caller can warn instead of aborting the parent command.
fn setup_auto_shim(bin_dir: &str, primary_name: &str, executable_path: &Path) -> Result<()> {
    create_shim_script(bin_dir)?;
    create_tool_symlink(bin_dir, primary_name)?;

    let system_info = platform::get_system_info();
    let all_binaries =
        find_all_executables_in_tool_dir(&executable_path.to_string_lossy(), &system_info.os);
    for binary in &all_binaries {
        create_tool_symlink(bin_dir, binary)?;
        let base = strip_platform_suffix(binary);
        if base != *binary {
            create_tool_symlink(bin_dir, &base)?;
        }
    }
    Ok(())
}

fn create_shim_script(bin_dir: &str) -> Result<()> {
    #[cfg(windows)]
    {
        let shim_path = Path::new(bin_dir).join("tooler-shim.cmd");
        fs::create_dir_all(bin_dir)?;
        let shim_content =
            "@echo off\r\nset \"tool_name=%~n0\"\r\ntooler run \"%tool_name%\" %*\r\n";
        if !shim_path.exists() || fs::read_to_string(&shim_path).unwrap_or_default() != shim_content
        {
            fs::write(&shim_path, shim_content)?;
            tracing::info!("Created shim script at {}", shim_path.display());
        }
        return Ok(());
    }

    #[cfg(not(windows))]
    {
        let shim_path = Path::new(bin_dir).join("tooler-shim");
        fs::create_dir_all(bin_dir)?;

        let tooler_bin = std::env::current_exe()?;
        let tooler_bin = shell_quote(&tooler_bin.to_string_lossy());
        let shim_content = format!(
            r#"#!/bin/bash
set -u

tool_name="${{0##*/}}"
tooler_bin={tooler_bin}
log_dir="${{TOOLER_STATE_DIR:-${{XDG_STATE_HOME:-${{HOME:-.}}/.local/state}}/tooler}}"
log_file="${{TOOLER_SHIM_LOG:-$log_dir/shim.log}}"

log_failure() {{
    mkdir -p "$log_dir" 2>/dev/null || true
    timestamp=$(date -u +%Y-%m-%dT%H:%M:%SZ 2>/dev/null || printf unknown)
    printf '%s tool=%s error=%s tooler_bin=%s\n' "$timestamp" "$tool_name" "$1" "$tooler_bin" >> "$log_file" 2>/dev/null || true
}}

if [ ! -x "$tooler_bin" ]; then
    log_failure "tooler-binary-missing-or-not-executable"
    fallback=$(command -v tooler 2>/dev/null || true)
    if [ -n "$fallback" ] && [ -x "$fallback" ] && [ "$fallback" != "$0" ]; then
        tooler_bin="$fallback"
    else
        echo "tooler shim error: tooler binary is missing or not executable: $tooler_bin" >&2
        echo "tooler shim error: details logged to $log_file" >&2
        exit 127
    fi
fi

exec "$tooler_bin" run "$tool_name" "$@"
status=$?
log_failure "exec-failed-status-$status"
exit "$status"
"#
        );

        let needs_write = match fs::read_to_string(&shim_path) {
            Ok(existing) => existing != shim_content,
            Err(_) => true,
        };

        if needs_write {
            fs::write(&shim_path, shim_content)?;

            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mut perms = fs::metadata(&shim_path)?.permissions();
                perms.set_mode(0o755);
                fs::set_permissions(&shim_path, perms)?;
            }
            tracing::info!("Created shim script at {}", shim_path.display());
        }
        Ok(())
    }
}

#[cfg(not(windows))]
fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

/// Strip platform-specific suffixes from a binary name.
/// e.g. "cmk.linux.x86-64" -> "cmk", "tool-linux-amd64" -> "tool"
/// but "wt-cli" stays "wt-cli" (no platform tokens).
fn strip_platform_suffix(name: &str) -> String {
    let platform_tokens = [
        "linux", "darwin", "macos", "windows", "win", "win32", "win64", "amd64", "x86_64",
        "x86-64", "arm64", "aarch64", "armv7", "i386", "i686", "gnu", "musl", "msvc",
    ];

    // Split on dots and dashes, keep segments that aren't platform tokens
    let parts: Vec<&str> = name.split('.').collect();
    if parts.len() > 1 {
        // Try stripping dot-separated platform tokens from the end
        let mut keep = parts.len();
        for i in (1..parts.len()).rev() {
            let segment_lower = parts[i].to_lowercase();
            // Check if this segment or any dash-separated part is a platform token
            let is_platform = segment_lower
                .split('-')
                .all(|p| platform_tokens.contains(&p) || p.chars().all(|c| c.is_ascii_digit()));
            if is_platform {
                keep = i;
            } else {
                break;
            }
        }
        if keep < parts.len() {
            return parts[..keep].join(".");
        }
    }

    name.to_string()
}

/// Check if a tool identifier refers to tooler itself, and if so, replace the
/// currently running binary with the newly downloaded one.
fn handle_self_update(new_executable: &Path, tool_id: &str) -> Result<bool> {
    let tool_identifier = match ToolIdentifier::parse(tool_id) {
        Ok(id) => id,
        Err(_) => return Ok(false),
    };

    let is_self = tool_identifier.tool_name() == "tooler"
        && (tool_identifier.author == "morgaesis" || tool_identifier.author == "unknown");

    if !is_self {
        return Ok(false);
    }

    let current_exe = env::current_exe()?;
    tracing::info!(
        "Self-update: replacing {} with {}",
        current_exe.display(),
        new_executable.display()
    );

    // On Unix, we can replace the running binary by writing to a new file and renaming.
    // The old inode stays alive until the process exits.
    let tmp_path = current_exe.with_extension("new");
    fs::copy(new_executable, &tmp_path)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&tmp_path)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&tmp_path, perms)?;
    }

    fs::rename(&tmp_path, &current_exe)?;
    tracing::debug!("Self-update: replaced binary at {}", current_exe.display());

    Ok(true)
}

/// Display a user-friendly error message for GitHub release errors.
/// Returns the appropriate exit code.
fn display_github_error(tool_id: &str, gh_error: &install::github::GitHubReleaseError) {
    use install::github::GitHubReleaseError;
    match gh_error {
        GitHubReleaseError::TagNotFound { repo, version } => {
            tracing::warn!("Release tag '{}' not found in {}", version, repo);
            eprintln!("\nError: Release tag '{}' not found in {}.", version, repo);
            eprintln!("Check available tags at: https://github.com/{}/tags", repo);
        }
        GitHubReleaseError::LatestNotFound { repo } => {
            tracing::warn!("No releases found for {}", repo);
            eprintln!("\nError: No releases found for {}.", repo);
            if repo.contains('/') {
                eprintln!("Check available tags at: https://github.com/{}/tags", repo);
            } else {
                eprintln!(
                    "Specify the full repository: tooler install <owner>/{}",
                    repo
                );
            }
        }
        GitHubReleaseError::RepoNotFound { repo } => {
            tracing::warn!("Repository '{}' not found on GitHub", repo);
            if repo.contains('/') {
                eprintln!(
                    "\nError: Repository '{}' not found on GitHub (or is private).",
                    repo
                );
            } else {
                eprintln!(
                    "\nError: Tool '{}' not found. Specify the full repository: tooler install <owner>/{}",
                    tool_id, repo
                );
            }
        }
        GitHubReleaseError::RateLimited { repo } => {
            tracing::warn!("GitHub API rate limit reached while querying {}", repo);
            eprintln!(
                "\nError: GitHub API rate limit reached. Try again later or set GITHUB_TOKEN."
            );
        }
        GitHubReleaseError::RequestFailed { repo, status } => {
            tracing::error!("Failed to get release info for {}: {}", repo, status);
            eprintln!("\nError: {}", gh_error);
            if repo.contains('/') {
                eprintln!("Check available tags at: https://github.com/{}/tags", repo);
            }
        }
    }
}

fn create_tool_symlink(bin_dir: &str, tool_name: &str) -> Result<()> {
    #[cfg(windows)]
    {
        let shim_path = Path::new(bin_dir).join("tooler-shim.cmd");
        let command_name = tool_name
            .strip_suffix(".exe")
            .or_else(|| tool_name.strip_suffix(".EXE"))
            .or_else(|| tool_name.strip_suffix(".cmd"))
            .or_else(|| tool_name.strip_suffix(".CMD"))
            .or_else(|| tool_name.strip_suffix(".bat"))
            .or_else(|| tool_name.strip_suffix(".BAT"))
            .unwrap_or(tool_name);
        let tool_file_name = format!("{}.cmd", command_name);
        let symlink_path = Path::new(bin_dir).join(tool_file_name);

        if command_name.eq_ignore_ascii_case("tooler-shim")
            || command_name.eq_ignore_ascii_case("tooler")
        {
            return Ok(());
        }

        if !symlink_path.exists() {
            fs::copy(&shim_path, &symlink_path)?;
            tracing::info!(
                "Created shim {} -> {}",
                symlink_path.display(),
                shim_path.display()
            );
        }
        return Ok(());
    }

    #[cfg(not(windows))]
    {
        let shim_path = Path::new(bin_dir).join("tooler-shim");
        let symlink_path = Path::new(bin_dir).join(tool_name);

        if tool_name == "tooler-shim" || tool_name == "tooler" {
            return Ok(());
        }

        if !symlink_path.exists() {
            #[cfg(unix)]
            {
                std::os::unix::fs::symlink(&shim_path, &symlink_path)?;
            }
            #[cfg(not(unix))]
            {
                fs::copy(&shim_path, &symlink_path)?;
            }
            tracing::info!(
                "Created symlink {} -> {}",
                symlink_path.display(),
                shim_path.display()
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod auto_shim_tests {
    use super::*;

    #[test]
    fn test_auto_shim_does_not_create_tooler_command() {
        let temp_dir = tempfile::tempdir().unwrap();
        create_shim_script(temp_dir.path().to_str().unwrap()).unwrap();
        create_tool_symlink(temp_dir.path().to_str().unwrap(), "tooler").unwrap();

        #[cfg(windows)]
        let tooler_shim = temp_dir.path().join("tooler.cmd");
        #[cfg(not(windows))]
        let tooler_shim = temp_dir.path().join("tooler");

        assert!(!tooler_shim.exists());
        assert!(temp_dir
            .path()
            .join(if cfg!(windows) {
                "tooler-shim.cmd"
            } else {
                "tooler-shim"
            })
            .exists());
    }
}
