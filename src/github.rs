use anyhow::{Context, Result, anyhow};
use serde::Deserialize;
use std::env;
use std::time::Duration;

use crate::error::GitHubTokenRequired;
use crate::git::is_commit_sha;

const GITHUB_API_BASE: &str = "https://api.github.com";
const USER_AGENT: &str = "gx-cli";
const REQUEST_TIMEOUT_SECS: u64 = 30;

/// Git ref structure returned by the GitHub API
#[derive(Debug, Deserialize)]
pub struct GitRef {
    pub object: GitObject,
}

/// Git object containing a SHA
#[derive(Debug, Deserialize)]
pub struct GitObject {
    pub sha: String,
}

/// Structure for git ref entries returned by the refs API
#[derive(Debug, Deserialize)]
pub struct GitRefEntry {
    #[serde(rename = "ref")]
    pub ref_name: String,
    pub object: GitObject,
}

#[derive(Deserialize)]
struct CommitResponse {
    sha: String,
}

pub struct GitHubClient {
    client: reqwest::blocking::Client,
    token: Option<String>,
}

impl GitHubClient {
    /// Create a new GitHub client, reading token from `GITHUB_TOKEN` environment variable
    ///
    /// # Errors
    ///
    /// Returns `VarError::NotPresent` if:
    /// - The variable is not set.
    /// - The variable’s name contains an equal sign or NUL ('=' or '\0').
    ///
    /// Returns `VarError::NotUnicode` if the variable’s value is not valid Unicode.
    pub fn from_env() -> Result<Self> {
        Self::new(env::var("GITHUB_TOKEN").ok())
    }

    /// Create a new GitHub client with a custom token
    ///
    /// # Errors
    ///
    /// This method fails if TLS backend cannot be initialized, or the resolver
    /// cannot load the system configuration.
    ///
    /// # Panics
    ///
    /// This method panics if called from within an async runtime. See docs on
    /// [`reqwest::blocking`][crate::blocking] for details.
    pub fn new(token: Option<String>) -> Result<Self> {
        let client = reqwest::blocking::Client::builder()
            .user_agent(USER_AGENT)
            .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
            .build()
            .context("Failed to create HTTP client")?;

        Ok(Self { client, token })
    }

    /// Resolve a ref (tag, branch, or commit) to a full commit SHA
    ///
    /// # Examples
    ///
    /// - `resolve_ref("actions/checkout", "v4") -> "abc123..."`
    /// - `resolve_ref("actions/checkout", "main") -> "def456..." -> "def456..."`
    /// - `resolve_ref("actions/checkout", "abc123") -> "abc123..." -> "abc123..."` (if valid)
    /// - `resolve_ref("github/codeql-action/upload-sarif", "v4") -> "abc123..." -> "abc123..."` (subpath action)
    ///
    /// # Errors
    ///
    /// Return `GitHubTokenRequired` if the client does not have a token.
    pub fn resolve_ref(&self, owner_repo: &str, ref_name: &str) -> Result<String> {
        // If it already looks like a full SHA (40 hex chars), return it
        if is_commit_sha(ref_name) {
            return Ok(ref_name.to_string());
        }

        // Handle subpath actions (e.g., "github/codeql-action/upload-sarif")
        // Extract just the owner/repo part (first two path segments)
        let base_repo = owner_repo.split('/').take(2).collect::<Vec<_>>().join("/");

        // Try to resolve as a tag or branch
        let url = format!("{GITHUB_API_BASE}/repos/{base_repo}/git/ref/tags/{ref_name}");

        self.fetch_ref(&url)
            .or_else(|_| {
                let url = format!("{GITHUB_API_BASE}/repos/{base_repo}/git/ref/heads/{ref_name}");
                self.fetch_ref(&url)
            })
            .or_else(|_| {
                let url = format!("{GITHUB_API_BASE}/repos/{base_repo}/commits/{ref_name}");
                self.fetch_commit_sha(&url)
            })
    }

    fn fetch_ref(&self, url: &str) -> Result<String> {
        let token = self
            .token
            .as_ref()
            .ok_or_else(|| anyhow!(GitHubTokenRequired))?;

        let response = self
            .client
            .get(url)
            .header("Authorization", format!("Bearer {token}"))
            .send()
            .with_context(|| format!("Failed to fetch ref from {url}"))?;

        if !response.status().is_success() {
            anyhow::bail!("GitHub API returned status {}", response.status());
        }

        let git_ref: GitRef = response
            .json()
            .context("Failed to parse GitHub API response")?;

        Ok(git_ref.object.sha)
    }

