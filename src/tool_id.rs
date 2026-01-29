use crate::types::Forge;
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolIdentifier {
    pub forge: Forge,
    pub author: String,
    pub repo: String,
    pub version: Option<String>,
    pub url: Option<String>,
}

impl ToolIdentifier {
    /// Parse a tool identifier from various formats:
    /// - "owner/repo" (default version)
    /// - "owner/repo@v1.2.3" (specific version)
    /// - "repo" (short name, looks up in config)
    /// - "repo@v1.2.3" (short name with version)
    /// - "https://..." (direct URL)
    pub fn parse(tool_id: &str) -> Result<Self, String> {
        if tool_id.is_empty() {
            return Err("Tool identifier cannot be empty".to_string());
        }

        if tool_id.starts_with('-') {
            return Err(format!(
                "Invalid tool identifier '{}'. It looks like a CLI flag.",
                tool_id
            ));
        }

        // Handle URL forge
        if tool_id.starts_with("http://") || tool_id.starts_with("https://") {
            let (url_part, version_part) = if tool_id.contains('@') && !tool_id.contains("@v") {
                // If it contains @ but it's not part of a version string (like in a path),
                // we might need careful parsing. But usually @ is for versioning in tooler.
                // However, for URLs, @ is rare unless it's for credentials (which we don't support here).
                // Let's assume @ at the end is for version pinning.
                let mut parts = tool_id.rsplitn(2, '@');
                let version = parts.next().map(|s| s.to_string());
                let url = parts.next().unwrap_or(tool_id).to_string();
                (url, version)
            } else {
                (tool_id.to_string(), None)
            };

            let name = url_part
                .split('/')
                .next_back()
                .unwrap_or("unknown_tool")
                .split('?')
                .next()
                .unwrap_or("unknown_tool")
                .trim_end_matches(".zip")
                .trim_end_matches(".tar.gz")
                .trim_end_matches(".tgz")
                .trim_end_matches(".tar.xz")
                .to_string();

            // Attempt to guess version from URL if not provided
            let version = version_part.or_else(|| {
                let re = regex::Regex::new(r"v?(\d+\.\d+\.\d+)").ok()?;
                re.find(&url_part).map(|m| m.as_str().to_string())
            });

            return Ok(ToolIdentifier {
                forge: Forge::Url,
                author: "direct".to_string(),
                repo: name,
                version,
                url: Some(url_part),
            });
        }

        // Handle @ for version
        let (repo_part, version_part) = if tool_id.contains('@') {
            let mut parts = tool_id.splitn(2, '@');
            let repo = parts
                .next()
                .ok_or_else(|| "Missing repository part".to_string())?;
            let version = parts.next().map(|s| s.to_string());
            (repo, version)
        } else {
            (tool_id, Some("default".to_string()))
        };

        // Parse repository part
        let repo_parts: Vec<&str> = repo_part.split('/').collect();
        let (author, repo) = match repo_parts.len() {
            1 => {
                // Short form like "act" - no author specified
                ("unknown".to_string(), repo_parts[0].to_string())
            }
            2 => {
                // Full form like "nektos/act"
                (repo_parts[0].to_string(), repo_parts[1].to_string())
            }
            _ => return Err(format!("Invalid repository format: {}", repo_part)),
        };

        Ok(ToolIdentifier {
            forge: Forge::GitHub,
            author,
            repo,
            version: version_part,
            url: None,
        })
    }

    /// Get: full repository string (author/repo)
    pub fn full_repo(&self) -> String {
        if self.author == "unknown" {
            self.repo.clone()
        } else {
            format!("{}/{}", self.author, self.repo)
        }
    }

    /// Get: tool name (repo name without author)
    pub fn tool_name(&self) -> String {
        self.repo.clone()
    }

    /// Get: version string for API calls (adds 'v' prefix if needed)
    pub fn api_version(&self) -> String {
        match self.version.as_deref().unwrap_or("default") {
            "default" => "latest".to_string(),
            v => {
                // Don't modify versions that contain special characters like slashes
                // or are non-numeric (e.g., "infisical-cli/v0.41.90", "master", "tip")
                // Also preserve existing 'v' prefixes
                if v.contains('/')
                    || v.chars().next().is_some_and(|c| !c.is_ascii_digit())
                    || v.starts_with('v')
                {
                    v.to_string()
                } else {
                    format!("v{}", v)
                }
            }
        }
    }

    /// Get: configuration key for storing this tool
    pub fn config_key(&self) -> String {
        let version = self.version.as_deref().unwrap_or("default");
        if version == "default" {
            format!("{}@latest", self.full_repo())
        } else {
            format!("{}@{}", self.full_repo(), version)
        }
    }

    /// Get: configuration key for a default (latest) tool
    pub fn default_config_key(&self) -> String {
        format!("{}@latest", self.full_repo())
    }

    /// Check if this is a version-pinned tool
    pub fn is_pinned(&self) -> bool {
        self.version.is_some() && self.version.as_deref().unwrap_or("default") != "default"
    }
}

impl fmt::Display for ToolIdentifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(version) = &self.version {
            write!(f, "{}@{}", self.full_repo(), version)
        } else {
            write!(f, "{}", self.full_repo())
        }
    }
}
