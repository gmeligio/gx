use serde::Deserialize;
use std::time::Duration;
use thiserror::Error;

use crate::domain::{
    ActionId, ActionSpec, CommitSha, RefType, ResolutionError, ResolvedRef, Version,
    VersionRegistry,
};

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

/// Response for a release API call
#[derive(Debug, Deserialize)]
struct ReleaseResponse {
    #[serde(rename = "published_at")]
    published_at: Option<String>,
}

/// Response for a commit details API call
#[derive(Debug, Deserialize)]
struct CommitDetailResponse {
    commit: CommitObject,
}

/// Commit object containing committer info
#[derive(Debug, Deserialize)]
struct CommitObject {
    committer: Option<CommitterInfo>,
}

/// Committer info from commit details
#[derive(Debug, Deserialize)]
struct CommitterInfo {
    date: Option<String>,
}

/// Response for a tag object API call
#[derive(Debug, Deserialize)]
struct TagObjectResponse {
    tagger: Option<TaggerInfo>,
}

/// Tagger info from tag object
#[derive(Debug, Deserialize)]
struct TaggerInfo {
    date: Option<String>,
}

#[derive(Clone)]
pub struct GithubRegistry {
    client: reqwest::blocking::Client,
    token: Option<String>,
}

impl GithubRegistry {
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

