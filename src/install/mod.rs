//! Installation and tool management module
//!
//! This module provides functionality for:
//! - Installing and updating tools from GitHub or URLs
//! - Recovering tools from local filesystem (self-healing)
//! - Checking for and applying updates
//! - Managing tool configuration (pinning, removing)
//! - Listing installed tools

// Submodules
pub mod github;

// Re-export GitHub API functions
pub use github::{build_gh_release_url, discover_url_versions, get_gh_release_info};

// Import from parent install.rs (temporary - will be moved to submodules)
pub use crate::install::{
    check_for_updates, find_highest_version, find_tool_entry, find_tool_executable,
    install_or_update_tool, list_installed_tools, pin_tool, recover_all_installed_tools,
    remove_tool, try_recover_tool, version_matches,
};
