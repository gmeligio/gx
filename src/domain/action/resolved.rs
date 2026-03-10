use super::identity::{ActionId, CommitSha, Specifier, Version};
use super::spec::LockKey;
use super::uses_ref::RefType;
use std::fmt;

/// A fully resolved action with its commit SHA and metadata.
/// The `version` field holds the manifest specifier (e.g., `"^6"`).
/// The resolved tag (e.g., `"v6.0.2"`) is stored in the lock entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedAction {
    pub id: ActionId,
    pub version: Specifier,
    pub sha: CommitSha,
    pub repository: String,
    pub ref_type: Option<RefType>,
    pub date: String,
}

impl ResolvedAction {
    /// Create a new resolved action with all metadata.
    #[must_use]
    pub fn new(
        id: ActionId,
        version: Specifier,
        sha: CommitSha,
        repository: String,
        ref_type: Option<RefType>,
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

    /// Format as "SHA # comment" for workflow updates.
    /// The comment is derived from the specifier (e.g., `"^6"` → `"v6"`).
    #[must_use]
    pub fn to_workflow_ref(&self) -> String {
        format!("{} # {}", self.sha, self.version.to_comment())
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
            resolved.version.clone(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{ActionId, CommitSha, RefType, ResolvedAction, Specifier};

    #[test]
    fn test_resolved_action_to_workflow_ref() {
        let resolved = ResolvedAction::new(
            ActionId::from("actions/checkout"),
            Specifier::parse("^4"),
            CommitSha::from("abc123def456789012345678901234567890abcd"),
            "actions/checkout".to_string(),
            Some(RefType::Tag),
            "2026-01-01T00:00:00Z".to_string(),
        );
        assert_eq!(
            resolved.to_workflow_ref(),
            "abc123def456789012345678901234567890abcd # v4"
        );
    }
}
