//! GitHub API interaction module
//!
//! Provides functions for querying GitHub releases and constructing API URLs.

use crate::types::GitHubRelease;
use anyhow::Result;
use reqwest::StatusCode;
use std::error::Error;
use std::fmt;

#[derive(Debug)]
pub enum GitHubReleaseError {
    TagNotFound { repo: String, version: String },
    LatestNotFound { repo: String },
    RequestFailed { repo: String, status: StatusCode },
}

impl fmt::Display for GitHubReleaseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GitHubReleaseError::TagNotFound { repo, version } => {
                write!(f, "Release tag '{}' not found in {}", version, repo)
            }
            GitHubReleaseError::LatestNotFound { repo } => {
                write!(f, "No releases found for {}", repo)
            }
            GitHubReleaseError::RequestFailed { repo, status } => {
                write!(f, "Failed to get release info for {}: {}", repo, status)
            }
        }
    }
}

impl Error for GitHubReleaseError {}

/// Build GitHub API URL for fetching release information
///
/// # Arguments
/// * `repo` - Repository in format "owner/repo"
/// * `version` - Optional version ("latest", "default", or specific like "v1.2.3")
pub fn build_gh_release_url(repo: &str, version: Option<&str>) -> String {
    if let Some(v) = version {
        if v == "latest" || v == "default" {
            format!("https://api.github.com/repos/{}/releases/latest", repo)
        } else {
            format!("https://api.github.com/repos/{}/releases/tags/{}", repo, v)
        }
    } else {
        format!("https://api.github.com/repos/{}/releases/latest", repo)
    }
}

/// Fetch release information from GitHub API
///
/// # Arguments
/// * `repo` - Repository in format "owner/repo"
/// * `version` - Optional version (None means latest)
pub async fn get_gh_release_info(repo: &str, version: Option<&str>) -> Result<GitHubRelease> {
    let url = build_gh_release_url(repo, version);

    let client = reqwest::Client::new();
    let response = client
        .get(&url)
        .header("User-Agent", "tooler")
        .send()
        .await?;

    if !response.status().is_success() {
        if response.status() == StatusCode::NOT_FOUND {
            if let Some(v) = version {
                return Err(GitHubReleaseError::TagNotFound {
                    repo: repo.to_string(),
                    version: v.to_string(),
                }
                .into());
            }
            return Err(GitHubReleaseError::LatestNotFound {
                repo: repo.to_string(),
            }
            .into());
        }
        return Err(GitHubReleaseError::RequestFailed {
            repo: repo.to_string(),
            status: response.status(),
        }
        .into());
    }

    let release: GitHubRelease = response.json().await?;
    Ok(release)
}

/// Stub for discovering versions from URL-based tools
///
/// TODO: Implement directory scraping for URL-based version discovery
pub async fn discover_url_versions(_url: &str) -> Result<Vec<String>> {
    Ok(vec![])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_gh_release_url() {
        // Test with specific version
        assert_eq!(
            build_gh_release_url("owner/repo", Some("v1.2.3")),
            "https://api.github.com/repos/owner/repo/releases/tags/v1.2.3"
        );

        // Test with "latest" version
        assert_eq!(
            build_gh_release_url("owner/repo", Some("latest")),
            "https://api.github.com/repos/owner/repo/releases/latest"
        );

        // Test with "default" version (should map to latest)
        assert_eq!(
            build_gh_release_url("owner/repo", Some("default")),
            "https://api.github.com/repos/owner/repo/releases/latest"
        );

        // Test with None version
        assert_eq!(
            build_gh_release_url("owner/repo", None),
            "https://api.github.com/repos/owner/repo/releases/latest"
        );

        // Test with complex version tag (with slashes)
        assert_eq!(
            build_gh_release_url("owner/repo", Some("prefix/v1.0.0")),
            "https://api.github.com/repos/owner/repo/releases/tags/prefix/v1.0.0"
        );
    }
}
