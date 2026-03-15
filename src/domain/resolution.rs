use super::action::identity::{ActionId, CommitSha, Version};
use super::action::resolved::Resolved as ResolvedAction;
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
    pub repository: String,
    pub date: String,
}

/// The result of resolving a ref to its metadata.
#[derive(Debug, Clone)]
pub struct ResolvedRef {
    pub sha: CommitSha,
    pub repository: String,
    pub ref_type: Option<RefType>,
    pub date: String,
}

impl ResolvedRef {
    /// Create a new resolved reference.
    #[must_use]
    pub fn new(
        sha: CommitSha,
        repository: String,
        ref_type: Option<RefType>,
        date: String,
    ) -> Self {
        Self {
            sha,
            repository,
            ref_type,
            date,
        }
    }
}

/// Trait for querying available versions and commit SHAs from a remote registry.
pub trait VersionRegistry {
    /// Look up the commit SHA and metadata for a version reference.
    ///
    /// # Errors
    ///
    /// Returns an error if the lookup fails.
    fn lookup_sha(&self, id: &ActionId, version: &Version) -> Result<ResolvedRef, Error>;

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
    pub fn resolve(&self, spec: &ActionSpec) -> Result<ResolvedAction, Error> {
        let lookup_version = Version::from(spec.version.to_comment());
        let resolved_ref = self.registry.lookup_sha(&spec.id, &lookup_version)?;
        Ok(ResolvedAction::new(
            spec.id.clone(),
            spec.version.clone(),
            resolved_ref.sha,
            resolved_ref.repository,
            resolved_ref.ref_type,
            resolved_ref.date,
        ))
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
    ) -> Result<ResolvedAction, Error> {
        let desc = sha_index.get_or_describe(self.registry, id, sha)?;
        let version =
            select_most_specific_tag(&desc.tags).unwrap_or_else(|| Version::from(sha.as_str()));
        let ref_type = if desc.tags.is_empty() {
            Some(RefType::Commit)
        } else {
            Some(RefType::Tag)
        };
        Ok(ResolvedAction::new(
            id.clone(),
            Specifier::from_v1(version.as_str()),
            sha.clone(),
            desc.repository.clone(),
            ref_type,
            desc.date.clone(),
        ))
    }

    /// Correct a version based on the commit SHA it points to.
    /// Returns `(best_version, was_corrected)`.
    /// If the best tag matches the `original_version`, `was_corrected` is false.
    /// This is a pure version-correction step; metadata resolution is done separately via `resolve()`.
    pub fn correct_version(
        &self,
        id: &ActionId,
        sha: &CommitSha,
        original_version: &Version,
        sha_index: &mut ShaIndex,
    ) -> (Version, bool) {
        match sha_index.get_or_describe(self.registry, id, sha) {
            Ok(desc) => {
                let tags = &desc.tags;
                // If the original version is already a valid tag, keep it
                if tags.contains(original_version) {
                    return (original_version.clone(), false);
                }
                if let Some(tag) = select_most_specific_tag(tags) {
                    (tag, true)
                } else {
                    (original_version.clone(), false)
                }
            }
            Err(_e) => (original_version.clone(), false),
        }
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
        ActionId, ActionResolver, ActionSpec, CommitSha, Error, RefType, ResolvedRef,
        ShaDescription, ShaIndex, Specifier, Version, VersionRegistry,
    };

    struct MockRegistry {
        resolve_result: Result<ResolvedRef, Error>,
        tags_result: Result<Vec<Version>, Error>,
    }

    impl VersionRegistry for MockRegistry {
        fn lookup_sha(&self, _id: &ActionId, _version: &Version) -> Result<ResolvedRef, Error> {
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
            resolve_result: Ok(ResolvedRef::new(
                CommitSha::from("abc123def456789012345678901234567890abcd"),
                "actions/checkout".to_owned(),
                Some(RefType::Tag),
                "2026-01-01T00:00:00Z".to_owned(),
            )),
            tags_result: Ok(vec![]),
        };
        let service = ActionResolver::new(&mock_registry);

        let spec = ActionSpec::new(ActionId::from("actions/checkout"), Specifier::from_v1("v4"));
        let result = service.resolve(&spec);

        let resolved = result.expect("Expected Ok result");
        assert_eq!(resolved.id.as_str(), "actions/checkout");
        assert_eq!(resolved.version.to_comment(), "v4");
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
    fn correct_version_no_correction_needed() {
        let registry = MockRegistry {
            resolve_result: Ok(ResolvedRef::new(
                CommitSha::from("abc123def456789012345678901234567890abcd"),
                "actions/checkout".to_owned(),
                Some(RefType::Tag),
                "2026-01-01T00:00:00Z".to_owned(),
            )),
            tags_result: Ok(vec![Version::from("v4"), Version::from("v4.0.0")]),
        };
        let service = ActionResolver::new(&registry);

        let id = ActionId::from("actions/checkout");
        let sha = CommitSha::from("abc123def456789012345678901234567890abcd");
        let original_version = Version::from("v4");
        let mut sha_index = ShaIndex::new();
        let (version, was_corrected) =
            service.correct_version(&id, &sha, &original_version, &mut sha_index);

        assert_eq!(version.as_str(), "v4");
        assert!(!was_corrected);
    }

    #[test]
    fn correct_version_correction_needed() {
        let registry = MockRegistry {
            resolve_result: Ok(ResolvedRef::new(
                CommitSha::from("abc123def456789012345678901234567890abcd"),
                "actions/checkout".to_owned(),
                Some(RefType::Tag),
                "2026-01-01T00:00:00Z".to_owned(),
            )),
            tags_result: Ok(vec![Version::from("v5"), Version::from("v5.0.0")]),
        };
        let service = ActionResolver::new(&registry);

        let id = ActionId::from("actions/checkout");
        let sha = CommitSha::from("abc123def456789012345678901234567890abcd");
        let original_version = Version::from("v4");
        let mut sha_index = ShaIndex::new();
        let (version, was_corrected) =
            service.correct_version(&id, &sha, &original_version, &mut sha_index);

        assert_eq!(version.as_str(), "v5.0.0");
        assert!(was_corrected);
    }

    #[test]
    fn resolve_from_sha_with_tags() {
        let sha = CommitSha::from("abc123def456789012345678901234567890abcd");
        let registry = MockRegistry {
            resolve_result: Ok(ResolvedRef::new(
                sha.clone(),
                "owner/repo".to_owned(),
                Some(RefType::Commit),
                "2026-01-01T00:00:00Z".to_owned(),
            )),
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

        assert_eq!(result.version.to_comment(), "v3.6.1");
        assert_eq!(result.commit.sha, sha);
        assert_eq!(result.commit.ref_type, Some(RefType::Tag));
        assert_eq!(result.commit.repository, "owner/repo");
    }

    #[test]
    fn resolve_from_sha_no_tags() {
        let sha = CommitSha::from("abc123def456789012345678901234567890abcd");
        let registry = MockRegistry {
            resolve_result: Ok(ResolvedRef::new(
                sha.clone(),
                "owner/repo".to_owned(),
                Some(RefType::Commit),
                "2026-01-01T00:00:00Z".to_owned(),
            )),
            tags_result: Ok(vec![]),
        };
        let service = ActionResolver::new(&registry);
        let id = ActionId::from("owner/repo");
        let mut sha_index = ShaIndex::new();

        let result = service
            .resolve_from_sha(&id, &sha, &mut sha_index)
            .expect("Expected Ok result");

        assert_eq!(result.version.as_str(), sha.as_str());
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
