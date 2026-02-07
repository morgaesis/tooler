use clap::{Parser, Subcommand};

fn get_version() -> &'static str {
    const BASE_VERSION: &str = env!("CARGO_PKG_VERSION");

    // If there's a git tag at HEAD, use just the tag (release build)
    if let Some(tag) = option_env!("TOOLER_GIT_TAG") {
        return tag;
    }

    // Not on a tag - include commit hash and branch (dev build)
    let commit = option_env!("TOOLER_GIT_COMMIT").unwrap_or("unknown");
    let branch = option_env!("TOOLER_GIT_BRANCH").unwrap_or("unknown");

    // Return a static string by leaking the formatted string
    // This is safe because it only happens once at startup
    let version = format!("v{}-{} ({})", BASE_VERSION, commit, branch);
    Box::leak(version.into_boxed_str())
}

#[derive(Parser)]
#[command(name = "tooler")]
#[command(about = "A CLI tool manager for GitHub Releases")]
#[command(version = get_version(), propagate_version = true)]
pub struct Cli {
    /// Increase verbosity (use multiple times for more detail)
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    pub verbose: u8,

    /// Reduce output to errors only
    #[arg(short, long, global = true)]
    pub quiet: bool,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Run a tool
    #[command(
        allow_hyphen_values = true,
        disable_help_flag = true,
        disable_version_flag = true,
        after_help = "Examples:\n  tooler run sst/opencode --version\n  tooler run nektos/act@v0.2.79 --help\n  tooler -v run act\n\nTo see help for this command, use 'tooler help run'."
    )]
    Run {
        /// GitHub repository (e.g., 'owner/repo@vX.Y.Z')
        tool_id: String,
        /// Arguments to pass to tool
        #[arg(trailing_var_arg = true)]
        tool_args: Vec<String>,
        /// Explicitly specify asset name from the release to download
        #[arg(long)]
        asset: Option<String>,
    },

    /// List all installed tools
    List,

    /// Update one or all tools
    Update {
        /// Tool to update (e.g., 'owner/repo' or 'tool-name'), or 'all' to update all
        tool_id: Option<String>,
    },

    /// Pull latest version of a tool without updating existing installation
    Pull {
        /// Tool to pull (e.g., 'owner/repo' or 'tool-name')
        tool_id: String,
    },

    /// Remove an installed tool
    Remove {
        /// Tool to remove (e.g., 'owner/repo')
        tool_id: String,
    },

    /// Pin a tool to a specific version
    Pin {
        /// Tool to pin (e.g., 'owner/repo@version')
        tool_id: String,
    },

    /// Show detailed information about a tool
    Info {
        /// Tool to show info for (e.g., 'owner/repo' or 'tool-name')
        tool_id: String,
    },

    /// Manage tooler's configuration
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },

    /// Show the current version
    Version,

    /// Catch-all for running tools directly
    #[command(external_subcommand)]
    External(Vec<String>),
}

#[derive(Subcommand)]
pub enum ConfigAction {
    /// Get a configuration setting
    Get {
        /// Key to get (if omitted, shows all settings)
        key: Option<String>,
    },
    /// Set a configuration setting
    Set {
        /// Key and value (e.g., 'update-check-days=30' or 'update-check-days 30')
        #[arg(trailing_var_arg = true, required = true)]
        args: Vec<String>,
    },
    /// Unset a configuration setting (removes from config file)
    Unset {
        /// Key to unset (e.g., 'bin-dir')
        key: String,
    },
    /// Show full configuration
    Show {
        /// Output format (json, yaml, plain)
        #[arg(long, default_value = "json")]
        format: String,
    },
    /// Manage tool aliases
    Alias {
        /// Alias name (e.g., 'gh')
        name: String,
        /// Target repository or URL (e.g., 'cli/cli')
        target: Option<String>,
        /// Remove the alias
        #[arg(short, long)]
        remove: bool,
    },
}
