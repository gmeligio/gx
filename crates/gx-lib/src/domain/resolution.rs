use log::{debug, info, warn};
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

/// Result of attempting to resolve an action version
#[derive(Debug)]
pub enum ResolutionResult {
    /// Successfully resolved to a SHA
    Resolved(ResolvedAction),
    /// Version was corrected based on SHA lookup
    Corrected {
        original: ActionSpec,
        corrected: ResolvedAction,
    },
    /// Could not resolve, with reason
    Unresolved { spec: ActionSpec, reason: String },
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

    /// Resolve an action spec to a commit SHA
    pub fn resolve(&self, spec: &ActionSpec) -> ResolutionResult {
        debug!("Resolving {spec}");

        match self.registry.lookup_sha(&spec.id, &spec.version) {
            Ok(resolved_ref) => {
                let resolved = ResolvedAction::new(
                    spec.id.clone(),
                    spec.version.clone(),
                    resolved_ref.sha,
                    resolved_ref.repository,
                    resolved_ref.ref_type,
                    resolved_ref.date,
                );
                ResolutionResult::Resolved(resolved)
            }
            Err(e) => ResolutionResult::Unresolved {
                spec: spec.clone(),
                reason: e.to_string(),
            },
        }
    }

    /// Validate that a SHA matches the expected version, and correct if needed
    pub fn validate_and_correct(
        &self,
        spec: &ActionSpec,
        workflow_sha: &CommitSha,
    ) -> ResolutionResult {
        match self.registry.tags_for_sha(&spec.id, workflow_sha) {
            Ok(tags) => {
                // Check if the version matches any tag
                if tags.iter().any(|t| t == &spec.version) {
                    // Version matches, use as-is
                    let resolved = ResolvedAction::new(
                        spec.id.clone(),
                        spec.version.clone(),
                        workflow_sha.clone(),
                        spec.id.base_repo(),
                        RefType::Commit,
                        String::new(),
                    );
                    ResolutionResult::Resolved(resolved)
                } else if let Some(correct_version) = select_best_tag(&tags) {
                    // Version comment doesn't match SHA - use the correct version
                    info!(
                        "Corrected {spec} version to {correct_version} (SHA {workflow_sha} points to {correct_version})",
                    );

                    let corrected = ResolvedAction::new(
                        spec.id.clone(),
                        correct_version,
                        workflow_sha.clone(),
                        spec.id.base_repo(),
                        RefType::Commit,
                        String::new(),
                    );
                    ResolutionResult::Corrected {
                        original: spec.clone(),
                        corrected,
                    }
                } else {
                    warn!("No tags found for {spec} SHA {workflow_sha}, keeping version");
                    // No tags found, keep original version
                    let resolved = ResolvedAction::new(
                        spec.id.clone(),
                        spec.version.clone(),
                        workflow_sha.clone(),
                        spec.id.base_repo(),
                        RefType::Commit,
                        String::new(),
                    );
                    ResolutionResult::Resolved(resolved)
                }
            }
            Err(e) => {
                // Log warning and continue
                if matches!(e, ResolutionError::TokenRequired) {
                    warn!(
                        "GITHUB_TOKEN not set. Without it, cannot validate for {spec} that {workflow_sha} commit SHA matches the version.",
                    );
                } else {
                    warn!("For {spec} could not validate {workflow_sha} commit SHA: {e}");
                }
                // Return as resolved with original version
                let resolved = ResolvedAction::new(
                    spec.id.clone(),
                    spec.version.clone(),
                    workflow_sha.clone(),
                    spec.id.base_repo(),
                    RefType::Commit,
                    String::new(),
                );
                ResolutionResult::Resolved(resolved)
            }
        }
    }
}

/// Parse a version string (with optional 'v' prefix) into numeric components.
/// Returns `None` if any component is non-numeric.
fn parse_version_components(s: &str) -> Option<Vec<u64>> {
    let stripped = s.trim_start_matches('v');
    stripped.split('.').map(|p| p.parse::<u64>().ok()).collect()
}

