use super::action::identity::{ActionId, CommitDate, CommitSha, Repository, Version};
use super::action::resolved::{Commit, Resolved, ResolvedRef};
use super::action::spec::Spec as ActionSpec;
use super::action::specifier::Specifier;

use super::action::tag_selection::{ShaIndex, select_most_specific_tag};
use super::action::uses_ref::RefType;
use thiserror::Error;

/// Errors that can occur during version resolution.
#[derive(Debug, Clone, Error)]
pub enum Error {
    #[error("failed to resolve {spec}: {reason}")]
    ResolveFailed { spec: ActionSpec, reason: String },

    #[error("no tags found for {action} at SHA {sha}")]
    NoTagsForSha { action: ActionId, sha: CommitSha },

    #[error("GitHub API rate limit exceeded")]
    RateLimited,

    #[error("GitHub API authorization required")]
    AuthRequired,
}

impl Error {
    /// Returns `true` for errors that are transient and the caller can retry later.
    #[must_use]
    pub fn is_recoverable(&self) -> bool {
        matches!(self, Self::RateLimited | Self::AuthRequired)
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

    /// Get all tags that point to a specific SHA.
    ///
    /// # Errors
    ///
    /// Returns an error if the lookup fails.
    fn tags_for_sha(&self, id: &ActionId, sha: &CommitSha) -> Result<Vec<Version>, Error>;

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
        ActionId, ActionResolver, ActionSpec, Commit, CommitDate, CommitSha, Error, RefType,
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

        fn tags_for_sha(&self, _id: &ActionId, _sha: &CommitSha) -> Result<Vec<Version>, Error> {
            self.tags_result.clone()
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
            resolve_result: Err(Error::AuthRequired),
            tags_result: Ok(vec![]),
        };
        let service = ActionResolver::new(&registry);
        let id = ActionId::from("owner/repo");
        let sha = CommitSha::from("abc123def456789012345678901234567890abcd");
        let mut sha_index = ShaIndex::new();

        let result = service.resolve_from_sha(&id, &sha, &mut sha_index);
        assert!(
            matches!(result, Err(Error::AuthRequired)),
            "describe_sha error should propagate through resolve_from_sha"
        );
    }

    #[test]
    fn is_recoverable_rate_limited() {
        assert!(Error::RateLimited.is_recoverable());
    }

    #[test]
    fn is_recoverable_auth_required() {
        assert!(Error::AuthRequired.is_recoverable());
    }

    #[test]
    fn is_recoverable_resolve_failed_is_not_recoverable() {
        let err = Error::ResolveFailed {
            spec: ActionSpec::new(ActionId::from("actions/checkout"), Specifier::from_v1("v4")),
            reason: "not found".to_owned(),
        };
        assert!(!err.is_recoverable());
    }

    #[test]
    fn is_recoverable_no_tags_for_sha_is_not_recoverable() {
        let err = Error::NoTagsForSha {
            action: ActionId::from("actions/checkout"),
            sha: CommitSha::from("abc123def456789012345678901234567890abcd"),
        };
        assert!(!err.is_recoverable());
    }
}
