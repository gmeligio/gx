use crate::domain::{
    ActionId, ActionSpec, CommitSha, RefType, ResolutionError, ResolvedRef, ShaDescription,
    Specifier, Version, VersionRegistry,
};
use std::time::Duration;
use thiserror::Error;

mod resolve;
mod responses;

const USER_AGENT: &str = "gx-cli";
const REQUEST_TIMEOUT_SECS: u64 = 30;

/// Errors that can occur when interacting with the Github API
#[derive(Debug, Error)]
pub enum GithubError {
    #[error("failed to create HTTP client")]
    ClientInit(#[source] reqwest::Error),

    #[error("failed to fetch {operation} from {url}")]
    Request {
        operation: &'static str,
        url: String,
        #[source]
        source: reqwest::Error,
    },

    #[error("GitHub API rate limit exceeded for {url}")]
    RateLimited { url: String },

    #[error("GitHub API unauthorized for {url}")]
    Unauthorized { url: String },

    #[error("GitHub API not found: {url}")]
    NotFound { url: String },

    #[error("GitHub API returned status {status} for {url}")]
    ApiError { status: u16, url: String },

    #[error("failed to parse response from {url}")]
    ParseResponse {
        url: String,
        #[source]
        source: reqwest::Error,
    },
}

#[derive(Clone)]
pub struct GithubRegistry {
    pub(super) client: reqwest::blocking::Client,
    pub(super) token: Option<String>,
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

    /// Build a GET request, attaching the Authorization header only if a token is set.
    pub(super) fn authenticated_get(&self, url: &str) -> reqwest::blocking::RequestBuilder {
        let req = self.client.get(url);
        match &self.token {
            Some(token) => req.header("Authorization", format!("Bearer {token}")),
            None => req,
        }
    }

    /// Classify a non-success HTTP response into the appropriate `GithubError` variant.
    pub(super) fn check_status(response: &reqwest::blocking::Response, url: &str) -> GithubError {
        let status = response.status();
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return GithubError::RateLimited {
                url: url.to_string(),
            };
        }
        if status == reqwest::StatusCode::FORBIDDEN {
            let remaining = response
                .headers()
                .get("x-ratelimit-remaining")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(1);
            if remaining == 0 {
                return GithubError::RateLimited {
                    url: url.to_string(),
                };
            }
            return GithubError::Unauthorized {
                url: url.to_string(),
            };
        }
        if status == reqwest::StatusCode::UNAUTHORIZED {
            return GithubError::Unauthorized {
                url: url.to_string(),
            };
        }
        if status == reqwest::StatusCode::NOT_FOUND {
            return GithubError::NotFound {
                url: url.to_string(),
            };
        }
        GithubError::ApiError {
            status: status.as_u16(),
            url: url.to_string(),
        }
    }
}

impl VersionRegistry for GithubRegistry {
    fn lookup_sha(&self, id: &ActionId, version: &Version) -> Result<ResolvedRef, ResolutionError> {
        let (sha, ref_type) =
            self.resolve_ref(id.as_str(), version.as_str())
                .map_err(|e| match e {
                    GithubError::RateLimited { .. } => ResolutionError::RateLimited,
                    GithubError::Unauthorized { .. } => ResolutionError::AuthRequired,
                    _ => ResolutionError::ResolveFailed {
                        spec: ActionSpec::new(id.clone(), Specifier::from_v1(version.as_str())),
                        reason: e.to_string(),
                    },
                })?;

        let base_repo = id.base_repo();

        // Fetch date with priority: release > annotated tag > commit
        let date = if ref_type == Some(RefType::Tag) {
            // For tags, try release first, then tag object, then commit
            self.fetch_release_date(&base_repo, version.as_str())
                .ok()
                .flatten()
                .or_else(|| self.fetch_tag_date(&base_repo, &sha).ok().flatten())
                .or_else(|| self.fetch_commit_date(&base_repo, &sha).ok().flatten())
                .unwrap_or_default()
        } else if ref_type == Some(RefType::Release) {
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
                GithubError::RateLimited { .. } => ResolutionError::RateLimited,
                GithubError::Unauthorized { .. } => ResolutionError::AuthRequired,
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
                GithubError::RateLimited { .. } => ResolutionError::RateLimited,
                GithubError::Unauthorized { .. } => ResolutionError::AuthRequired,
                _ => ResolutionError::ResolveFailed {
                    spec: ActionSpec::new(id.clone(), Specifier::Ref(String::new())),
                    reason: e.to_string(),
                },
            })
    }

    fn describe_sha(
        &self,
        id: &ActionId,
        sha: &CommitSha,
    ) -> Result<ShaDescription, ResolutionError> {
        let base_repo = id.base_repo();

        // Fetch commit date directly — no tag/branch fallback chain needed since SHA is trusted
        let date = self
            .fetch_commit_date(&base_repo, sha.as_str())
            .map_err(|e| match e {
                GithubError::RateLimited { .. } => ResolutionError::RateLimited,
                GithubError::Unauthorized { .. } => ResolutionError::AuthRequired,
                _ => ResolutionError::ResolveFailed {
                    spec: ActionSpec::new(id.clone(), Specifier::Sha(sha.as_str().to_string())),
                    reason: e.to_string(),
                },
            })?
            .unwrap_or_default();

        // Tag lookup is non-fatal: return empty tags on failure
        let tags = self
            .get_tags_for_sha(id.as_str(), sha.as_str())
            .unwrap_or_default()
            .into_iter()
            .map(Version::from)
            .collect();

        Ok(ShaDescription {
            tags,
            repository: base_repo,
            date,
        })
    }
}

impl Default for GithubRegistry {
    fn default() -> Self {
        Self::new(None).expect("Failed to create Github client")
    }
}
