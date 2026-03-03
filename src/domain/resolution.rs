use log::{debug, warn};
use thiserror::Error;

use super::{ActionId, ActionSpec, CommitSha, RefType, ResolvedAction, Version};

/// Errors that can occur during version resolution
#[derive(Debug, Clone, Error)]
pub enum ResolutionError {
    #[error("failed to resolve {spec}: {reason}")]
    ResolveFailed { spec: ActionSpec, reason: String },

    #[error("no tags found for {action} at SHA {sha}")]
    NoTagsForSha { action: ActionId, sha: CommitSha },

    #[error("token required for resolution")]
    TokenRequired,
}

/// Metadata for a known commit SHA: the tags pointing to it, the base repository, and the commit date.
#[derive(Debug, Clone)]
pub struct ShaDescription {
    pub tags: Vec<Version>,
    pub repository: String,
    pub date: String,
}

/// The result of resolving a ref to its metadata
#[derive(Debug, Clone)]
pub struct ResolvedRef {
    pub sha: CommitSha,
    pub repository: String,
    pub ref_type: RefType,
    pub date: String,
}

impl ResolvedRef {
    /// Create a new resolved reference.
    #[must_use]
    pub fn new(sha: CommitSha, repository: String, ref_type: RefType, date: String) -> Self {
        Self {
            sha,
            repository,
            ref_type,
            date,
        }
    }
}

/// Trait for querying available versions and commit SHAs from a remote registry
pub trait VersionRegistry {
    /// Look up the commit SHA and metadata for a version reference
    ///
    /// # Errors
    ///
    /// Returns an error if the lookup fails
    fn lookup_sha(&self, id: &ActionId, version: &Version) -> Result<ResolvedRef, ResolutionError>;

    /// Get all tags that point to a specific SHA
    ///
    /// # Errors
    ///
    /// Returns an error if the lookup fails
    fn tags_for_sha(&self, id: &ActionId, sha: &CommitSha)
    -> Result<Vec<Version>, ResolutionError>;

    /// Get all available version tags for an action's repository
    ///
    /// # Errors
    ///
    /// Returns an error if the lookup fails
    fn all_tags(&self, id: &ActionId) -> Result<Vec<Version>, ResolutionError>;

    /// Describe a known commit SHA: return the tags pointing to it, the base repository, and the commit date.
    ///
    /// # Errors
    ///
    /// Returns an error if the commit lookup fails (tag lookup failure is non-fatal, returns empty tags).
    fn describe_sha(
        &self,
        id: &ActionId,
        sha: &CommitSha,
    ) -> Result<ShaDescription, ResolutionError>;
}

/// Resolves actions to their correct version and commit SHA
pub struct ActionResolver<R: VersionRegistry> {
    registry: R,
}

impl<R: VersionRegistry> ActionResolver<R> {
    #[must_use]
    pub fn new(registry: R) -> Self {
        Self { registry }
    }

    /// Access the underlying version registry
    #[must_use]
    pub fn registry(&self) -> &R {
        &self.registry
    }