    /// Resolve a ref (tag, branch, or commit) to a full commit SHA and detect the ref type
    ///
    /// Returns a tuple of (`sha`, `ref_type`) by tracking which API path succeeded.
    ///
    /// # Examples
    ///
    /// - `resolve_ref("actions/checkout", "v4") -> ("abc123...", RefType::Tag)`
    /// - `resolve_ref("actions/checkout", "main") -> ("def456...", RefType::Branch)`
    /// - `resolve_ref("actions/checkout", "abc123") -> ("abc123...", RefType::Commit)`
    /// - `resolve_ref("github/codeql-action/upload-sarif", "v4") -> ("abc123...", RefType::Tag)`
    ///
    /// # Errors
    ///
    /// Return `GithubError::TokenRequired` if the client does not have a token.
    pub fn resolve_ref(
        &self,
        owner_repo: &str,
        ref_name: &str,
    ) -> Result<(String, RefType), GithubError> {
        // If it already looks like a full SHA (40 hex chars), return it as a Commit
        if CommitSha::is_valid(ref_name) {
            return Ok((ref_name.to_string(), RefType::Commit));
        }

        // Handle subpath actions (e.g., "github/codeql-action/upload-sarif")
        // Extract just the owner/repo part (first two path segments)
        let base_repo = owner_repo.split('/').take(2).collect::<Vec<_>>().join("/");

        // Try to resolve as a tag first
        let url = format!("{GITHUB_API_BASE}/repos/{base_repo}/git/ref/tags/{ref_name}");
        if let Ok(sha) = self.fetch_ref(&url) {
            // Check if this tag has a GitHub Release
            if self
                .fetch_release_date(&base_repo, ref_name)
                .ok()
                .flatten()
                .is_some()
            {
                return Ok((sha, RefType::Release));
            }
            return Ok((sha, RefType::Tag));
        }

        // Try to resolve as a branch
        let url = format!("{GITHUB_API_BASE}/repos/{base_repo}/git/ref/heads/{ref_name}");
        if let Ok(sha) = self.fetch_ref(&url) {
            return Ok((sha, RefType::Branch));
        }

        // Try to resolve as a direct commit
        let url = format!("{GITHUB_API_BASE}/repos/{base_repo}/commits/{ref_name}");
        self.fetch_commit_sha(&url)
            .map(|sha| (sha, RefType::Commit))
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

    /// Fetch the commit date from a commit SHA
    ///
    /// # Errors
    ///
    /// Returns an error if no token is set, the request fails, or the response cannot be parsed.
    fn fetch_commit_date(&self, base_repo: &str, sha: &str) -> Result<Option<String>, GithubError> {
        let token = self.token.as_ref().ok_or(GithubError::TokenRequired)?;
        let url = format!("{GITHUB_API_BASE}/repos/{base_repo}/commits/{sha}");

        let response = self
            .client
            .get(&url)
            .header("Authorization", format!("Bearer {token}"))
            .send()
            .map_err(|source| GithubError::Request {
                operation: "commit details",
                url: url.clone(),
                source,
            })?;

        if !response.status().is_success() {
            return Err(GithubError::ApiStatus {
                status: response.status(),
                url,
            });
        }

        let commit: CommitDetailResponse = response
            .json()
            .map_err(|source| GithubError::ParseResponse { url, source })?;

        Ok(commit.commit.committer.and_then(|c| c.date))
    }

    /// Fetch the release date from a release tag
    ///
    /// # Errors
    ///
    /// Returns an error if no token is set, the request fails, or the response cannot be parsed.
    fn fetch_release_date(
        &self,
        base_repo: &str,
        tag: &str,
    ) -> Result<Option<String>, GithubError> {
        let token = self.token.as_ref().ok_or(GithubError::TokenRequired)?;
        let url = format!("{GITHUB_API_BASE}/repos/{base_repo}/releases/tags/{tag}");

        let response = self
            .client
            .get(&url)
            .header("Authorization", format!("Bearer {token}"))
            .send()
            .map_err(|source| GithubError::Request {
                operation: "release",
                url: url.clone(),
                source,
            })?;

        if !response.status().is_success() {
            return Err(GithubError::ApiStatus {
                status: response.status(),
                url,
            });
        }

        let release: ReleaseResponse = response
            .json()
            .map_err(|source| GithubError::ParseResponse { url, source })?;

        Ok(release.published_at)
    }

    /// Fetch the tag date from an annotated tag object
    ///
    /// # Errors
    ///
    /// Returns an error if no token is set, the request fails, or the response cannot be parsed.
    fn fetch_tag_date(&self, base_repo: &str, sha: &str) -> Result<Option<String>, GithubError> {
        let token = self.token.as_ref().ok_or(GithubError::TokenRequired)?;
        let url = format!("{GITHUB_API_BASE}/repos/{base_repo}/git/tags/{sha}");

        let response = self
            .client
            .get(&url)
            .header("Authorization", format!("Bearer {token}"))
            .send()
            .map_err(|source| GithubError::Request {
                operation: "tag",
                url: url.clone(),
                source,
            })?;

        if !response.status().is_success() {
            return Err(GithubError::ApiStatus {
                status: response.status(),
                url,
            });
        }

        let tag: TagObjectResponse = response
            .json()
            .map_err(|source| GithubError::ParseResponse { url, source })?;

        Ok(tag.tagger.and_then(|t| t.date))
    }

    /// Given a commit SHA and a list of version tags, find the most specific tag
    /// (highest semver) pointing to that SHA.
    ///
    /// This method resolves each tag to its SHA and filters to tags matching the target.
    /// Among matches, returns the tag with the highest semantic version.
    ///
    /// # Errors
    ///
    /// Returns an error if no token is set, the request fails, or the response cannot be parsed.
    pub fn resolve_version_for_sha(
        &self,
        owner_repo: &str,
        sha: &str,
        tags: &[String],
    ) -> Result<Option<Version>, GithubError> {
        use crate::domain::action::Version as VersionType;

        // Find all tags from the provided list that point to this SHA
        let matching_tags: Vec<VersionType> = tags
            .iter()
            .filter_map(|tag| {
                // Try to resolve this tag to its SHA
                let url = format!(
                    "{GITHUB_API_BASE}/repos/{}/git/ref/tags/{}",
                    owner_repo.split('/').take(2).collect::<Vec<_>>().join("/"),
                    tag
                );
                match self.fetch_ref(&url) {
                    Ok(tag_sha) if tag_sha == sha => Some(VersionType::from(tag.as_str())),
                    _ => None,
                }
            })
            .collect();

        // Return the tag with the highest semver
        Ok(VersionType::highest(&matching_tags))
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
    fn lookup_sha(&self, id: &ActionId, version: &Version) -> Result<ResolvedRef, ResolutionError> {
        let (sha, ref_type) =
            self.resolve_ref(id.as_str(), version.as_str())
                .map_err(|e| match e {
                    GithubError::TokenRequired => ResolutionError::TokenRequired,
                    _ => ResolutionError::ResolveFailed {
                        spec: ActionSpec::new(id.clone(), version.clone()),
                        reason: e.to_string(),
                    },
                })?;

        let base_repo = id.base_repo();

        // Fetch date with priority: release > annotated tag > commit
        let date = if ref_type == RefType::Tag {
            // For tags, try release first, then tag object, then commit
            self.fetch_release_date(&base_repo, version.as_str())
                .ok()
                .flatten()
                .or_else(|| self.fetch_tag_date(&base_repo, &sha).ok().flatten())
                .or_else(|| self.fetch_commit_date(&base_repo, &sha).ok().flatten())
                .unwrap_or_default()
        } else if ref_type == RefType::Release {
            // For releases, try release first, then fall back to commit
            self.fetch_release_date(&base_repo, version.as_str())
                .ok()
                .flatten()
                .or_else(|| self.fetch_commit_date(&base_repo, &sha).ok().flatten())
                .unwrap_or_default()
        } else {
            // For branches and commits, just get the commit date
            self.fetch_commit_date(&base_repo, &sha)
                .ok()
                .flatten()
                .unwrap_or_default()
        };

        Ok(ResolvedRef::new(
            CommitSha::from(sha),
            base_repo,
            ref_type,
            date,
        ))
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
        let (result_sha, result_type) = client.resolve_ref("actions/checkout", sha).unwrap();
        assert_eq!(result_sha, sha);
        assert_eq!(result_type, RefType::Commit);
    }

    #[test]
    fn test_subpath_action_extracts_base_repo() {
        let client = GithubRegistry::new(None).unwrap();
        let sha = "a1b2c3d4e5f6789012345678901234567890abcd";
        // Should work with subpath actions
        let (result_sha, result_type) = client
            .resolve_ref("github/codeql-action/upload-sarif", sha)
            .unwrap();
        assert_eq!(result_sha, sha);
        assert_eq!(result_type, RefType::Commit);
    }

    #[test]
    fn test_version_resolver_trait() {
        let client = GithubRegistry::new(None).unwrap();
        let id = ActionId::from("actions/checkout");
        let sha_version = Version::from("a1b2c3d4e5f6789012345678901234567890abcd");

        // Full SHA should pass through
        let result = client.lookup_sha(&id, &sha_version).unwrap();
        assert_eq!(result.sha.as_str(), sha_version.as_str());
        assert_eq!(result.ref_type, RefType::Commit);
    }

    #[test]
    #[ignore = "requires GITHUB_TOKEN and network access"]
    fn test_resolve_ref_returns_release_for_tag_with_release() {
        // This test requires a valid GITHUB_TOKEN to call the GitHub API
        // It verifies that a tag with an associated release returns RefType::Release
        let token = std::env::var("GITHUB_TOKEN").ok();
        let client = GithubRegistry::new(token).unwrap();
        // Using actions/checkout@v6 as test case (has a GitHub Release)
        let (sha, ref_type) = client.resolve_ref("actions/checkout", "v6").unwrap();
        assert!(!sha.is_empty());
        assert_eq!(ref_type, RefType::Release);
    }

    #[test]
    fn test_resolve_version_for_sha_no_matching_tags() {
        let client = GithubRegistry::new(None).unwrap();
        // Non-existent SHA - no tags will match
        let result = client.resolve_version_for_sha(
            "actions/checkout",
            "0000000000000000000000000000000000000000",
            &["v1.0.0".to_string(), "v2.0.0".to_string()],
        );
        // Should return Ok(None) when no tags match
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }
}