/// Select the best tag from a list (prefers semver-like tags with fewer components,
/// then highest version value among equal component counts, non-semver tags last).
fn select_best_tag(tags: &[Version]) -> Option<Version> {
    if tags.is_empty() {
        return None;
    }

    let mut indexed: Vec<(&Version, Option<Vec<u64>>)> = tags
        .iter()
        .map(|t| (t, parse_version_components(t.as_str())))
        .collect();

    // Sort: semver-like tags first (fewer components preferred: v4 < v4.1 < v4.1.0),
    // then highest version value wins among equal component counts, non-semver tags last.
    indexed.sort_by(|(_, av), (_, bv)| match (av, bv) {
        (None, None) => std::cmp::Ordering::Equal,
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (Some(av), Some(bv)) => {
            let a_len = av.len();
            let b_len = bv.len();
            match a_len.cmp(&b_len) {
                std::cmp::Ordering::Equal => bv.cmp(av), // higher version wins (descending)
                other => other,                          // fewer components wins (ascending)
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

        match result {
            ResolutionResult::Resolved(resolved) => {
                assert_eq!(resolved.id.as_str(), "actions/checkout");
                assert_eq!(resolved.version.as_str(), "v4");
                assert_eq!(
                    resolved.sha.as_str(),
                    "abc123def456789012345678901234567890abcd"
                );
            }
            _ => panic!("Expected Resolved result"),
        }
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

        match result {
            ResolutionResult::Unresolved { spec: s, reason: _ } => {
                assert_eq!(s.id.as_str(), "actions/checkout");
            }
            _ => panic!("Expected Unresolved result"),
        }
    }

    #[test]
    fn test_validate_version_matches() {
        let mock_registry = MockRegistry {
            resolve_result: Ok(ResolvedRef::new(
                CommitSha::from("abc123def456789012345678901234567890abcd"),
                "actions/checkout".to_string(),
                RefType::Tag,
                "2026-01-01T00:00:00Z".to_string(),
            )),
            tags_result: Ok(vec![Version::from("v4"), Version::from("v4.0.0")]),
        };
        let service = ActionResolver::new(mock_registry);

        let spec = ActionSpec::new(ActionId::from("actions/checkout"), Version::from("v4"));
        let sha = CommitSha::from("abc123def456789012345678901234567890abcd");
        let result = service.validate_and_correct(&spec, &sha);

        match result {
            ResolutionResult::Resolved(resolved) => {
                assert_eq!(resolved.version.as_str(), "v4");
            }
            _ => panic!("Expected Resolved result"),
        }
    }

    #[test]
    fn test_validate_version_corrected() {
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

        let spec = ActionSpec::new(ActionId::from("actions/checkout"), Version::from("v4"));
        let sha = CommitSha::from("abc123def456789012345678901234567890abcd");
        let result = service.validate_and_correct(&spec, &sha);

        match result {
            ResolutionResult::Corrected {
                original,
                corrected,
            } => {
                assert_eq!(original.version.as_str(), "v4");
                assert_eq!(corrected.version.as_str(), "v5");
            }
            _ => panic!("Expected Corrected result"),
        }
    }

    #[test]
    fn test_select_best_tag_empty() {
        assert_eq!(select_best_tag(&[]), None);
    }

    #[test]
    fn test_select_best_tag_single() {
        let tags = vec![Version::from("v4")];
        assert_eq!(select_best_tag(&tags), Some(Version::from("v4")));
    }

    #[test]
    fn test_select_best_tag_prefers_major_over_patch() {
        let tags = vec![Version::from("v4.1.0"), Version::from("v4")];
        assert_eq!(select_best_tag(&tags), Some(Version::from("v4")));
    }

    #[test]
    fn test_select_best_tag_prefers_major_over_minor() {
        let tags = vec![Version::from("v4.1"), Version::from("v4")];
        assert_eq!(select_best_tag(&tags), Some(Version::from("v4")));
    }

    #[test]
    fn test_select_best_tag_non_semver_sorted_last() {
        let tags = vec![Version::from("latest"), Version::from("v4")];
        assert_eq!(select_best_tag(&tags), Some(Version::from("v4")));
    }

    #[test]
    fn test_select_best_tag_all_non_semver_returns_first() {
        let tags = vec![Version::from("latest"), Version::from("stable")];
        // No semver tags — returns the first one
        assert!(select_best_tag(&tags).is_some());
    }

    #[test]
    fn test_select_best_tag_higher_major_wins_among_same_precision() {
        let tags = vec![
            Version::from("v3"),
            Version::from("v4"),
            Version::from("v5"),
        ];
        assert_eq!(select_best_tag(&tags), Some(Version::from("v5")));
    }
}