    fn fetch_commit_sha(&self, url: &str) -> Result<String> {
        let token = self
            .token
            .as_ref()
            .ok_or_else(|| anyhow!(GitHubTokenRequired))?;

        let response = self
            .client
            .get(url)
            .header("Authorization", format!("Bearer {token}"))
            .send()
            .with_context(|| format!("Failed to fetch commit from {url}"))?;

        if !response.status().is_success() {
            anyhow::bail!("GitHub API returned status {}", response.status());
        }

        let commit: CommitResponse = response
            .json()
            .context("Failed to parse GitHub API commit response")?;

        Ok(commit.sha)
    }
}

impl GitHubClient {
    /// Get all tags that point to a specific commit SHA
    ///
    /// Returns tag names without the "refs/tags/" prefix (e.g., ["v5", "v5.0.0"])
    /// Note: This only works for lightweight tags. Annotated tags store the tag object SHA,
    /// not the commit SHA, so they won't match.
    pub fn get_tags_for_sha(&self, owner_repo: &str, sha: &str) -> Result<Vec<String>> {
        let token = self
            .token
            .as_ref()
            .ok_or_else(|| anyhow!(GitHubTokenRequired))?;

        // Handle subpath actions (e.g., "github/codeql-action/upload-sarif")
        let base_repo = owner_repo.split('/').take(2).collect::<Vec<_>>().join("/");

        let url = format!("{GITHUB_API_BASE}/repos/{base_repo}/git/refs/tags");

        let response = self
            .client
            .get(&url)
            .header("Authorization", format!("Bearer {token}"))
            .send()
            .with_context(|| format!("Failed to fetch tags from {url}"))?;

        if !response.status().is_success() {
            anyhow::bail!("GitHub API returned status {}", response.status());
        }

        let refs: Vec<GitRefEntry> = response
            .json()
            .context("Failed to parse GitHub API tags response")?;

        // Filter tags that point to the given SHA and extract tag names
        let tags: Vec<String> = refs
            .into_iter()
            .filter(|r| r.object.sha == sha)
            .map(|r| {
                r.ref_name
                    .strip_prefix("refs/tags/")
                    .unwrap_or(&r.ref_name)
                    .to_string()
            })
            .collect();

        Ok(tags)
    }
}

impl Default for GitHubClient {
    fn default() -> Self {
        Self::new(None).expect("Failed to create GitHub client")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_full_sha_passthrough() {
        let client = GitHubClient::new(None).unwrap();
        let sha = "a1b2c3d4e5f6789012345678901234567890abcd";
        let result = client.resolve_ref("actions/checkout", sha).unwrap();
        assert_eq!(result, sha);
    }

    #[test]
    fn test_subpath_action_extracts_base_repo() {
        let client = GitHubClient::new(None).unwrap();
        let sha = "a1b2c3d4e5f6789012345678901234567890abcd";
        // Should work with subpath actions
        let result = client
            .resolve_ref("github/codeql-action/upload-sarif", sha)
            .unwrap();
        assert_eq!(result, sha);
    }

    // Note: The following tests require network access and hit the real GitHub API
    // They are marked with #[ignore] to skip during normal test runs

    #[test]
    fn test_resolve_tag() {
        let token = std::env::var("GITHUB_TOKEN").ok();
        let client = GitHubClient::new(token).unwrap();
        let sha = client.resolve_ref("actions/checkout", "v4").unwrap();
        // Should return a 40-character hex SHA
        assert_eq!(sha.len(), 40);
        assert!(sha.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_resolve_branch() {
        let token = std::env::var("GITHUB_TOKEN").ok();
        let client = GitHubClient::new(token).unwrap();
        let sha = client.resolve_ref("actions/checkout", "main").unwrap();
        assert_eq!(sha.len(), 40);
        assert!(sha.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_get_tags_for_sha() {
        let token = std::env::var("GITHUB_TOKEN").ok();
        let client = GitHubClient::new(token).unwrap();
        // First resolve v4 to get the SHA
        let sha = client.resolve_ref("actions/checkout", "v4").unwrap();
        // Then get tags for that SHA
        let tags = client.get_tags_for_sha("actions/checkout", &sha).unwrap();
        // v4 should be in the list
        assert!(
            tags.contains(&"v4".to_string()),
            "Expected v4 in tags: {tags:?}"
        );
    }
}
