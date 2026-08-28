//! Repository state lookups over GitHub's REST API.
//!
//! A check deciding "archived or not" must be testable offline, hence the seam:
//! [`RepoMetadata`] is what checks depend on, [`RestRepoMetadata`] is the real adapter, and
//! `CannedRepos` below is the double. Same shape as [`super::advisory`].
//!
//! Separate from the advisory seam because it answers a different question against a
//! different endpoint: merging them would force each caller to depend on the half it never
//! calls.

use super::Error;
use super::Registry;
use super::resolve::GITHUB_API_BASE;
use serde::Deserialize;

/// The state of a repository an action is published from, normalized at the integration
/// edge so check logic never touches raw JSON.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct RepoMeta {
    /// Whether GitHub reports the repository as archived — accepting no commits, so no
    /// future version can ever be published.
    pub archived: bool,
    /// When the repository was last pushed to, as GitHub sends it.
    ///
    /// Kept as the raw string: it is only ever displayed, never compared or sorted, so
    /// parsing it would add a failure mode without buying any behavior.
    pub pushed_at: String,
}

/// Source of repository state for an action.
///
/// Implemented by [`RestRepoMetadata`] (real) and `CannedRepos` (tests).
pub trait RepoMetadata {
    /// The state of `repo`, an `owner/repo` slug.
    ///
    /// Any failure to establish that state is an `Err`, never a default [`RepoMeta`], so a
    /// caller cannot mistake "could not check" for "not archived".
    ///
    /// # Errors
    ///
    /// Returns [`Error`] if the request fails, is rejected, or cannot be parsed.
    fn metadata(&self, repo: &str) -> Result<RepoMeta, Error>;
}

/// Real adapter: reads repository state over the shared blocking HTTP client.
pub struct RestRepoMetadata {
    /// The authenticated client. Reused from [`Registry`] so timeout, user-agent, and
    /// token handling stay in one place.
    registry: Registry,
}

impl RestRepoMetadata {
    /// Wrap an authenticated registry as a repository-state source.
    #[must_use]
    pub const fn new(registry: Registry) -> Self {
        Self { registry }
    }
}

impl RepoMetadata for RestRepoMetadata {
    fn metadata(&self, repo: &str) -> Result<RepoMeta, Error> {
        let url = format!("{GITHUB_API_BASE}/repos/{repo}");
        self.registry.get_json(&url, "repository metadata")
    }
}

/// The [`RepoMetadata`] test double.
///
/// Lives in this file rather than its own because `src/infra/github/` is at its 8-file
/// budget, which is also why [`super::advisory`] keeps its fake inline.
#[cfg(test)]
pub mod fake {
    use super::{Error, GITHUB_API_BASE, RepoMeta, RepoMetadata};
    use std::cell::RefCell;

    /// Returns pre-seeded repository state without issuing any request, so the check that
    /// judges whether an action is archived can be unit-tested with no network.
    pub struct CannedRepos {
        /// What every lookup returns. `Err` models a failed request so callers can be
        /// tested on the path where reporting "not archived" would be a lie.
        result: Result<RepoMeta, ()>,
        /// Slugs passed to `metadata`, in call order. Asserting on these is the only way
        /// to observe that a subpath action was looked up by its base repository.
        pub seen: RefCell<Vec<String>>,
    }

    impl CannedRepos {
        /// A source reporting every repository as archived at `pushed_at`.
        #[must_use]
        pub fn archived(pushed_at: &str) -> Self {
            Self::returning(RepoMeta {
                archived: true,
                pushed_at: pushed_at.to_owned(),
            })
        }

        /// A source reporting every repository as active.
        #[must_use]
        pub fn active() -> Self {
            Self::returning(RepoMeta {
                archived: false,
                pushed_at: "2026-08-01T00:00:00Z".to_owned(),
            })
        }

        /// A source returning `meta` for every lookup.
        #[must_use]
        pub fn returning(meta: RepoMeta) -> Self {
            Self {
                result: Ok(meta),
                seen: RefCell::new(Vec::new()),
            }
        }

        /// A source whose every lookup fails.
        #[must_use]
        pub fn failing() -> Self {
            Self {
                result: Err(()),
                seen: RefCell::new(Vec::new()),
            }
        }
    }

    impl RepoMetadata for CannedRepos {
        fn metadata(&self, repo: &str) -> Result<RepoMeta, Error> {
            self.seen.borrow_mut().push(repo.to_owned());
            self.result.clone().map_err(|()| Error::NotFound {
                url: format!("{GITHUB_API_BASE}/repos/{repo}"),
            })
        }
    }
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    reason = "tests use unwrap, indexing, and other patterns freely"
)]
mod tests {
    use super::RepoMeta;

    #[test]
    fn parses_the_documented_repository_payload() {
        // Field names asserted against a literal payload in GitHub's documented shape, so
        // a rename breaks this test rather than production. Extra keys are ignored, which
        // is why only the two the check reads appear beyond the noise.
        let raw = r#"{
          "id": 12345,
          "full_name": "actions/setup-ruby",
          "archived": true,
          "disabled": false,
          "pushed_at": "2021-04-14T18:22:31Z"
        }"#;
        let meta: RepoMeta = serde_json::from_str(raw).unwrap();

        assert!(meta.archived);
        assert_eq!(meta.pushed_at, "2021-04-14T18:22:31Z");
    }

    #[test]
    fn parses_an_active_repository() {
        let raw = r#"{"archived": false, "pushed_at": "2026-08-01T09:15:00Z"}"#;
        let meta: RepoMeta = serde_json::from_str(raw).unwrap();

        assert!(!meta.archived);
        assert_eq!(meta.pushed_at, "2026-08-01T09:15:00Z");
    }

    #[test]
    fn a_payload_missing_archived_does_not_decode_as_not_archived() {
        // Defaulting the field would turn an unexpected response into a silent "clean",
        // which is the failure mode this check exists to prevent.
        let raw = r#"{"pushed_at": "2021-04-14T18:22:31Z"}"#;
        serde_json::from_str::<RepoMeta>(raw).unwrap_err();
    }
}
