use super::action::identity::{ActionId, CommitDate, CommitSha, Repository, Version};
use super::action::resolved::{Commit, Resolved, ResolvedRef};
use super::action::spec::Spec as ActionSpec;
use super::action::specifier::Specifier;

use super::action::tag_selection::{ShaIndex, select_most_specific_tag};
use super::action::uses_ref::RefType;
use std::fmt;
use thiserror::Error;

/// A code-hosting platform that a [`VersionRegistry`] resolves against.
///
/// Carried as data on the failure variants that are inherently platform-specific,
/// so [`Error`] grows only with failure semantics and never gains a variant per
/// platform.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Forge {
    /// github.com.
    GitHub,
}

impl Forge {
    /// The environment variable holding this forge's API credential.
    ///
    /// Used to tell the user exactly what to set when a request is rejected or
    /// throttled.
    #[must_use]
    pub fn token_env(self) -> &'static str {
        match self {
            Self::GitHub => "GITHUB_TOKEN",
        }
    }
}

impl fmt::Display for Forge {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::GitHub => f.write_str("GitHub"),
        }
    }
}

/// Errors that can occur during version resolution.
///
/// Variants describe *what went wrong*, never which backend produced it: the
/// backend travels as a [`Forge`] field. Each message names both the forge and the
/// remedy, because a skipped resolution is often the only output a user reads.
#[derive(Debug, Clone, Error)]
#[non_exhaustive]
pub enum Error {
    #[error("failed to resolve {spec}: {reason}")]
    ResolveFailed { spec: ActionSpec, reason: String },

    /// The forge's request quota is exhausted.
    ///
    /// Deliberately does not state when the quota resets: that is not read from
    /// the response, and guessing would misinform an unauthenticated user whose
    /// window can be nearly an hour.
    #[error("{forge} rate limit exhausted; set {} to raise the limit", forge.token_env())]
    RateLimited {
        /// The forge whose quota was exhausted.
        forge: Forge,
    },

    /// The forge rejected the request for lack of a usable credential.
    #[error(
        "{forge} requires authorization; set {} to a token with repository read access",
        forge.token_env()
    )]
    AuthRequired {
        /// The forge that rejected the request.
        forge: Forge,
    },
}

impl Error {
    /// Returns `true` when the run may continue without this action.
    ///
    /// A skippable failure is reported as a warning and the lock is written
    /// without that entry; anything else fails the command. Authorization
    /// failures are skippable so that a user without a token still gets a partial
    /// lock and a warning rather than a hard failure.
    ///
    /// This is not the same question as [`Self::is_retryable`].
    #[must_use]
    pub fn is_skippable(&self) -> bool {
        matches!(self, Self::RateLimited { .. } | Self::AuthRequired { .. })
    }

    /// Returns `true` when repeating the identical request could plausibly succeed.
    ///
    /// Only rate limiting qualifies. [`Self::AuthRequired`] is excluded even though
    /// it is skippable: re-issuing a request with the same absent or rejected
    /// credential cannot produce a different outcome, so retrying it only delays
    /// the failure.
    #[must_use]
    pub fn is_retryable(&self) -> bool {
        matches!(self, Self::RateLimited { .. })
    }
}

/// Metadata for a known commit SHA: the tags pointing to it, the base repository, and the commit date.
#[derive(Debug, Clone)]
pub struct ShaDescription {
    pub tags: Vec<Version>,
    pub repository: Repository,
    pub date: CommitDate,
}

/// Trait for querying available versions and commit SHAs from a remote registry.
pub trait VersionRegistry {
    /// Look up the commit SHA and metadata for a version reference.
    ///
    /// # Errors
    ///
    /// Returns an error if the lookup fails.
    fn lookup_sha(&self, id: &ActionId, version: &Version) -> Result<Commit, Error>;

    /// Get all available version tags for an action's repository.
    ///
    /// # Errors
    ///
    /// Returns an error if the lookup fails.
    fn all_tags(&self, id: &ActionId) -> Result<Vec<Version>, Error>;

    /// Describe a known commit SHA: return the tags pointing to it, the base repository, and the commit date.
    ///
    /// # Errors
    ///
    /// Returns an error if the commit lookup fails (tag lookup failure is non-fatal, returns empty tags).
    fn describe_sha(&self, id: &ActionId, sha: &CommitSha) -> Result<ShaDescription, Error>;
}

/// Resolves actions to their correct version and commit SHA.
pub struct ActionResolver<'reg, R: VersionRegistry> {
    /// The version registry used for lookups.
    registry: &'reg R,
}

impl<'reg, R: VersionRegistry> ActionResolver<'reg, R> {
    #[must_use]
    pub fn new(registry: &'reg R) -> Self {
        Self { registry }
    }

    /// Access the underlying version registry.
    #[must_use]
    pub fn registry(&self) -> &R {
        self.registry
    }

