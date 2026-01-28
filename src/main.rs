mod cli;
mod config;
mod download;
mod install;
mod platform;
mod tests;
mod tool_id;
mod types;

use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use clap::{CommandFactory, Parser};
use cli::{Cli, Commands, ConfigAction};
use config::{load_tool_configs, normalize_key, save_tool_configs};
use install::{find_tool_executable, install_or_update_tool, pin_tool, remove_tool};
use std::env;
use std::fs;
use std::path::Path;
use std::process::Command;
use tool_id::ToolIdentifier;
use types::ToolerSettings;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Setup logging
    setup_logging(&cli)?;

    // Load configuration
    let mut config = load_tool_configs()?;

    match cli.command {
        Commands::Version => {
            println!("{} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        Commands::List => {
            list_installed_tools(&config);
        }
        Commands::Remove { tool_id } => {
            let tool_identifier = match ToolIdentifier::parse(&tool_id) {
                Ok(id) => id,
                Err(e) => {
                    if tool_id.starts_with('-') {
                        eprintln!(
                            "\nError: Invalid tool identifier '{}'. It looks like a flag.",
                            tool_id
                        );
                        eprintln!("Tooler flags (like -v, --quiet) must be placed BEFORE the subcommand: 'tooler {} remove ...'", tool_id);
                        eprintln!(
                            "Subcommand flags must be placed AFTER the tool name: 'tooler remove <tool> {}'",
                            tool_id
                        );
                        std::process::exit(1);
                    }
                    return Err(anyhow!("Invalid tool identifier: {}", e));
                }
            };
            remove_tool(&mut config, &tool_identifier.config_key())?;
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
                            match install_or_update_tool(
                                &mut config,
                                &info.tool_name,
                                &info.repo,
                                Some("latest"),
                                true,
                                None,
                            )
                            .await
                            {
                                Ok(_) => updated_count += 1,
                                Err(e) => tracing::warn!("Failed to update {}: {}", info.repo, e),
                            }
                        }
                    }
                    tracing::info!(
                        "Update process finished. {} tool(s) were checked/updated",
                        updated_count
                    );
                } else {
                    // First find the existing tool to get the correct repo
                    let existing_tool = find_tool_executable(&config, &tool_id);
                    let (tool_name, repo, tool_identifier) = if let Some(tool_info) = existing_tool
                    {
                        (
                            tool_info.tool_name.clone(),
                            tool_info.repo.clone(),
                            ToolIdentifier::parse(&tool_id).ok(),
                        )
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
                        (
                            tool_identifier.tool_name(),
                            tool_identifier.full_repo(),
                            Some(tool_identifier),
                        )
                    };

                    tracing::info!("Attempting to update {}...", repo);
                    match install_or_update_tool(
                        &mut config,
                        &tool_name,
                        &repo,
                        Some("latest"),
                        true,
                        None,
                    )
                    .await
                    {
                        Ok(_) => tracing::info!("{} updated successfully", tool_id),
                        Err(e) => {
                            tracing::error!("Failed to update tool '{}': {}", tool_id, e);
                            if e.to_string().contains("404") {
                                eprintln!("\nError: Tool '{}' not found on GitHub.", tool_id);
                                eprintln!(
                                    "Please check that the repository 'https://github.com/{}' exists.",
                                    repo
                                );
                                if let Some(id) = tool_identifier {
                                    if id.author == "unknown" {
                                        eprintln!("\nTip: If you're trying to install a new tool, use the full 'owner/repo' format.");
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
        Commands::Pull { tool_id } => {
            let tool_identifier = match ToolIdentifier::parse(&tool_id) {
                Ok(id) => {
                    if id.author == "unknown" {
                        eprintln!("\nError: Tool '{}' not found locally.", tool_id);
                        eprintln!(
                            "To install a new tool from GitHub, use the full 'owner/repo' format."
                        );
                        std::process::exit(1);
                    }
                    id
                }
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

            let repo = tool_identifier.full_repo();
            let tool_name = tool_identifier.tool_name();
            let api_version = tool_identifier.api_version();
            let version = Some(api_version.as_str());

            tracing::info!("Pulling version {} of {}...", api_version, repo);
            match install_or_update_tool(&mut config, &tool_name, &repo, version, true, None).await
            {
                Ok(path) => {
                    tracing::info!(
                        "Successfully pulled {} {} to {}",
                        repo,
                        api_version,
                        path.display()
                    );
                    // Create shim if auto_shim is enabled
                    if config.settings.auto_shim && !cfg!(windows) {
                        let bin_dir = &config.settings.bin_dir;
                        create_shim_script(bin_dir)?;
                        create_tool_symlink(bin_dir, &tool_identifier.tool_name())?;
                    }
                }
                Err(e) => {
                    tracing::error!("Failed to install tool '{}': {}", tool_id, e);
                    if e.to_string().contains("404") {
                        eprintln!("\nError: Tool '{}' not found on GitHub.", tool_id);
                        eprintln!(
                            "Please check that the repository 'https://github.com/{}' exists.",
                            tool_identifier.full_repo()
                        );
                        if tool_identifier.author == "unknown" {
                            eprintln!(
                                "\nTip: If you're trying to install a new tool, use the full 'owner/repo' format."
                            );
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
                    "bin_dir" => {
                        config.settings.bin_dir = value_str.to_string();
                        save_tool_configs(&config)?;
                        tracing::info!("Setting '{}' updated to '{}'", normalized_key, value_str);
                    }
                    _ => {
                        tracing::error!("'{}' is not a valid configuration setting. Valid settings: update-check-days, auto-shim, auto-update, bin-dir", key);
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
                    "bin_dir" => {
                        config.settings.bin_dir = ToolerSettings::default().bin_dir;
                        save_tool_configs(&config)?;
                        tracing::info!("Setting '{}' unset", key);
                    }
                    _ => {
                        tracing::error!("'{}' is not a valid configuration setting. Valid settings: update_check_days, auto_shim, auto_update, bin_dir", key);
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
                    println!("  bin-dir: {}", config.settings.bin_dir);
                    println!("\nTools: {}", config.tools.len());
                    for (key, info) in &config.tools {
                        println!("  - {}: v{} ({})", key, info.version, info.repo);
                    }
                }
            }
        },
        Commands::Run {
            tool_id,
            tool_args,
            asset,
        } => {
            let tool_identifier = match ToolIdentifier::parse(&tool_id) {
                Ok(id) => id,
                Err(_) => {
                    if tool_id.starts_with('-') {
                        // If it's -h or --help, let's show the subcommand help
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
            let version_req = tool_identifier.api_version();
            // Check for updates if not a pinned version
            if !tool_identifier.is_pinned() {
                check_for_updates(&mut config).await?;
            }
            let mut tool_info = find_tool_executable(&config, &tool_id);
            // Install if not found or if asset override is used
            if tool_info.is_none() || asset.is_some() {
                if tool_info.is_none() {
                    // Check if it's a full repo format before attempting to install
                    if tool_identifier.author == "unknown" {
                        eprintln!("\nError: Tool '{}' not found locally.", tool_id);
                        eprintln!(
                            "To install a new tool from GitHub, use the full 'owner/repo' format."
                        );
                        std::process::exit(1);
                    }

                    tracing::info!(
                        "Tool {} not found locally or is corrupted. Attempting to install...",
                        tool_id
                    );
                }
                match install_or_update_tool(
                    &mut config,
                    &tool_identifier.tool_name(),
                    &tool_identifier.full_repo(),
                    Some(&version_req),
                    false,
                    asset.as_deref(),
                )
                .await
                {
                    Ok(_) => {
                        config = load_tool_configs()?; // Reload config
                        tool_info = find_tool_executable(&config, &tool_id);
                    }
                    Err(e) => {
                        tracing::error!("Failed to install tool '{}': {}", tool_id, e);
                        if e.to_string().contains("404") {
                            eprintln!("\nError: Tool '{}' not found on GitHub.", tool_id);
                            eprintln!(
                                "Please check that the repository 'https://github.com/{}' exists.",
                                tool_identifier.full_repo()
                            );
                            if tool_identifier.author == "unknown" {
                                eprintln!(
                                    "\nTip: If you're trying to install a new tool, use the full 'owner/repo' format."
                                );
                            }
                        } else {
                            eprintln!("\nError: {}", e);
                        }
                        std::process::exit(1);
                    }
                }
            }
            if let Some(info) = tool_info {
                // Show tool age with update reason
                match info.last_accessed.parse::<DateTime<Utc>>() {
                    Ok(last_accessed) => {
                        let now = Utc::now();
                        let duration = now - last_accessed;
                        let days_since_update = duration.num_days();
                        let hours = duration.num_hours() % 24;
                        let minutes = duration.num_minutes() % 60;
                        let seconds = duration.num_seconds() % 60;
                        let is_pinned_version = info.version != "latest"
                            && !info.version.to_lowercase().contains("latest");

                        if is_pinned_version {
                            tracing::info!(
                                "{} is {} days old ({}h {}m {}s)",
                                info.repo,
                                days_since_update,
                                hours,
                                minutes,
                                seconds
                            );
                            if days_since_update > config.settings.update_check_days as i64 {
                                tracing::info!("Tool is version-pinned and not auto-updated");
                            }
                        } else {
                            tracing::info!(
                                "{} is {} days old ({}h {}m {}s)",
                                info.repo,
                                days_since_update,
                                hours,
                                minutes,
                                seconds
                            );
                        }
                    }
                    Err(_) => {
                        tracing::info!("{} age: unknown", info.repo);
                    }
                }
                // Create shim if auto_shim is enabled
                if config.settings.auto_shim && !cfg!(windows) {
                    let bin_dir = &config.settings.bin_dir;
                    create_shim_script(bin_dir)?;
                    create_tool_symlink(bin_dir, &tool_identifier.tool_name())?;
                }
                // Update last accessed time
                let key = tool_identifier.config_key();
                let executable_path = info.executable_path.clone();
                // Update config in separate scope
                {
                    if let Some(tool_info) = config.tools.get_mut(&key) {
                        tool_info.last_accessed = Utc::now().to_rfc3339();
                        save_tool_configs(&config)?;
                    }
                }
                // Execute tool
                let mut cmd = Command::new(&executable_path);
                cmd.args(&tool_args);
                tracing::debug!("Executing: {:?} {:?}", executable_path, tool_args);
                let mut child = cmd.spawn()?;
                let status = child.wait()?;
                std::process::exit(status.code().unwrap_or(1));
            } else {
                tracing::error!("Failed to find or install executable for {}", tool_id);
                std::process::exit(1);
            }
        }
        Commands::Pin { tool_id } => {
            pin_tool(&mut config, &tool_id)?;
        }
        Commands::Info { tool_id } => {
            if let Some(info) = find_tool_executable(&config, &tool_id) {
                println!("--- Tool Information ---");
                println!("  Name:          {}", info.tool_name);
                println!("  Repository:    {}", info.repo);
                println!("  Version:       {}", info.version);
                println!("  Installed at:  {}", info.installed_at);
                println!("  Last accessed: {}", info.last_accessed);
                println!("  Install type:  {}", info.install_type);
                println!("  Pinned:        {}", info.pinned);
                println!("  Path:          {}", info.executable_path);
                println!("------------------------");
            } else {
                tracing::error!(
                    "Tool '{}' not found. Try `tooler list` to see installed tools.",
                    tool_id
                );
                std::process::exit(1);
            }
        }
    }

    Ok(())
}

fn setup_logging(cli: &Cli) -> Result<()> {
    use tracing_subscriber::{fmt, EnvFilter};

    let level = if cli.quiet {
        "error"
    } else if cli.verbose == 0 {
        "warn"
    } else if cli.verbose == 1 {
        "info"
    } else {
        "debug"
    };

    // Support LOG_LEVEL and TOOLER_LOG_LEVEL
    let filter = EnvFilter::try_from_default_env()
        .or_else(|_| {
            let env_val = std::env::var("TOOLER_LOG_LEVEL")
                .or_else(|_| std::env::var("LOG_LEVEL"))
                .unwrap_or_else(|_| level.to_string());
            EnvFilter::try_new(env_val)
        })
        .unwrap_or_else(|_| EnvFilter::new(level));

    fmt()
        .with_env_filter(filter)
        .with_target(false)
        .with_thread_ids(false)
        .with_thread_names(false)
        .init();

    Ok(())
}

fn list_installed_tools(config: &types::ToolerConfig) {
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

        // Extract architecture from path if possible
        let arch = if info.executable_path.contains("__") {
            info.executable_path
                .split("__")
                .nth(2)
                .and_then(|s| s.split('/').next())
                .unwrap_or("unknown")
        } else {
            "unknown"
        };

        // Calculate age and eligibility
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

        println!(
            "  - {} ({}) {}{}[{} | {} | {}]{}",
            info.repo,
            info.version,
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

async fn check_for_updates(config: &mut types::ToolerConfig) -> Result<()> {
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

    // 1. Identify tools that need checking (all unpinned tools)
    let stale_tools: Vec<(String, String, String, String)> = config
        .tools
        .iter()
        .filter_map(|(key, info)| {
            if info.pinned {
                return None;
            }
            if let Ok(last_accessed) = info.last_accessed.parse::<DateTime<Utc>>() {
                let days_since_update = (now - last_accessed).num_days();
                if days_since_update > config.settings.update_check_days as i64 {
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

    // 2. Check for updates on GitHub (does not borrow config)
    for (key, name, repo, version) in stale_tools {
        tracing::info!("Checking for update for {} (current: {})...", repo, version);

        if let Ok(release) = install::get_gh_release_info(&repo, Some("latest")).await {
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
        } else {
            tracing::warn!(
                "Could not get latest release for {} during update check",
                repo
            );
        }
    }

    // 3. Perform auto-updates (mutably borrows config)
    if !tools_to_auto_update.is_empty() {
        eprintln!("Auto-updating {} stale tools...", tools_to_auto_update.len());
    }
    for (name, repo, version) in tools_to_auto_update {
        tracing::info!("Auto-updating {} to {}...", repo, version);
        match install_or_update_tool(config, &name, &repo, Some(&version), true, None).await {
            Ok(_) => tracing::info!("{} auto-updated successfully", repo),
            Err(e) => tracing::error!("Failed to auto-update {}: {}", repo, e),
        }
    }

    // 4. Update last_accessed timestamps for all checked tools
    for key in keys_to_update {
        if let Some(tool_info) = config.tools.get_mut(&key) {
            tool_info.last_accessed = now.to_rfc3339();
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

fn create_shim_script(bin_dir: &str) -> Result<()> {
    let shim_path = Path::new(bin_dir).join("tooler-shim");
    if !shim_path.exists() {
        fs::create_dir_all(bin_dir)?;
        let shim_content =
            "#!/bin/bash\ntool_name=$(basename \"$0\")\nexec tooler run \"$tool_name\" \"$@\"\n";
        fs::write(&shim_path, shim_content)?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&shim_path)?.permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&shim_path, perms)?;
        }

        tracing::info!("Created shim script at {}", shim_path.display());
    } else {
        // Verify existing shim is a script, not a binary
        if let Ok(metadata) = fs::metadata(&shim_path) {
            if metadata.is_file() {
                // Check if it's a script by reading first few bytes
                if let Ok(content) = fs::read_to_string(&shim_path) {
                    if !content.starts_with("#!/bin/bash") {
                        tracing::warn!("tooler-shim exists but is not a script. Recreating...");
                        let shim_content = "#!/bin/bash\ntool_name=$(basename \"$0\")\nexec tooler run \"$tool_name\" \"$@\"\n";
                        fs::write(&shim_path, shim_content)?;

                        #[cfg(unix)]
                        {
                            use std::os::unix::fs::PermissionsExt;
                            let mut perms = fs::metadata(&shim_path)?.permissions();
                            perms.set_mode(0o755);
                            fs::set_permissions(&shim_path, perms)?;
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

fn create_tool_symlink(bin_dir: &str, tool_name: &str) -> Result<()> {
    let shim_path = Path::new(bin_dir).join("tooler-shim");
    let symlink_path = Path::new(bin_dir).join(tool_name);

    // Don't create symlink for tooler-shim itself
    if tool_name == "tooler-shim" {
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
