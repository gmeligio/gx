use log::{debug, info, warn};
use thiserror::Error;

use super::{ActionId, ActionSpec, CommitSha, ResolvedAction, Version};

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

/// Trait for querying available versions and commit SHAs from a remote registry
pub trait VersionRegistry {
    /// Look up the commit SHA for a version reference
    ///
    /// # Errors
    ///
    /// Returns an error if the lookup fails
    fn lookup_sha(&self, id: &ActionId, version: &Version) -> Result<CommitSha, ResolutionError>;

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
            Ok(sha) => {
                let resolved = ResolvedAction::new(spec.id.clone(), spec.version.clone(), sha);
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
                    );
                    ResolutionResult::Resolved(resolved)
                } else if let Some(correct_version) = select_best_tag(&tags) {
                    // Version comment doesn't match SHA - use the correct version
                    info!(
                        "Corrected {spec} version to {correct_version} (SHA {workflow_sha} points to {correct_version})",
                    );

                    let corrected =
                        ResolvedAction::new(spec.id.clone(), correct_version, workflow_sha.clone());
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
                );
                ResolutionResult::Resolved(resolved)
            }
        }
    }
}

/// Select the best tag from a list (prefers shorter semver-like tags)
fn select_best_tag(tags: &[Version]) -> Option<Version> {
    if tags.is_empty() {
        return None;
    }

    // Convert to string refs for sorting
    let tag_strs: Vec<&str> = tags.iter().map(Version::as_str).collect();

    // Prefer tags that look like semver (v1, v1.2, v1.2.3)
    // Sort by: semver-like first, then by length (shorter is better for major version tags)
    let mut sorted_tags: Vec<&str> = tag_strs;
    sorted_tags.sort_by(|a, b| {
        let a_is_semver =
            a.starts_with('v') && a.chars().nth(1).is_some_and(|c| c.is_ascii_digit());
        let b_is_semver =
            b.starts_with('v') && b.chars().nth(1).is_some_and(|c| c.is_ascii_digit());

        match (a_is_semver, b_is_semver) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.len().cmp(&b.len()),
        }
    });

    sorted_tags.first().map(|s| Version::from(*s))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockRegistry {
        resolve_result: Result<CommitSha, ResolutionError>,
        tags_result: Result<Vec<Version>, ResolutionError>,
    }

    impl VersionRegistry for MockRegistry {
        fn lookup_sha(
            &self,
            _id: &ActionId,
            _version: &Version,
        ) -> Result<CommitSha, ResolutionError> {
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
            resolve_result: Ok(CommitSha::from("abc123def456789012345678901234567890abcd")),
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
            resolve_result: Ok(CommitSha::from("abc123")),
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
            resolve_result: Ok(CommitSha::from("abc123")),
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
}
