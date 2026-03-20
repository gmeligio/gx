use super::identity::{ActionId, CommitDate, CommitSha, Repository, Version};
use super::uses_ref::RefType;

/// Commit metadata for a resolved action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Commit {
    pub sha: CommitSha,
    pub repository: Repository,
    pub ref_type: Option<RefType>,
    pub date: CommitDate,
}

/// The result of resolving an action spec via the registry.
///
/// Contains only the discovered data — the `Spec` (id + specifier) is already
/// known by the caller and not duplicated here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    pub version: Version,
    pub commit: Commit,
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
    use super::{Commit, CommitDate, CommitSha, RefType, Repository, Resolved, Version};

    #[test]
    fn resolved_holds_version_and_commit() {
        let resolved = Resolved {
            version: Version::from("v4.2.1"),
            commit: Commit {
                sha: CommitSha::from("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
                repository: Repository::from("actions/checkout"),
                ref_type: Some(RefType::Tag),
                date: CommitDate::from("2026-01-01T00:00:00Z"),
            },
        };
        assert_eq!(resolved.version.as_str(), "v4.2.1");
        assert_eq!(
            resolved.commit.sha,
            CommitSha::from("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
        );
    }
}
