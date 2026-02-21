use serde::Deserialize;
use std::env;
use std::time::Duration;
use thiserror::Error;

use crate::domain::{ActionId, ActionSpec, CommitSha, ResolutionError, Version, VersionRegistry};

const GITHUB_API_BASE: &str = "https://api.github.com";
const USER_AGENT: &str = "gx-cli";
const REQUEST_TIMEOUT_SECS: u64 = 30;

/// Errors that can occur when interacting with the Github API
#[derive(Debug, Error)]
pub enum GithubError {
    #[error(
        "GITHUB_TOKEN environment variable is required for this operation.\n\
         Set it with: export GITHUB_TOKEN=<your-token>\n\
         Create a token at: https://github.com/settings/tokens"
    )]
    TokenRequired,

    #[error("failed to create HTTP client")]
    ClientInit(#[source] reqwest::Error),

    #[error("failed to fetch {operation} from {url}")]
    Request {
        operation: &'static str,
        url: String,
        #[source]
        source: reqwest::Error,
    },

    #[error("Github API returned status {status} for {url}")]
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

/// Git ref structure returned by the Github API
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

pub struct GithubRegistry {
    client: reqwest::blocking::Client,
    token: Option<String>,
}

impl GithubRegistry {
    /// Create a new Github client, reading token from `GITHUB_TOKEN` environment variable
    ///
    /// # Errors
    ///
    /// Returns `GithubError::ClientInit` if the HTTP client cannot be initialized.
    pub fn from_env() -> Result<Self, GithubError> {
        Self::new(env::var("GITHUB_TOKEN").ok())
    }

    /// Create a new Github client with a custom token
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
    pub fn new(token: Option<String>) -> Result<Self, GithubError> {
        let client = reqwest::blocking::Client::builder()
            .user_agent(USER_AGENT)
            .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
            .build()
            .map_err(GithubError::ClientInit)?;

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
    /// Return `GithubError::TokenRequired` if the client does not have a token.
    pub fn resolve_ref(&self, owner_repo: &str, ref_name: &str) -> Result<String, GithubError> {
        // If it already looks like a full SHA (40 hex chars), return it
        if CommitSha::is_valid(ref_name) {
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

    fn fetch_ref(&self, url: &str) -> Result<String, GithubError> {
        let token = self.token.as_ref().ok_or(GithubError::TokenRequired)?;

        let response = self
            .client
            .get(url)
            .header("Authorization", format!("Bearer {token}"))
            .send()
            .map_err(|source| GithubError::Request {
                operation: "ref",
                url: url.to_string(),
                source,
            })?;

        if !response.status().is_success() {
            return Err(GithubError::ApiStatus {
                status: response.status(),
                url: url.to_string(),
            });
        }

        let git_ref: GitRef = response
            .json()
            .map_err(|source| GithubError::ParseResponse {
                url: url.to_string(),
                source,
            })?;

        Ok(git_ref.object.sha)
    }

    fn fetch_commit_sha(&self, url: &str) -> Result<String, GithubError> {
        let token = self.token.as_ref().ok_or(GithubError::TokenRequired)?;

        let response = self
            .client
            .get(url)
            .header("Authorization", format!("Bearer {token}"))
            .send()
            .map_err(|source| GithubError::Request {
                operation: "commit",
                url: url.to_string(),
                source,
            })?;

        if !response.status().is_success() {
            return Err(GithubError::ApiStatus {
                status: response.status(),
                url: url.to_string(),
            });
        }

        let commit: CommitResponse =
            response
                .json()
                .map_err(|source| GithubError::ParseResponse {
                    url: url.to_string(),
                    source,
                })?;

        Ok(commit.sha)
    }

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
    ) -> Result<Vec<String>, GithubError> {
        let token = self.token.as_ref().ok_or(GithubError::TokenRequired)?;

        // Handle subpath actions (e.g., "github/codeql-action/upload-sarif")
        let base_repo = owner_repo.split('/').take(2).collect::<Vec<_>>().join("/");

        let url = format!("{GITHUB_API_BASE}/repos/{base_repo}/git/refs/tags");

        let response = self
            .client
            .get(&url)
            .header("Authorization", format!("Bearer {token}"))
            .send()
            .map_err(|source| GithubError::Request {
                operation: "tags",
                url: url.clone(),
                source,
            })?;

        if !response.status().is_success() {
            return Err(GithubError::ApiStatus {
                status: response.status(),
                url,
            });
        }

        let refs: Vec<GitRefEntry> =
            response
                .json()
                .map_err(|source| GithubError::ParseResponse {
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

    /// Fetch all version-like tags using the matching-refs endpoint.
    /// Uses `GET /repos/{owner}/{repo}/git/matching-refs/tags/v` to narrow
    /// results to tags starting with "v" (semver convention).
    /// Handles pagination via Link header.
    ///
    /// # Errors
    ///
    /// Returns an error if no token is set, the request fails, or the response cannot be parsed.
    pub fn get_version_tags(&self, owner_repo: &str) -> Result<Vec<String>, GithubError> {
        let token = self.token.as_ref().ok_or(GithubError::TokenRequired)?;

        let base_repo = owner_repo.split('/').take(2).collect::<Vec<_>>().join("/");

        let mut all_refs: Vec<GitRefEntry> = Vec::new();
        let mut url =
            format!("{GITHUB_API_BASE}/repos/{base_repo}/git/matching-refs/tags/v?per_page=100");

        loop {
            let response = self
                .client
                .get(&url)
                .header("Authorization", format!("Bearer {token}"))
                .send()
                .map_err(|source| GithubError::Request {
                    operation: "version tags",
                    url: url.clone(),
                    source,
                })?;

            if !response.status().is_success() {
                return Err(GithubError::ApiStatus {
                    status: response.status(),
                    url,
                });
            }

            let next_url = parse_next_link(response.headers());

            let page: Vec<GitRefEntry> =
                response
                    .json()
                    .map_err(|source| GithubError::ParseResponse {
                        url: url.clone(),
                        source,
                    })?;

            all_refs.extend(page);

            match next_url {
                Some(next) => url = next,
                None => break,
            }
        }

        let tags: Vec<String> = all_refs
            .into_iter()
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

/// Parse the `Link` header to find the `rel="next"` URL for pagination.
fn parse_next_link(headers: &reqwest::header::HeaderMap) -> Option<String> {
    let link_header = headers.get("link")?.to_str().ok()?;
    for part in link_header.split(',') {
        let part = part.trim();
        if part.ends_with("rel=\"next\"") {
            // Extract URL between < and >
            let start = part.find('<')? + 1;
            let end = part.find('>')?;
            return Some(part[start..end].to_string());
        }
    }
    None
}

impl VersionRegistry for GithubRegistry {
    fn lookup_sha(&self, id: &ActionId, version: &Version) -> Result<CommitSha, ResolutionError> {
        self.resolve_ref(id.as_str(), version.as_str())
            .map(CommitSha::from)
            .map_err(|e| match e {
                GithubError::TokenRequired => ResolutionError::TokenRequired,
                _ => ResolutionError::ResolveFailed {
                    spec: ActionSpec::new(id.clone(), version.clone()),
                    reason: e.to_string(),
                },
            })
    }

    fn tags_for_sha(
        &self,
        id: &ActionId,
        sha: &CommitSha,
    ) -> Result<Vec<Version>, ResolutionError> {
        self.get_tags_for_sha(id.as_str(), sha.as_str())
            .map(|tags| tags.into_iter().map(Version::from).collect())
            .map_err(|e| match e {
                GithubError::TokenRequired => ResolutionError::TokenRequired,
                _ => ResolutionError::NoTagsForSha {
                    action: id.clone(),
                    sha: sha.clone(),
                },
            })
    }

    fn all_tags(&self, id: &ActionId) -> Result<Vec<Version>, ResolutionError> {
        self.get_version_tags(id.as_str())
            .map(|tags| tags.into_iter().map(Version::from).collect())
            .map_err(|e| match e {
                GithubError::TokenRequired => ResolutionError::TokenRequired,
                _ => ResolutionError::ResolveFailed {
                    spec: ActionSpec::new(id.clone(), Version::from("")),
                    reason: e.to_string(),
                },
            })
    }
}

impl Default for GithubRegistry {
    fn default() -> Self {
        Self::new(None).expect("Failed to create Github client")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_full_sha_passthrough() {
        let client = GithubRegistry::new(None).unwrap();
        let sha = "a1b2c3d4e5f6789012345678901234567890abcd";
        let result = client.resolve_ref("actions/checkout", sha).unwrap();
        assert_eq!(result, sha);
    }

    #[test]
    fn test_subpath_action_extracts_base_repo() {
        let client = GithubRegistry::new(None).unwrap();
        let sha = "a1b2c3d4e5f6789012345678901234567890abcd";
        // Should work with subpath actions
        let result = client
            .resolve_ref("github/codeql-action/upload-sarif", sha)
            .unwrap();
        assert_eq!(result, sha);
    }

    #[test]
    fn test_version_resolver_trait() {
        let client = GithubRegistry::new(None).unwrap();
        let id = ActionId::from("actions/checkout");
        let sha_version = Version::from("a1b2c3d4e5f6789012345678901234567890abcd");

        // Full SHA should pass through
        let result = client.lookup_sha(&id, &sha_version).unwrap();
        assert_eq!(result.as_str(), sha_version.as_str());
    }
}
