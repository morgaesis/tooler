use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolIdentifier {
    pub forge: String,
    pub author: String,
    pub repo: String,
    pub version: Option<String>,
}

impl ToolIdentifier {
    /// Parse a tool identifier from various formats:
    /// - "owner/repo" (default version)
    /// - "owner/repo@v1.2.3" (specific version)
    /// - "repo" (short name, looks up in config)
    /// - "repo@v1.2.3" (short name with version)
    pub fn parse(tool_id: &str) -> Result<Self, String> {
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
            forge: "github".to_string(),
            author,
            repo,
            version: version_part,
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
                if v.starts_with('v') {
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
