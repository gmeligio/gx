use super::identity::{ActionId, CommitSha, Version};
use super::spec::LockKey;
use super::uses_ref::RefType;
use std::fmt;

/// A fully resolved action with its commit SHA and metadata
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedAction {
    pub id: ActionId,
    pub version: Version,
    pub sha: CommitSha,
    pub repository: String,
    pub ref_type: RefType,
    pub date: String,
}

impl ResolvedAction {
    /// Create a new resolved action with all metadata.
    #[must_use]
    pub fn new(
        id: ActionId,
        version: Version,
        sha: CommitSha,
        repository: String,
        ref_type: RefType,
        date: String,
    ) -> Self {
        Self {
            id,
            version,
            sha,
            repository,
            ref_type,
            date,
        }
    }

    /// Format as "SHA # version" for workflow updates
    #[must_use]
    pub fn to_workflow_ref(&self) -> String {
        format!("{} # {}", self.sha, self.version)
    }

    /// Create a new `ResolvedAction` with the SHA replaced.
    /// Used when a workflow has a pinned SHA that differs from the registry.
    #[must_use]
    pub fn with_sha(&self, sha: CommitSha) -> Self {
        Self {
            id: self.id.clone(),
            version: self.version.clone(),
            sha,
            repository: self.repository.clone(),
            ref_type: self.ref_type.clone(),
            date: self.date.clone(),
        }
    }
}

/// Tracks a version correction when SHA doesn't match the version comment
#[derive(Debug)]
pub struct VersionCorrection {
    pub action: ActionId,
    pub old_version: Version,
    pub new_version: Version,
    pub sha: CommitSha,
}

impl fmt::Display for VersionCorrection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} {} -> {} (SHA {} points to {})",
            self.action, self.old_version, self.new_version, self.sha, self.new_version
        )
    }
}

impl From<&ResolvedAction> for LockKey {
    fn from(resolved: &ResolvedAction) -> Self {
        Self::new(
            ActionId::from(resolved.id.as_str()),
            Version::from(resolved.version.as_str()),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolved_action_to_workflow_ref() {
        let resolved = ResolvedAction::new(
            ActionId::from("actions/checkout"),
            Version::from("v4"),
            CommitSha::from("abc123def456789012345678901234567890abcd"),
            "actions/checkout".to_string(),
            RefType::Tag,
            "2026-01-01T00:00:00Z".to_string(),
        );
        assert_eq!(
            resolved.to_workflow_ref(),
            "abc123def456789012345678901234567890abcd # v4"
        );
    }
}
