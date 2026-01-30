use serde::Deserialize;
use std::env;
use std::time::Duration;
use thiserror::Error;

use crate::error::GitHubTokenRequired;
use crate::git::is_commit_sha;

const GITHUB_API_BASE: &str = "https://api.github.com";
const USER_AGENT: &str = "gx-cli";
const REQUEST_TIMEOUT_SECS: u64 = 30;

/// Errors that can occur when interacting with the GitHub API
#[derive(Debug, Error)]
pub enum GitHubError {
    #[error(transparent)]
    TokenRequired(#[from] GitHubTokenRequired),

    #[error("failed to create HTTP client")]
    ClientInit(#[source] reqwest::Error),

    #[error("failed to fetch {operation} from {url}")]
    Request {
        operation: &'static str,
        url: String,
        #[source]
        source: reqwest::Error,
    },

    #[error("GitHub API returned status {status} for {url}")]
    ApiStatus {
        status: reqwest::StatusCode,
        url: String,
    },

    #[error("failed to parse response from {url}")]
    ParseResponse {
        url: String,
        #[source]
        source: reqwest::Error,
    },
}

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
    /// Returns `GitHubError::ClientInit` if the HTTP client cannot be initialized.
    pub fn from_env() -> Result<Self, GitHubError> {
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
    pub fn new(token: Option<String>) -> Result<Self, GitHubError> {
        let client = reqwest::blocking::Client::builder()
            .user_agent(USER_AGENT)
            .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
            .build()
            .map_err(GitHubError::ClientInit)?;

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
    /// Return `GitHubError::TokenRequired` if the client does not have a token.
    pub fn resolve_ref(&self, owner_repo: &str, ref_name: &str) -> Result<String, GitHubError> {
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

    fn fetch_ref(&self, url: &str) -> Result<String, GitHubError> {
        let token = self.token.as_ref().ok_or(GitHubTokenRequired)?;

        let response = self
            .client
            .get(url)
            .header("Authorization", format!("Bearer {token}"))
            .send()
            .map_err(|source| GitHubError::Request {
                operation: "ref",
                url: url.to_string(),
                source,
            })?;

        if !response.status().is_success() {
            return Err(GitHubError::ApiStatus {
                status: response.status(),
                url: url.to_string(),
            });
        }

        let git_ref: GitRef = response
            .json()
            .map_err(|source| GitHubError::ParseResponse {
                url: url.to_string(),
                source,
            })?;

        Ok(git_ref.object.sha)
    }

    fn fetch_commit_sha(&self, url: &str) -> Result<String, GitHubError> {
        let token = self.token.as_ref().ok_or(GitHubTokenRequired)?;

        let response = self
            .client
            .get(url)
            .header("Authorization", format!("Bearer {token}"))
            .send()
            .map_err(|source| GitHubError::Request {
                operation: "commit",
                url: url.to_string(),
                source,
            })?;

        if !response.status().is_success() {
            return Err(GitHubError::ApiStatus {
                status: response.status(),
                url: url.to_string(),
            });
        }

        let commit: CommitResponse =
            response
                .json()
                .map_err(|source| GitHubError::ParseResponse {
                    url: url.to_string(),
                    source,
                })?;

        Ok(commit.sha)
    }
}

impl GitHubClient {
    /// Get all tags that point to a specific commit SHA.
    ///
    /// Returns tag names without the "refs/tags/" prefix (e.g., `["v5", "v5.0.0"]`)
    /// Note: This only works for lightweight tags. Annotated tags store the tag object SHA,
    /// not the commit SHA, so they won't match.
    ///
    /// # Errors
    ///
    /// Returns an error if no token is set, the request fails, or the response cannot be parsed.
    pub fn get_tags_for_sha(
        &self,
        owner_repo: &str,
        sha: &str,
    ) -> Result<Vec<String>, GitHubError> {
        let token = self.token.as_ref().ok_or(GitHubTokenRequired)?;

        // Handle subpath actions (e.g., "github/codeql-action/upload-sarif")
        let base_repo = owner_repo.split('/').take(2).collect::<Vec<_>>().join("/");

        let url = format!("{GITHUB_API_BASE}/repos/{base_repo}/git/refs/tags");

        let response = self
            .client
            .get(&url)
            .header("Authorization", format!("Bearer {token}"))
            .send()
            .map_err(|source| GitHubError::Request {
                operation: "tags",
                url: url.clone(),
                source,
            })?;

        if !response.status().is_success() {
            return Err(GitHubError::ApiStatus {
                status: response.status(),
                url,
            });
        }

        let refs: Vec<GitRefEntry> =
            response
                .json()
                .map_err(|source| GitHubError::ParseResponse {
                    url: url.clone(),
                    source,
                })?;

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
}
