use super::identity::{ActionId, CommitDate, CommitSha, Repository, Version};
use super::specifier::Specifier;
use super::uses_ref::RefType;

/// Commit metadata shared between `RegistryResolution` and lock `Entry`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Commit {
    pub sha: CommitSha,
    pub repository: Repository,
    pub ref_type: Option<RefType>,
    pub date: CommitDate,
}

/// A registry resolution: the result of looking up an action specifier in the registry.
/// The `specifier` field holds the manifest specifier (e.g., `"^6"`).
/// The resolved tag (e.g., `"v6.0.2"`) is stored in the lock entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistryResolution {
    pub id: ActionId,
    pub specifier: Specifier,
    pub commit: Commit,
}

impl RegistryResolution {
    /// Create a new registry resolution with all metadata.
    #[must_use]
    pub fn new(
        id: ActionId,
        specifier: Specifier,
        sha: CommitSha,
        repository: Repository,
        ref_type: Option<RefType>,
        date: CommitDate,
    ) -> Self {
        Self {
            id,
            specifier,
            commit: Commit {
                sha,
                repository,
                ref_type,
                date,
            },
        }
    }

    /// Create a new `RegistryResolution` with the SHA replaced.
    /// Used when a workflow has a pinned SHA that differs from the registry.
    #[must_use]
    pub fn with_sha(self, sha: CommitSha) -> Self {
        Self {
            commit: Commit { sha, ..self.commit },
            ..self
        }
    }
}

/// A resolved action ready for workflow output.
///
/// This is the domain representation of "what goes into the workflow file":
/// the action ID, its pinned SHA, and an optional version annotation.
/// `version` is `None` for bare SHA specifiers (no `# comment` needed).
#[derive(Debug, Clone)]
#[expect(
    clippy::module_name_repetitions,
    reason = "ResolvedAction is the canonical domain name for workflow-output actions"
)]
pub struct ResolvedAction {
    pub id: ActionId,
    pub sha: CommitSha,
    pub version: Option<Version>,
}

#[cfg(test)]
mod tests {
    use super::{
        ActionId, CommitDate, CommitSha, RefType, RegistryResolution, Repository, Specifier,
    };

    #[test]
    fn with_sha_replaces_only_sha() {
        let resolved = RegistryResolution::new(
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
