use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "tooler")]
#[command(about = "A CLI tool manager for GitHub Releases")]
#[command(version)]
#[command(after_help = "Examples:
  tooler run nektos/act@v0.2.79 -- --help                   # Run specific version with args
  tooler run infisical/infisical@infisical-cli/v0.41.90      # Run with complex GitHub tag
  tooler run adrienverge/yamllint                           # Run Python tool from .whl asset
  tooler run argoproj/argo-cd --asset argocd-darwin-amd64   # Run with an explicit asset
  tooler run yamllint .                                     # Run a tool previously fetched
  tooler -v run act                                         # Run verbosely

  tooler list                                               # List all installed tools
  tooler update nektos/act                                  # Update to latest version
  tooler update yamllint                                    # Update short-name to latest version
  tooler update all                                         # Update all non-pinned tools
  tooler pull infisical/infisical@infisical-cli/v0.41.90    # Pull complex tag without updating
  tooler remove nektos/act                                  # Remove all versions of a tool
  tooler pin nektos/act@v0.2.79                           # Pin tool to specific version

  tooler config get                                         # Show all settings
  tooler config set auto_shim=true                          # Enable auto-shimming
  tooler config set shim_dir=/home/user/.local/bin          # Set shim directory
  tooler config unset shim_dir                              # Unset shim_dir (reverts to default)")]
pub struct Cli {
    #[arg(short, long, action = clap::ArgAction::Count)]
    pub verbose: u8,

    #[arg(short, long)]
    pub quiet: bool,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Run a tool
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

    /// Manage tooler's configuration
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },

    /// Show the current version
    Version,
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
        /// Key=Value pair (e.g., 'update_check_days=30')
        key_value: String,
    },
    /// Unset a configuration setting (removes from config file)
    Unset {
        /// Key to unset (e.g., 'shim-dir')
        key: String,
    },
}