    /// Resolve an action spec to a commit SHA.
    ///
    /// # Errors
    ///
    /// Returns `Error` if the registry lookup fails.
    pub fn resolve(&self, spec: &ActionSpec) -> Result<Resolved, Error> {
        let version = Version::from(spec.specifier.to_lookup_tag());
        let commit = self.registry.lookup_sha(&spec.id, &version)?;
        // The specifier dictates the kind: a range resolves a semver tag, a
        // branch ref a branch, and a bare SHA a commit with no version label.
        let reference = match &spec.specifier {
            Specifier::Range { .. } => ResolvedRef::Tag(version),
            Specifier::Ref(_) => ResolvedRef::Branch(version),
            Specifier::Sha(_) => ResolvedRef::Commit,
        };
        Ok(Resolved { reference, commit })
    }

    /// Resolve an action from a known commit SHA.
    /// Derives version (most specific tag) and `ref_type` from tags for the SHA.
    ///
    /// # Errors
    ///
    /// Returns `Error` if the registry lookup fails.
    pub fn resolve_from_sha(
        &self,
        id: &ActionId,
        sha: &CommitSha,
        sha_index: &mut ShaIndex,
    ) -> Result<Resolved, Error> {
        let desc = sha_index.get_or_describe(self.registry, id, sha)?;
        // Establish the kind once, at construction: a tag if one points at this
        // SHA, otherwise a bare commit pin with no version label. No SHA is ever
        // fabricated into a version slot.
        let (reference, ref_type) = match select_most_specific_tag(&desc.tags) {
            Some(tag) => (ResolvedRef::Tag(tag), Some(RefType::Tag)),
            None => (ResolvedRef::Commit, Some(RefType::Commit)),
        };
        Ok(Resolved {
            reference,
            commit: Commit {
                sha: sha.clone(),
                repository: desc.repository.clone(),
                ref_type,
                date: desc.date.clone(),
            },
        })
    }
}

#[cfg(test)]
#[path = "resolution_testutil.rs"]
pub(crate) mod testutil;

#[cfg(test)]
#[expect(
    clippy::expect_used,
    clippy::assertions_on_result_states,
    reason = "tests use unwrap, indexing, and other patterns freely"
)]
mod tests {
    use super::{
        ActionId, ActionResolver, ActionSpec, Commit, CommitDate, CommitSha, Error, Forge, RefType,
        Repository, ShaDescription, ShaIndex, Version, VersionRegistry,
    };
    use crate::domain::action::resolved::ResolvedRef;
    use crate::domain::action::specifier::Specifier;

    struct MockRegistry {
        resolve_result: Result<Commit, Error>,
        tags_result: Result<Vec<Version>, Error>,
    }

    impl VersionRegistry for MockRegistry {
        fn lookup_sha(&self, _id: &ActionId, _version: &Version) -> Result<Commit, Error> {
            self.resolve_result.clone()
        }

        fn all_tags(&self, _id: &ActionId) -> Result<Vec<Version>, Error> {
            self.tags_result.clone()
        }

        fn describe_sha(&self, _id: &ActionId, _sha: &CommitSha) -> Result<ShaDescription, Error> {
            let meta = self.resolve_result.clone()?;
            let tags = self.tags_result.clone().unwrap_or_default();
            Ok(ShaDescription {
                tags,
                repository: meta.repository,
                date: meta.date,
            })
        }
    }

    #[test]
    fn resolve_success() {
        let mock_registry = MockRegistry {
            resolve_result: Ok(Commit {
                sha: CommitSha::from("abc123def456789012345678901234567890abcd"),
                repository: Repository::from("actions/checkout"),
                ref_type: Some(RefType::Tag),
                date: CommitDate::from("2026-01-01T00:00:00Z"),
            }),
            tags_result: Ok(vec![]),
        };
        let service = ActionResolver::new(&mock_registry);

        let spec = ActionSpec::new(ActionId::from("actions/checkout"), Specifier::from_v1("v4"));
        let result = service.resolve(&spec);

        let resolved = result.expect("Expected Ok result");
        assert_eq!(
            resolved.reference,
            ResolvedRef::Tag(Version::from("v4")),
            "a range specifier resolves to a Tag reference"
        );
        assert_eq!(
            resolved.commit.sha.as_str(),
            "abc123def456789012345678901234567890abcd"
        );
    }

    #[test]
    fn resolve_failure() {
        let registry = MockRegistry {
            resolve_result: Err(Error::ResolveFailed {
                spec: ActionSpec::new(ActionId::from("actions/checkout"), Specifier::from_v1("v4")),
                reason: "not found".to_owned(),
            }),
            tags_result: Ok(vec![]),
        };
        let service = ActionResolver::new(&registry);

        let spec = ActionSpec::new(ActionId::from("actions/checkout"), Specifier::from_v1("v4"));
        let result = service.resolve(&spec);

        assert!(result.is_err());
    }

