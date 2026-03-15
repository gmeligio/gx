use super::identity::{ActionId, CommitDate, CommitSha, Repository};
use super::specifier::Specifier;
use super::uses_ref::RefType;

/// Commit metadata shared between `Resolved` and lock `Entry`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Commit {
    pub sha: CommitSha,
    pub repository: Repository,
    pub ref_type: Option<RefType>,
    pub date: CommitDate,
}

/// A fully resolved action with its commit SHA and metadata.
/// The `version` field holds the manifest specifier (e.g., `"^6"`).
/// The resolved tag (e.g., `"v6.0.2"`) is stored in the lock entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    pub id: ActionId,
    pub version: Specifier,
    pub commit: Commit,
}

impl Resolved {
    /// Create a new resolved action with all metadata.
    #[must_use]
    pub fn new(
        id: ActionId,
        version: Specifier,
        sha: CommitSha,
        repository: Repository,
        ref_type: Option<RefType>,
        date: CommitDate,
    ) -> Self {
        Self {
            id,
            version,
            commit: Commit {
                sha,
                repository,
                ref_type,
                date,
            },
        }
    }

    /// Format as "SHA # comment" for workflow updates.
    /// The comment is derived from the specifier (e.g., `"^6"` → `"v6"`).
    #[must_use]
    pub fn to_workflow_ref(&self) -> String {
        format!("{} # {}", self.commit.sha, self.version.to_comment())
    }

    /// Create a new `Resolved` with the SHA replaced.
    /// Used when a workflow has a pinned SHA that differs from the registry.
    #[must_use]
    pub fn with_sha(self, sha: CommitSha) -> Self {
        Self {
            commit: Commit { sha, ..self.commit },
            ..self
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ActionId, CommitDate, CommitSha, RefType, Repository, Resolved, Specifier};

    #[test]
    fn resolved_action_to_workflow_ref() {
        let resolved = Resolved::new(
            ActionId::from("actions/checkout"),
            Specifier::parse("^4"),
            CommitSha::from("abc123def456789012345678901234567890abcd"),
            Repository::from("actions/checkout"),
            Some(RefType::Tag),
            CommitDate::from("2026-01-01T00:00:00Z"),
        );
        assert_eq!(
            resolved.to_workflow_ref(),
            "abc123def456789012345678901234567890abcd # v4"
        );
    }

    #[test]
    fn with_sha_replaces_only_sha() {
        let resolved = Resolved::new(
            ActionId::from("actions/checkout"),
            Specifier::parse("^4"),
            CommitSha::from("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
            Repository::from("actions/checkout"),
            Some(RefType::Tag),
            CommitDate::from("2026-01-01T00:00:00Z"),
        );
        let updated =
            resolved.with_sha(CommitSha::from("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"));
        assert_eq!(
            updated.commit.sha,
            CommitSha::from("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb")
        );
        assert_eq!(updated.commit.repository.as_str(), "actions/checkout");
        assert_eq!(updated.id.as_str(), "actions/checkout");
    }
}
