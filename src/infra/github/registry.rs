use crate::domain::action::identity::{ActionId, CommitDate, CommitSha, Version};
use crate::domain::action::resolved::Commit;
use crate::domain::action::spec::Spec as ActionSpec;
use crate::domain::action::specifier::Specifier;
use crate::domain::action::uses_ref::RefType;
use crate::domain::resolution::{
    Error as ResolutionError, Forge, RetryAfter, ShaDescription, VersionRegistry,
};
use std::time::Duration;
use thiserror::Error;

/// HTTP User-Agent header value sent with all GitHub API requests.
const USER_AGENT: &str = "gx-cli";
/// Timeout in seconds for each HTTP request to the GitHub API.
const REQUEST_TIMEOUT_SECS: u64 = 30;
/// Longest reset delay still worth waiting out, in seconds.
///
/// An exhausted unauthenticated quota can reset nearly an hour out; blocking a
/// terminal that long is worse than failing, so anything beyond this becomes
/// [`RetryAfter::TooDistant`].
///
/// The retry layer's backoff schedule must stay under this, which
/// `infra::registry::retrying` asserts against at compile time.
pub const MAX_RETRY_WAIT_SECS: u64 = 5;

/// Reduce GitHub's absolute `x-ratelimit-reset` epoch to a wait worth taking.
///
/// Both operands are passed in so the skew and cap cases are unit-testable
/// without faking a clock. Subtraction saturates: a reset already in the past —
/// including one that only looks past because the local clock runs ahead of
/// GitHub's — yields a zero wait rather than a negative or a panic.
fn normalize_reset(reset_epoch: u64, now_epoch: u64) -> RetryAfter {
    let secs = reset_epoch.saturating_sub(now_epoch);
    if secs > MAX_RETRY_WAIT_SECS {
        return RetryAfter::TooDistant;
    }
    RetryAfter::After(Duration::from_secs(secs))
}

/// Read `x-ratelimit-reset` from a response and normalize it against the clock.
///
/// An absent or unparseable header is [`RetryAfter::Unstated`] — the retry layer
/// falls back to its own backoff rather than guessing at a reset time.
fn reset_from_headers(response: &reqwest::blocking::Response) -> RetryAfter {
    let Some(reset_epoch) = response
        .headers()
        .get("x-ratelimit-reset")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok())
    else {
        return RetryAfter::Unstated;
    };
    let now_epoch = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    normalize_reset(reset_epoch, now_epoch)
}

/// Errors that can occur when interacting with the Github API.
///
/// These stay GitHub-specific and never leave this module: the
/// [`VersionRegistry`] impl below maps them into the forge-neutral
/// [`ResolutionError`], preserving the concrete cause via `#[source]`.
#[derive(Debug, Error)]
#[non_exhaustive]
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
    RateLimited {
        url: String,
        /// When GitHub said the quota resets, normalized and clamped.
        retry_after: RetryAfter,
    },

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
    /// [`reqwest::blocking`] for details.
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

    /// Send an authenticated GET and deserialize a successful JSON response.
    ///
    /// `operation` names the API call in [`Error::Request`] for diagnostics.
    ///
    /// The status is classified *before* the body is parsed, so a non-2xx
    /// response yields the precise error (e.g. [`Error::NotFound`]) rather than
    /// a [`Error::ParseResponse`] from parsing an error body. Callers such as
    /// `resolve_ref` depend on this to distinguish a missing ref from a
    /// malformed one.
    pub(super) fn get_json<T: serde::de::DeserializeOwned>(
        &self,
        url: &str,
        operation: &'static str,
    ) -> Result<T, Error> {
        let response = self
            .authenticated_get(url)
            .send()
            .map_err(|source| Error::Request {
                operation,
                url: url.to_owned(),
                source,
            })?;

        if !response.status().is_success() {
            return Err(Self::check_status(&response, url));
        }

        response.json().map_err(|source| Error::ParseResponse {
            url: url.to_owned(),
            source,
        })
    }

    /// Build a POST request, attaching the Authorization header only if a token is set.
    ///
    /// Used by the GraphQL seam, which is POST-only. Unlike the REST paths, its caller
    /// requires a token and refuses to run without one — an unauthenticated GraphQL
    /// request is rejected outright, so a token-less POST could only ever fail.
    pub(super) fn authenticated_post(&self, url: &str) -> reqwest::blocking::RequestBuilder {
        let req = self.client.post(url);
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
                retry_after: reset_from_headers(response),
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
                    retry_after: reset_from_headers(response),
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
                    Error::RateLimited { retry_after, .. } => ResolutionError::RateLimited {
                        forge: Forge::GitHub,
                        retry_after,
                    },
                    Error::Unauthorized { .. } => ResolutionError::AuthRequired {
                        forge: Forge::GitHub,
                    },
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

    fn all_tags(&self, id: &ActionId) -> Result<Vec<Version>, ResolutionError> {
        self.get_version_tags(id.as_str())
            .map(|tags| tags.into_iter().map(Version::from).collect())
            .map_err(|e| match e {
                Error::RateLimited { retry_after, .. } => ResolutionError::RateLimited {
                    forge: Forge::GitHub,
                    retry_after,
                },
                Error::Unauthorized { .. } => ResolutionError::AuthRequired {
                    forge: Forge::GitHub,
                },
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
                Error::RateLimited { retry_after, .. } => ResolutionError::RateLimited {
                    forge: Forge::GitHub,
                    retry_after,
                },
                Error::Unauthorized { .. } => ResolutionError::AuthRequired {
                    forge: Forge::GitHub,
                },
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

#[cfg(test)]
mod tests {
    use super::{Duration, MAX_RETRY_WAIT_SECS, RetryAfter, normalize_reset};

    #[test]
    fn near_reset_is_waited_out() {
        assert_eq!(
            normalize_reset(1_000_003, 1_000_000),
            RetryAfter::After(Duration::from_secs(3)),
            "a reset a few seconds out is a wait worth taking"
        );
    }

    #[test]
    fn reset_exactly_at_the_cap_is_still_waited_out() {
        assert_eq!(
            normalize_reset(1_000_000 + MAX_RETRY_WAIT_SECS, 1_000_000),
            RetryAfter::After(Duration::from_secs(MAX_RETRY_WAIT_SECS)),
            "the cap is inclusive, so the boundary does not silently become TooDistant"
        );
    }

    #[test]
    fn hour_out_reset_is_too_distant() {
        assert_eq!(
            normalize_reset(1_003_600, 1_000_000),
            RetryAfter::TooDistant,
            "an exhausted unauthenticated quota must not stall the terminal"
        );
    }

    #[test]
    fn past_reset_does_not_produce_a_negative_wait() {
        // The local clock running ahead of GitHub's must not underflow.
        assert_eq!(
            normalize_reset(1_000_000, 1_000_030),
            RetryAfter::After(Duration::ZERO),
            "a reset already past means retry now, not panic or wrap"
        );
    }
}
