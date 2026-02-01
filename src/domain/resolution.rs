use log::{debug, info, warn};
use thiserror::Error;

use super::version::{is_commit_sha, is_semver_like};
use super::{ActionId, ActionSpec, CommitSha, ResolvedAction, Version, find_highest_version};

/// Errors that can occur during version resolution
#[derive(Debug, Clone, Error)]
pub enum ResolutionError {
    #[error("failed to resolve {action}@{version}: {reason}")]
    ResolveFailed {
        action: String,
        version: String,
        reason: String,
    },

    #[error("no tags found for {action} at SHA {sha}")]
    NoTagsForSha { action: String, sha: String },

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

/// Trait for resolving version references to commit SHAs
pub trait VersionResolver {
    /// Resolve a version reference to a commit SHA
    ///
    /// # Errors
    ///
    /// Returns an error if resolution fails
    fn resolve(&self, id: &ActionId, version: &Version) -> Result<CommitSha, ResolutionError>;

    /// Get all tags that point to a specific SHA
    ///
    /// # Errors
    ///
    /// Returns an error if the lookup fails
    fn tags_for_sha(&self, id: &ActionId, sha: &CommitSha)
    -> Result<Vec<Version>, ResolutionError>;
}

/// Service for resolving action versions to commit SHAs
pub struct ResolutionService<R: VersionResolver> {
    resolver: R,
}

impl<R: VersionResolver> ResolutionService<R> {
    #[must_use]
    pub fn new(resolver: R) -> Self {
        Self { resolver }
    }

    /// Resolve an action spec to a commit SHA
    pub fn resolve(&self, spec: &ActionSpec) -> ResolutionResult {
        debug!("Resolving {}@{}", spec.id, spec.version);

        match self.resolver.resolve(&spec.id, &spec.version) {
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
        match self.resolver.tags_for_sha(&spec.id, workflow_sha) {
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
                        "Corrected {} version: {} -> {} (SHA {} points to {})",
                        spec.id, spec.version, correct_version, workflow_sha, correct_version
                    );

                    let corrected =
                        ResolvedAction::new(spec.id.clone(), correct_version, workflow_sha.clone());
                    ResolutionResult::Corrected {
                        original: spec.clone(),
                        corrected,
                    }
                } else {
                    warn!(
                        "No tags found for {} SHA {}, keeping version {}",
                        spec.id, workflow_sha, spec.version
                    );
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
                        "GITHUB_TOKEN not set. Without it, cannot validate for {} that {} commit SHA matches the {} version.",
                        spec.id, workflow_sha, spec.version
                    );
                } else {
                    warn!(
                        "For {} action could not validate {} commit SHA: {}",
                        spec.id, workflow_sha, e
                    );
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

/// Select the best version from a list of versions.
/// Prefers the highest semantic version if available.
#[must_use]
pub fn select_highest_version(versions: &[Version]) -> Option<Version> {
    let version_refs: Vec<&str> = versions.iter().map(Version::as_str).collect();
    find_highest_version(&version_refs).map(Version::from)
}

/// Determines if a manifest version should be updated based on workflow version.
///
/// Rule: Only update if manifest has a SHA and workflow has a semantic version tag.
/// This handles the case where someone upgraded from SHA to semver via comment.
#[must_use]
pub fn should_update_manifest(manifest_version: &Version, workflow_version: &Version) -> bool {
    manifest_version != workflow_version
        && is_commit_sha(manifest_version.as_str())
        && is_semver_like(workflow_version.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockResolver {
        resolve_result: Result<CommitSha, ResolutionError>,
        tags_result: Result<Vec<Version>, ResolutionError>,
    }

    impl VersionResolver for MockResolver {
        fn resolve(
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
    }

    #[test]
    fn test_resolve_success() {
        let mock_resolver = MockResolver {
            resolve_result: Ok(CommitSha::from("abc123def456789012345678901234567890abcd")),
            tags_result: Ok(vec![]),
        };
        let service = ResolutionService::new(mock_resolver);

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
        let resolver = MockResolver {
            resolve_result: Err(ResolutionError::ResolveFailed {
                action: "actions/checkout".to_string(),
                version: "v4".to_string(),
                reason: "not found".to_string(),
            }),
            tags_result: Ok(vec![]),
        };
        let service = ResolutionService::new(resolver);

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
        let mock_resolver = MockResolver {
            resolve_result: Ok(CommitSha::from("abc123")),
            tags_result: Ok(vec![Version::from("v4"), Version::from("v4.0.0")]),
        };
        let service = ResolutionService::new(mock_resolver);

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
        let resolver = MockResolver {
            resolve_result: Ok(CommitSha::from("abc123")),
            tags_result: Ok(vec![Version::from("v5"), Version::from("v5.0.0")]),
        };
        let service = ResolutionService::new(resolver);

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
    fn test_select_highest_version() {
        let versions = vec![
            Version::from("v3"),
            Version::from("v4"),
            Version::from("v2"),
        ];
        let result = select_highest_version(&versions);
        assert_eq!(result.map(|v| v.0), Some("v4".to_string()));
    }

    #[test]
    fn test_select_highest_version_empty() {
        let versions: Vec<Version> = vec![];
        let result = select_highest_version(&versions);
        assert!(result.is_none());
    }

    #[test]
    fn test_should_update_manifest_sha_to_semver() {
        let manifest = Version::from("abc123def456789012345678901234567890abcd");
        let workflow = Version::from("v4");
        assert!(should_update_manifest(&manifest, &workflow));
    }

    #[test]
    fn test_should_update_manifest_same_version() {
        let manifest = Version::from("v4");
        let workflow = Version::from("v4");
        assert!(!should_update_manifest(&manifest, &workflow));
    }

    #[test]
    fn test_should_update_manifest_semver_to_semver() {
        let manifest = Version::from("v3");
        let workflow = Version::from("v4");
        // Don't update if manifest already has semver
        assert!(!should_update_manifest(&manifest, &workflow));
    }

    #[test]
    fn test_should_update_manifest_sha_to_sha() {
        let manifest = Version::from("abc123def456789012345678901234567890abcd");
        let workflow = Version::from("def456789012345678901234567890abcd1234");
        // Don't update if workflow also has SHA (not semver)
        assert!(!should_update_manifest(&manifest, &workflow));
    }

    #[test]
    fn test_should_update_manifest_sha_to_branch() {
        let manifest = Version::from("abc123def456789012345678901234567890abcd");
        let workflow = Version::from("main");
        // Don't update if workflow has branch name (not semver)
        assert!(!should_update_manifest(&manifest, &workflow));
    }
}