    #[test]
    fn resolve_from_sha_with_tags() {
        let sha = CommitSha::from("abc123def456789012345678901234567890abcd");
        let registry = MockRegistry {
            resolve_result: Ok(Commit {
                sha: sha.clone(),
                repository: Repository::from("owner/repo"),
                ref_type: Some(RefType::Commit),
                date: CommitDate::from("2026-01-01T00:00:00Z"),
            }),
            tags_result: Ok(vec![
                Version::from("v3"),
                Version::from("v3.6"),
                Version::from("v3.6.1"),
            ]),
        };
        let service = ActionResolver::new(&registry);
        let id = ActionId::from("owner/repo");
        let mut sha_index = ShaIndex::new();

        let result = service
            .resolve_from_sha(&id, &sha, &mut sha_index)
            .expect("Expected Ok result");

        assert_eq!(result.reference, ResolvedRef::Tag(Version::from("v3.6.1")));
        assert_eq!(result.commit.sha, sha);
        assert_eq!(result.commit.ref_type, Some(RefType::Tag));
        assert_eq!(result.commit.repository.as_str(), "owner/repo");
    }

    #[test]
    fn resolve_from_sha_no_tags() {
        let sha = CommitSha::from("abc123def456789012345678901234567890abcd");
        let registry = MockRegistry {
            resolve_result: Ok(Commit {
                sha: sha.clone(),
                repository: Repository::from("owner/repo"),
                ref_type: Some(RefType::Commit),
                date: CommitDate::from("2026-01-01T00:00:00Z"),
            }),
            tags_result: Ok(vec![]),
        };
        let service = ActionResolver::new(&registry);
        let id = ActionId::from("owner/repo");
        let mut sha_index = ShaIndex::new();

        let result = service
            .resolve_from_sha(&id, &sha, &mut sha_index)
            .expect("Expected Ok result");

        // No tag points at this SHA: the reference is a bare Commit, not a SHA
        // fabricated into a version slot.
        assert_eq!(result.reference, ResolvedRef::Commit);
        assert_eq!(result.commit.sha, sha);
        assert_eq!(result.commit.ref_type, Some(RefType::Commit));
    }

    #[test]
    fn resolve_from_sha_describe_error_propagates() {
        let registry = MockRegistry {
            resolve_result: Err(Error::AuthRequired {
                forge: Forge::GitHub,
            }),
            tags_result: Ok(vec![]),
        };
        let service = ActionResolver::new(&registry);
        let id = ActionId::from("owner/repo");
        let sha = CommitSha::from("abc123def456789012345678901234567890abcd");
        let mut sha_index = ShaIndex::new();

        let result = service.resolve_from_sha(&id, &sha, &mut sha_index);
        assert!(
            matches!(
                result,
                Err(Error::AuthRequired {
                    forge: Forge::GitHub
                })
            ),
            "describe_sha error should propagate through resolve_from_sha"
        );
    }

    /// A `ResolveFailed` to exercise the predicates against a strict variant.
    fn resolve_failed() -> Error {
        Error::ResolveFailed {
            spec: ActionSpec::new(ActionId::from("actions/checkout"), Specifier::from_v1("v4")),
            reason: "not found".to_owned(),
        }
    }

    #[test]
    fn rate_limited_is_skippable() {
        assert!(
            Error::RateLimited {
                forge: Forge::GitHub
            }
            .is_skippable()
        );
    }

    #[test]
    fn auth_required_is_skippable() {
        // A user with no token still gets a partial lock and a warning rather
        // than a hard failure.
        assert!(
            Error::AuthRequired {
                forge: Forge::GitHub
            }
            .is_skippable()
        );
    }

    #[test]
    fn strict_errors_are_not_skippable() {
        assert!(!resolve_failed().is_skippable());
    }

    #[test]
    fn rate_limited_is_retryable() {
        assert!(
            Error::RateLimited {
                forge: Forge::GitHub
            }
            .is_retryable()
        );
    }

    #[test]
    fn auth_required_is_not_retryable() {
        // Skippable but never retryable: repeating a request with the same
        // absent credential cannot succeed, so a retrying caller must not.
        assert!(
            !Error::AuthRequired {
                forge: Forge::GitHub
            }
            .is_retryable()
        );
    }

    #[test]
    fn strict_errors_are_not_retryable() {
        assert!(!resolve_failed().is_retryable());
    }

    #[test]
    fn rate_limited_message_names_forge_and_remedy() {
        let message = Error::RateLimited {
            forge: Forge::GitHub,
        }
        .to_string();

        assert!(message.contains("GitHub"), "names the forge: {message}");
        assert!(
            message.contains("GITHUB_TOKEN"),
            "names the remedy: {message}"
        );
        assert!(
            !message.contains("resets"),
            "must not claim a reset time it never read: {message}"
        );
    }

    #[test]
    fn auth_required_message_names_forge_and_remedy() {
        let message = Error::AuthRequired {
            forge: Forge::GitHub,
        }
        .to_string();

        assert!(message.contains("GitHub"), "names the forge: {message}");
        assert!(
            message.contains("GITHUB_TOKEN"),
            "names the remedy: {message}"
        );
    }
}