    /// Resolve an action spec to a commit SHA.
    ///
    /// # Errors
    ///
    /// Returns `ResolutionError` if the registry lookup fails.
    pub fn resolve(&self, spec: &ActionSpec) -> Result<ResolvedAction, ResolutionError> {
        debug!("Resolving {spec}");
        let resolved_ref = self.registry.lookup_sha(&spec.id, &spec.version)?;
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
    /// Returns `ResolutionError` if the registry lookup fails.
    pub fn resolve_from_sha(
        &self,
        id: &ActionId,
        sha: &CommitSha,
    ) -> Result<ResolvedAction, ResolutionError> {
        let desc = self.registry.describe_sha(id, sha)?;
        let version =
            select_most_specific_tag(&desc.tags).unwrap_or_else(|| Version::from(sha.as_str()));
        let ref_type = if desc.tags.is_empty() {
            RefType::Commit
        } else {
            RefType::Tag
        };
        Ok(ResolvedAction::new(
            id.clone(),
            version,
            sha.clone(),
            desc.repository,
            ref_type,
            desc.date,
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
    ) -> (Version, bool) {
        match self.registry.tags_for_sha(id, sha) {
            Ok(tags) => {
                // If the original version is already a valid tag, keep it
                if tags.contains(original_version) {
                    return (original_version.clone(), false);
                }
                if let Some(tag) = select_most_specific_tag(&tags) {
                    (tag, true)
                } else {
                    warn!("No tags found for {id} SHA {sha}, keeping version");
                    (original_version.clone(), false)
                }
            }
            Err(e) => {
                if matches!(e, ResolutionError::TokenRequired) {
                    warn!(
                        "GITHUB_TOKEN not set. Without it, cannot correct version for {id} SHA {sha}.",
                    );
                } else {
                    warn!("Could not correct version for {id} SHA {sha}: {e}");
                }
                (original_version.clone(), false)
            }
        }
    }

    /// Refine a version for a given SHA (finds the most specific version tag).
    /// This is the REFINE operation: returns the best version tag for a SHA.
    /// Returns `Some(version)` if tags are found, `None` if not.
    pub fn refine_version(&self, id: &ActionId, sha: &CommitSha) -> Option<Version> {
        match self.registry.tags_for_sha(id, sha) {
            Ok(tags) => select_most_specific_tag(&tags),
            Err(_) => None,
        }
    }
}

/// Parse a version string (with optional 'v' prefix) into numeric components.
/// Returns `None` if any component is non-numeric.
fn parse_version_components(s: &str) -> Option<Vec<u64>> {
    let stripped = s.trim_start_matches('v');
    stripped.split('.').map(|p| p.parse::<u64>().ok()).collect()
}

/// Select the most specific version tag from a list.
/// Prefers semver-like tags with more components (patch over minor over major),
/// then highest version value among equal component counts, with non-semver tags last.
#[must_use]
pub fn select_most_specific_tag(tags: &[Version]) -> Option<Version> {
    if tags.is_empty() {
        return None;
    }

    let mut indexed: Vec<(&Version, Option<Vec<u64>>)> = tags
        .iter()
        .map(|t| (t, parse_version_components(t.as_str())))
        .collect();

    // Sort: semver-like tags first (more components preferred: v4.1.0 > v4.1 > v4),
    // then highest version value wins among equal component counts, non-semver tags last.
    indexed.sort_by(|(_, av), (_, bv)| match (av, bv) {
        (None, None) => std::cmp::Ordering::Equal,
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (Some(av), Some(bv)) => {
            let a_len = av.len();
            let b_len = bv.len();
            match b_len.cmp(&a_len) {
                std::cmp::Ordering::Equal => bv.cmp(av), // higher version wins (descending)
                other => other,                          // more components wins (descending)
            }
        }
    });

    indexed.first().map(|(t, _)| (*t).clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockRegistry {
        resolve_result: Result<ResolvedRef, ResolutionError>,
        tags_result: Result<Vec<Version>, ResolutionError>,
    }

    impl VersionRegistry for MockRegistry {
        fn lookup_sha(
            &self,
            _id: &ActionId,
            _version: &Version,
        ) -> Result<ResolvedRef, ResolutionError> {
            self.resolve_result.clone()
        }

        fn tags_for_sha(
            &self,
            _id: &ActionId,
            _sha: &CommitSha,
        ) -> Result<Vec<Version>, ResolutionError> {
            self.tags_result.clone()
        }

        fn all_tags(&self, _id: &ActionId) -> Result<Vec<Version>, ResolutionError> {
            self.tags_result.clone()
        }

        fn describe_sha(
            &self,
            _id: &ActionId,
            _sha: &CommitSha,
        ) -> Result<ShaDescription, ResolutionError> {
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
    fn test_resolve_success() {
        let mock_registry = MockRegistry {
            resolve_result: Ok(ResolvedRef::new(
                CommitSha::from("abc123def456789012345678901234567890abcd"),
                "actions/checkout".to_string(),
                RefType::Tag,
                "2026-01-01T00:00:00Z".to_string(),
            )),
            tags_result: Ok(vec![]),
        };
        let service = ActionResolver::new(mock_registry);

        let spec = ActionSpec::new(ActionId::from("actions/checkout"), Version::from("v4"));
        let result = service.resolve(&spec);

        let resolved = result.expect("Expected Ok result");
        assert_eq!(resolved.id.as_str(), "actions/checkout");
        assert_eq!(resolved.version.as_str(), "v4");
        assert_eq!(
            resolved.sha.as_str(),
            "abc123def456789012345678901234567890abcd"
        );
    }

    #[test]
    fn test_resolve_failure() {
        let registry = MockRegistry {
            resolve_result: Err(ResolutionError::ResolveFailed {
                spec: ActionSpec::new(ActionId::from("actions/checkout"), Version::from("v4")),
                reason: "not found".to_string(),
            }),
            tags_result: Ok(vec![]),
        };
        let service = ActionResolver::new(registry);

        let spec = ActionSpec::new(ActionId::from("actions/checkout"), Version::from("v4"));
        let result = service.resolve(&spec);

        assert!(result.is_err());
    }

    #[test]
    fn test_correct_version_no_correction_needed() {
        let registry = MockRegistry {
            resolve_result: Ok(ResolvedRef::new(
                CommitSha::from("abc123def456789012345678901234567890abcd"),
                "actions/checkout".to_string(),
                RefType::Tag,
                "2026-01-01T00:00:00Z".to_string(),
            )),
            tags_result: Ok(vec![Version::from("v4"), Version::from("v4.0.0")]),
        };
        let service = ActionResolver::new(registry);

        let id = ActionId::from("actions/checkout");
        let sha = CommitSha::from("abc123def456789012345678901234567890abcd");
        let original_version = Version::from("v4");
        let (version, was_corrected) = service.correct_version(&id, &sha, &original_version);

        assert_eq!(version.as_str(), "v4");
        assert!(!was_corrected);
    }

    #[test]
    fn test_correct_version_correction_needed() {
        let registry = MockRegistry {
            resolve_result: Ok(ResolvedRef::new(
                CommitSha::from("abc123def456789012345678901234567890abcd"),
                "actions/checkout".to_string(),
                RefType::Tag,
                "2026-01-01T00:00:00Z".to_string(),
            )),
            tags_result: Ok(vec![Version::from("v5"), Version::from("v5.0.0")]),
        };
        let service = ActionResolver::new(registry);

        let id = ActionId::from("actions/checkout");
        let sha = CommitSha::from("abc123def456789012345678901234567890abcd");
        let original_version = Version::from("v4");
        let (version, was_corrected) = service.correct_version(&id, &sha, &original_version);

        assert_eq!(version.as_str(), "v5.0.0");
        assert!(was_corrected);
    }

    #[test]
    fn test_resolve_from_sha_with_tags() {
        let sha = CommitSha::from("abc123def456789012345678901234567890abcd");
        let registry = MockRegistry {
            resolve_result: Ok(ResolvedRef::new(
                sha.clone(),
                "owner/repo".to_string(),
                RefType::Commit,
                "2026-01-01T00:00:00Z".to_string(),
            )),
            tags_result: Ok(vec![
                Version::from("v3"),
                Version::from("v3.6"),
                Version::from("v3.6.1"),
            ]),
        };
        let service = ActionResolver::new(registry);
        let id = ActionId::from("owner/repo");

        let result = service
            .resolve_from_sha(&id, &sha)
            .expect("Expected Ok result");

        assert_eq!(result.version.as_str(), "v3.6.1");
        assert_eq!(result.sha, sha);
        assert_eq!(result.ref_type, RefType::Tag);
        assert_eq!(result.repository, "owner/repo");
    }

    #[test]
    fn test_resolve_from_sha_no_tags() {
        let sha = CommitSha::from("abc123def456789012345678901234567890abcd");
        let registry = MockRegistry {
            resolve_result: Ok(ResolvedRef::new(
                sha.clone(),
                "owner/repo".to_string(),
                RefType::Commit,
                "2026-01-01T00:00:00Z".to_string(),
            )),
            tags_result: Ok(vec![]),
        };
        let service = ActionResolver::new(registry);
        let id = ActionId::from("owner/repo");

        let result = service
            .resolve_from_sha(&id, &sha)
            .expect("Expected Ok result");

        assert_eq!(result.version.as_str(), sha.as_str());
        assert_eq!(result.sha, sha);
        assert_eq!(result.ref_type, RefType::Commit);
    }

    #[test]
    fn test_resolve_from_sha_describe_error_propagates() {
        let registry = MockRegistry {
            resolve_result: Err(ResolutionError::TokenRequired),
            tags_result: Ok(vec![]),
        };
        let service = ActionResolver::new(registry);
        let id = ActionId::from("owner/repo");
        let sha = CommitSha::from("abc123def456789012345678901234567890abcd");

        let result = service.resolve_from_sha(&id, &sha);
        assert!(
            matches!(result, Err(ResolutionError::TokenRequired)),
            "describe_sha error should propagate through resolve_from_sha"
        );
    }

    #[test]
    fn test_select_most_specific_tag_empty() {
        assert_eq!(select_most_specific_tag(&[]), None);
    }

    #[test]
    fn test_select_most_specific_tag_single() {
        let tags = vec![Version::from("v4")];
        assert_eq!(select_most_specific_tag(&tags), Some(Version::from("v4")));
    }

    #[test]
    fn test_select_most_specific_tag_prefers_patch_over_major() {
        let tags = vec![Version::from("v4.1.0"), Version::from("v4")];
        assert_eq!(
            select_most_specific_tag(&tags),
            Some(Version::from("v4.1.0"))
        );
    }

    #[test]
    fn test_select_most_specific_tag_prefers_minor_over_major() {
        let tags = vec![Version::from("v4.1"), Version::from("v4")];
        assert_eq!(select_most_specific_tag(&tags), Some(Version::from("v4.1")));
    }

    #[test]
    fn test_select_most_specific_tag_three_tiers() {
        let tags = vec![
            Version::from("v3"),
            Version::from("v3.6"),
            Version::from("v3.6.1"),
        ];
        assert_eq!(
            select_most_specific_tag(&tags),
            Some(Version::from("v3.6.1"))
        );
    }

    #[test]
    fn test_select_most_specific_tag_non_semver_sorted_last() {
        let tags = vec![Version::from("latest"), Version::from("v4")];
        assert_eq!(select_most_specific_tag(&tags), Some(Version::from("v4")));
    }

    #[test]
    fn test_select_most_specific_tag_all_non_semver_returns_first() {
        let tags = vec![Version::from("latest"), Version::from("stable")];
        // No semver tags — returns the first one
        assert!(select_most_specific_tag(&tags).is_some());
    }

    #[test]
    fn test_select_most_specific_tag_higher_major_wins_among_same_precision() {
        let tags = vec![
            Version::from("v3"),
            Version::from("v4"),
            Version::from("v5"),
        ];
        assert_eq!(select_most_specific_tag(&tags), Some(Version::from("v5")));
    }
}
