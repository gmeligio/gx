use crate::domain::action::identity::{ActionId, CommitDate, CommitSha, Version};
use crate::domain::action::resolved::Commit;
use crate::domain::action::spec::Spec as ActionSpec;
use crate::domain::action::specifier::Specifier;
use crate::domain::action::uses_ref::RefType;
use crate::domain::resolution::{Error as ResolutionError, ShaDescription, VersionRegistry};
use std::time::Duration;
use thiserror::Error;

/// Ref resolution and tag lookup against the GitHub API.
mod resolve;
/// GitHub API response deserialization types.
mod responses;

/// HTTP User-Agent header value sent with all GitHub API requests.
const USER_AGENT: &str = "gx-cli";
/// Timeout in seconds for each HTTP request to the GitHub API.
const REQUEST_TIMEOUT_SECS: u64 = 30;

/// Errors that can occur when interacting with the Github API.
#[derive(Debug, Error)]
pub enum Error {
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

/// GitHub API client for resolving action versions and commit SHAs.
#[derive(Clone)]
pub struct Registry {
    /// The HTTP client used for API requests.
    pub client: reqwest::blocking::Client,
    /// Optional personal access token for authenticated requests.
    pub token: Option<crate::config::GitHubToken>,
}

impl Registry {
    /// Create a new Github client with a custom token.
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
    pub fn new(token: Option<crate::config::GitHubToken>) -> Result<Self, Error> {
        let client = reqwest::blocking::Client::builder()
            .user_agent(USER_AGENT)
            .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
            .build()
            .map_err(Error::ClientInit)?;

        Ok(Self { client, token })
    }

    /// Build a GET request, attaching the Authorization header only if a token is set.
    pub(super) fn authenticated_get(&self, url: &str) -> reqwest::blocking::RequestBuilder {
        let req = self.client.get(url);
        match &self.token {
            Some(token) => req.header("Authorization", format!("Bearer {}", token.as_str())),
            None => req,
        }
    }

    /// Classify a non-success HTTP response into the appropriate `Error` variant.
    pub(super) fn check_status(response: &reqwest::blocking::Response, url: &str) -> Error {
        let status = response.status();
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Error::RateLimited {
                url: url.to_owned(),
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
                return Error::RateLimited {
                    url: url.to_owned(),
                };
            }
            return Error::Unauthorized {
                url: url.to_owned(),
            };
        }
        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Error::Unauthorized {
                url: url.to_owned(),
            };
        }
        if status == reqwest::StatusCode::NOT_FOUND {
            return Error::NotFound {
                url: url.to_owned(),
            };
        }
        Error::ApiError {
            status: status.as_u16(),
            url: url.to_owned(),
        }
    }
}

impl VersionRegistry for Registry {
    fn lookup_sha(&self, id: &ActionId, version: &Version) -> Result<Commit, ResolutionError> {
        let (sha, ref_type) =
            self.resolve_ref(id.as_str(), version.as_str())
                .map_err(|e| match e {
                    Error::RateLimited { .. } => ResolutionError::RateLimited,
                    Error::Unauthorized { .. } => ResolutionError::AuthRequired,
                    Error::ClientInit(_)
                    | Error::Request { .. }
                    | Error::NotFound { .. }
                    | Error::ApiError { .. }
                    | Error::ParseResponse { .. } => ResolutionError::ResolveFailed {
                        spec: ActionSpec::new(id.clone(), Specifier::from_v1(version.as_str())),
                        reason: e.to_string(),
                    },
                })?;

        let base_repo = id.base_repo();
        let base_repo_str = base_repo.as_str();

        // Fetch date with priority: release > annotated tag > commit
        let date = if ref_type == Some(RefType::Tag) {
            // For tags, try release first, then tag object, then commit
            self.fetch_release_date(base_repo_str, version.as_str())
                .ok()
                .flatten()
                .or_else(|| self.fetch_tag_date(base_repo_str, &sha).ok().flatten())
                .or_else(|| self.fetch_commit_date(base_repo_str, &sha).ok().flatten())
                .unwrap_or_default()
        } else if ref_type == Some(RefType::Release) {
            // For releases, try release first, then fall back to commit
            self.fetch_release_date(base_repo_str, version.as_str())
                .ok()
                .flatten()
                .or_else(|| self.fetch_commit_date(base_repo_str, &sha).ok().flatten())
                .unwrap_or_default()
        } else {
            // For branches and commits, just get the commit date
            self.fetch_commit_date(base_repo_str, &sha)
                .ok()
                .flatten()
                .unwrap_or_default()
        };

        Ok(Commit {
            sha: CommitSha::from(sha),
            repository: base_repo,
            ref_type,
            date: CommitDate::from(date),
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
                Error::RateLimited { .. } => ResolutionError::RateLimited,
                Error::Unauthorized { .. } => ResolutionError::AuthRequired,
                Error::ClientInit(_)
                | Error::Request { .. }
                | Error::NotFound { .. }
                | Error::ApiError { .. }
                | Error::ParseResponse { .. } => ResolutionError::NoTagsForSha {
                    action: id.clone(),
                    sha: sha.clone(),
                },
            })
    }

    fn all_tags(&self, id: &ActionId) -> Result<Vec<Version>, ResolutionError> {
        self.get_version_tags(id.as_str())
            .map(|tags| tags.into_iter().map(Version::from).collect())
            .map_err(|e| match e {
                Error::RateLimited { .. } => ResolutionError::RateLimited,
                Error::Unauthorized { .. } => ResolutionError::AuthRequired,
                Error::ClientInit(_)
                | Error::Request { .. }
                | Error::NotFound { .. }
                | Error::ApiError { .. }
                | Error::ParseResponse { .. } => ResolutionError::ResolveFailed {
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
            .fetch_commit_date(base_repo.as_str(), sha.as_str())
            .map_err(|e| match e {
                Error::RateLimited { .. } => ResolutionError::RateLimited,
                Error::Unauthorized { .. } => ResolutionError::AuthRequired,
                Error::ClientInit(_)
                | Error::Request { .. }
                | Error::NotFound { .. }
                | Error::ApiError { .. }
                | Error::ParseResponse { .. } => ResolutionError::ResolveFailed {
                    spec: ActionSpec::new(id.clone(), Specifier::Sha(sha.as_str().to_owned())),
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
            date: CommitDate::from(date),
        })
    }
}
