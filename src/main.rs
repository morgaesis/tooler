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
    check_for_updates, find_tool_entry, find_tool_executable, install_or_update_tool,
    list_installed_tools, pin_tool, remove_tool,
};
use std::env;
use std::fs;
use std::path::Path;
use std::process::Command;
use tool_id::ToolIdentifier;
use types::{ToolerConfig, ToolerSettings};

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Setup logging
    setup_logging(&cli)?;

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
            execute_run(&mut config, tool_id, tool_args, None).await?;
        }
        Commands::Run {
            tool_id,
            tool_args,
            asset,
        } => {
            execute_run(&mut config, tool_id, tool_args, asset).await?;
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
                            match install_or_update_tool(&mut config, &info.repo, true, None).await
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
                    let existing_tool = find_tool_executable(&config, &tool_id);
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
                    match install_or_update_tool(&mut config, &repo, true, None).await {
                        Ok(_) => tracing::info!("{} updated successfully", tool_id),
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
        Commands::Pull { tool_id } => {
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

            tracing::info!("Pulling {}...", tool_id);
            match install_or_update_tool(&mut config, &tool_id, true, None).await {
                Ok(path) => {
                    tracing::info!("Successfully pulled {} to {}", tool_id, path.display());
                    if config.settings.auto_shim && !cfg!(windows) {
                        let bin_dir = &config.settings.bin_dir;
                        create_shim_script(bin_dir)?;
                        create_tool_symlink(bin_dir, &tool_identifier.tool_name())?;
                    }
                }
                Err(e) => {
                    tracing::error!("Failed to install tool '{}': {}", tool_id, e);
                    if e.to_string().contains("404") {
                        match tool_identifier.forge {
                            types::Forge::GitHub => {
                                eprintln!("\nError: Tool '{}' not found on GitHub.", tool_id);
                                eprintln!(
                                    "Please check that the repository 'https://github.com/{}' exists.",
                                    tool_identifier.full_repo()
                                );
                            }
                            types::Forge::Url => {
                                eprintln!(
                                    "\nError: Tool '{}' (URL) not found or returned 404.",
                                    tool_id
                                );
                                if let Some(url) = &tool_identifier.url {
                                    eprintln!("Please check that the URL '{}' is valid.", url);
                                }
                            }
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
            ConfigAction::Alias {
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
        },
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

async fn execute_run(
    config: &mut ToolerConfig,
    tool_id: String,
    tool_args: Vec<String>,
    asset: Option<String>,
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

    // Check for updates if not a pinned version
    if !tool_identifier.is_pinned() {
        check_for_updates(config).await?;
    }

    let mut tool_info = find_tool_executable(config, &tool_id);

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

    // Recovery: If tool not found in config, try to discover it locally
    if tool_info.is_none() && asset.is_none() {
        if let Ok(Some(recovered)) = install::try_recover_tool(&tool_id) {
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
        match install_or_update_tool(config, &tool_id, false, asset.as_deref()).await {
            Ok(_) => {
                *config = load_tool_configs()?; // Reload config
                tool_info = find_tool_executable(config, &tool_id);
            }
            Err(e) => {
                tracing::error!("Failed to install tool '{}': {}", tool_id, e);
                if e.to_string().contains("404") {
                    match tool_identifier.forge {
                        types::Forge::GitHub => {
                            eprintln!("\nError: Tool '{}' not found on GitHub.", tool_id);
                            eprintln!(
                                "Please check that the repository 'https://github.com/{}' exists.",
                                tool_identifier.full_repo()
                            );
                        }
                        types::Forge::Url => {
                            eprintln!(
                                "\nError: Tool '{}' (URL) not found or returned 404.",
                                tool_id
                            );
                            if let Some(url) = &tool_identifier.url {
                                eprintln!("Please check that the URL '{}' is valid.", url);
                            }
                        }
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
        if config.settings.auto_shim && !cfg!(windows) {
            let bin_dir = &config.settings.bin_dir;
            create_shim_script(bin_dir)?;
            create_tool_symlink(bin_dir, &tool_identifier.tool_name())?;
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
