mod cli;
mod config;
mod download;
mod install;
mod platform;
mod tool_id;
mod types;

use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use clap::Parser;
use cli::{Cli, Commands, ConfigAction};
use config::{load_tool_configs, normalize_key, save_tool_configs};
use install::{find_tool_executable, install_or_update_tool, remove_tool};
use tool_id::ToolIdentifier;
use types::ToolerSettings;
use std::env;
use std::fs;
use std::path::Path;
use std::process::Command;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    
    // Setup logging
    setup_logging(&cli)?;
    
    // Load configuration
    let mut config = load_tool_configs()?;
    
    match cli.command {
        Commands::Version => {
            println!("tooler v{}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        
        Commands::List => {
            list_installed_tools(&config);
        }
        
        Commands::Remove { tool_id } => {
            let tool_identifier = ToolIdentifier::parse(&tool_id)
                .map_err(|e| anyhow!("Invalid tool identifier: {}", e))?;
            remove_tool(&mut config, &tool_identifier.config_key())?;
        }
        
        Commands::Update { tool_id } => {
            if let Some(tool_id) = tool_id {
                if tool_id == "all" {
                    tracing::info!("Updating all applicable tools...");
                    let mut updated_count = 0;
                    let keys_to_update: Vec<String> = config.tools
                        .keys()
                        .filter(|k| !k.contains(':')) // Only non-version-pinned tools
                        .cloned()
                        .collect();
                    
                    for key in keys_to_update {
                        if let Some(info) = config.tools.get(&key).cloned() {
                            match install_or_update_tool(&mut config, &info.tool_name, &info.repo, Some("latest"), true, None).await {
                                Ok(_) => updated_count += 1,
                                Err(e) => tracing::warn!("Failed to update {}: {}", info.repo, e),
                            }
                        }
                    }
                    tracing::info!("Update process finished. {} tool(s) were checked/updated", updated_count);
                } else {
                    let tool_identifier = ToolIdentifier::parse(&tool_id)
                        .map_err(|e| anyhow!("Invalid tool identifier: {}", e))?;
                    tracing::info!("Attempting to update {}...", tool_id);
                    match install_or_update_tool(&mut config, &tool_identifier.tool_name(), &tool_identifier.full_repo(), Some("latest"), true, None).await {
                        Ok(_) => tracing::info!("{} updated successfully", tool_id),
                        Err(e) => {
                            tracing::error!("Failed to update {}: {}", tool_id, e);
                            std::process::exit(1);
                        }
                    }
                }
            } else {
                tracing::error!("Please specify a tool to update or use 'all' to update all tools");
                std::process::exit(1);
            }
        }
        
        Commands::Config { action } => {
            match action {
                ConfigAction::Get { key } => {
                    if let Some(key) = key {
                        let value = match key.as_str() {
                            "update_check_days" => config.settings.update_check_days.to_string(),
                            "auto_shim" => config.settings.auto_shim.to_string(),
                            "shim_dir" => config.settings.shim_dir.clone(),
                            _ => format!("Setting '{}' not found", key),
                        };
                        println!("{}", value);
                    } else {
                        println!("--- Tooler Settings ---");
                        for (k, v) in &[
                            ("update_check_days", &config.settings.update_check_days.to_string()),
                            ("auto_shim", &config.settings.auto_shim.to_string()),
                            ("shim_dir", &config.settings.shim_dir),
                        ] {
                            println!("  {}: {}", k, v);
                        }
                    }
                }
                ConfigAction::Set { key_value } => {
                    if let Some((key, value_str)) = key_value.split_once('=') {
                        let key = normalize_key(key);
                        match key.as_str() {
                            "update_check_days" => {
                                if let Ok(days) = value_str.parse::<i32>() {
                                    config.settings.update_check_days = days;
                                    save_tool_configs(&config)?;
                                    tracing::info!("Setting '{}' updated to '{}'", key, days);
                                } else {
                                    tracing::error!("Invalid value for '{}'", key);
                                }
                            }
                            "auto_shim" => {
                                let value = value_str.to_lowercase() == "true" || value_str == "1";
                                config.settings.auto_shim = value;
                                save_tool_configs(&config)?;
                                tracing::info!("Setting '{}' updated to '{}'", key, value);
                            }
                            "shim_dir" => {
                                config.settings.shim_dir = value_str.to_string();
                                save_tool_configs(&config)?;
                                tracing::info!("Setting '{}' updated to '{}'", key, value_str);
                            }
                            _ => {
                                tracing::error!("'{}' is not a valid configuration setting. Valid settings: update_check_days, auto_shim, shim_dir", key);
                            }
                        }
                    } else {
                        tracing::error!("Invalid format. Use 'key=value'.");
                    }
                }
                ConfigAction::Unset { key } => {
                    let key = normalize_key(&key);
                    match key.as_str() {
                        "update_check_days" => {
                            config.settings.update_check_days = ToolerSettings::default().update_check_days;
                            save_tool_configs(&config)?;
                            tracing::info!("Setting '{}' unset", key);
                        }
                        "auto_shim" => {
                            config.settings.auto_shim = ToolerSettings::default().auto_shim;
                            save_tool_configs(&config)?;
                            tracing::info!("Setting '{}' unset", key);
                        }
                        "shim_dir" => {
                            config.settings.shim_dir = ToolerSettings::default().shim_dir;
                            save_tool_configs(&config)?;
                            tracing::info!("Setting '{}' unset", key);
                        }
                        _ => {
                            tracing::error!("'{}' is not a valid configuration setting. Valid settings: update_check_days, auto_shim, shim_dir", key);
                        }
                    }
                }
            }
        }
        
        Commands::Run { tool_id, tool_args, asset } => {
            let tool_identifier = ToolIdentifier::parse(&tool_id)
                .map_err(|e| anyhow!("Invalid tool identifier: {}", e))?;
            let version_req = tool_identifier.api_version();
            
            // Check for updates if not a pinned version
            if !tool_identifier.is_pinned() {
                check_for_updates(&mut config).await?;
            }
            
            let mut tool_info = find_tool_executable(&config, &tool_id);
            
            // Install if not found or if asset override is used
            if tool_info.is_none() || asset.is_some() {
                if tool_info.is_none() {
                    tracing::info!("Tool {} not found locally or is corrupted. Attempting to install...", tool_id);
                }
                
                match install_or_update_tool(&mut config, &tool_identifier.tool_name(), &tool_identifier.full_repo(), Some(&version_req), false, asset.as_deref()).await {
                    Ok(_) => {
                        config = load_tool_configs()?; // Reload config
                        tool_info = find_tool_executable(&config, &tool_id);
                    }
                    Err(e) => {
                        tracing::error!("Failed to install tool: {}", e);
                        std::process::exit(1);
                    }
                }
            }
            
            if let Some(info) = tool_info {
                // Create shim if auto_shim is enabled
                if config.settings.auto_shim && !cfg!(windows) {
                    create_shim_script(&config.settings.shim_dir)?;
                    create_tool_symlink(&config.settings.shim_dir, &tool_identifier.tool_name())?;
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
    
    let filter = EnvFilter::try_from_default_env()
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
    println!("--- Installed Tooler Tools ---");
    if config.tools.is_empty() {
        println!("  No tools installed yet.");
        return;
    }
    
    let mut tools: Vec<_> = config.tools.values().collect();
    tools.sort_by_key(|t| &t.repo);
    
    for info in tools {
        println!("  - {} (v{}) [type: {}]", info.repo, info.version, info.install_type);
        println!("    Path:    {}\n", info.executable_path);
    }
    println!("------------------------------");
}

async fn check_for_updates(config: &mut types::ToolerConfig) -> Result<()> {
    if config.settings.update_check_days <= 0 {
        return Ok(());
    }
    
    tracing::info!("Checking for tools not updated in >{} days...", config.settings.update_check_days);
    let now = Utc::now();
    let mut updates_found = Vec::new();
    
    let keys_to_check: Vec<String> = config.tools
        .keys()
        .filter(|k| !k.contains(':')) // Only non-version-pinned tools
        .cloned()
        .collect();
    
    for key in keys_to_check {
        if let Some(info) = config.tools.get(&key).cloned() {
            let last_accessed: DateTime<Utc> = info.last_accessed.parse()?;
            let days_since_update = (now - last_accessed).num_days();
            
            if days_since_update > config.settings.update_check_days as i64 {
                tracing::info!("Checking for update for {} (current: {}, last updated: {} days ago)", 
                    info.repo, info.version, days_since_update);
                
                if let Ok(release) = install::get_gh_release_info(&info.repo, Some("latest")).await {
                    if release.tag_name != info.version {
                        updates_found.push(format!("Tool {} ({}) has update: {} -> {} (last updated {} days ago)", 
                            info.tool_name, info.repo, info.version, release.tag_name, days_since_update));
                    }
                    
                    // Update last_accessed
                    if let Some(tool_info) = config.tools.get_mut(&key) {
                        tool_info.last_accessed = now.to_rfc3339();
                    }
                } else {
                    tracing::warn!("Could not get latest release for {} during update check", info.repo);
                }
            }
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
    } else {
        tracing::info!("No updates found or checks are not due.");
    }
    
    Ok(())
}

fn create_shim_script(shim_dir: &str) -> Result<()> {
    let shim_path = Path::new(shim_dir).join("tooler-shim");
    if !shim_path.exists() {
        fs::create_dir_all(shim_dir)?;
        let shim_content = "#!/bin/bash\ntool_name=$(basename \"$0\")\nexec tooler run \"$tool_name\" \"$@\"\n";
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

fn create_tool_symlink(shim_dir: &str, tool_name: &str) -> Result<()> {
    let shim_path = Path::new(shim_dir).join("tooler-shim");
    let symlink_path = Path::new(shim_dir).join(tool_name);
    
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
        tracing::info!("Created symlink {} -> {}", symlink_path.display(), shim_path.display());
    }
    Ok(())
}