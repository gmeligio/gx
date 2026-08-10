use super::action::identity::{ActionId, CommitSha, Repository};
use super::action::resolved::Commit;
use super::action::spec::Spec;
use super::action::specifier::Specifier;
use super::lock::LockEntry;

/// One action as locked: a row's key paired with its value.
///
/// Read a row through this; write one through [`LockEntry`], which is what the
/// map stores.
#[derive(Debug, Clone, Copy)]
pub struct LockedAction<'lock> {
    /// The lock key: action ID plus the specifier it is recorded under.
    spec: &'lock Spec,
    /// The stored value this row maps to.
    entry: &'lock LockEntry,
}

impl<'lock> LockedAction<'lock> {
    /// View a stored lock row as a managed dependency.
    pub(super) fn new(spec: &'lock Spec, entry: &'lock LockEntry) -> Self {
        Self { spec, entry }
    }

    /// The action this row locks, e.g. `actions/checkout`.
    #[must_use]
    pub fn id(&self) -> &'lock ActionId {
        &self.spec.id
    }

    /// The specifier this row is keyed under, e.g. `^4`.
    #[must_use]
    pub fn specifier(&self) -> &'lock Specifier {
        &self.spec.specifier
    }

    /// The pinned commit SHA.
    #[must_use]
    pub fn sha(&self) -> &'lock CommitSha {
        &self.entry.commit.sha
    }

    /// The repository the commit was resolved against.
    ///
    /// `None` for a row that stored none, so a check building
    /// `GET /repos/{owner}/{repo}` can't request a malformed URL and report clean.
    #[must_use]
    pub fn repository(&self) -> Option<&'lock Repository> {
        let repository = &self.entry.commit.repository;
        (!repository.as_str().is_empty()).then_some(repository)
    }

    /// The full commit metadata.
    #[must_use]
    pub fn commit(&self) -> &'lock Commit {
        &self.entry.commit
    }

    /// The string recorded in the lock's `version` slot.
    #[must_use]
    pub fn version_label(&self) -> &'lock str {
        self.entry.version_label()
    }
}

#[cfg(test)]
mod tests {
    use super::{LockEntry, LockedAction};
    use crate::domain::action::identity::{ActionId, CommitDate, CommitSha, Repository, Version};
    use crate::domain::action::resolved::{Commit, ResolvedRef};
    use crate::domain::action::spec::Spec;
    use crate::domain::action::specifier::Specifier;
    use crate::domain::action::uses_ref::RefType;

    const SHA: &str = "abc123def456789012345678901234567890abcd";

    fn entry(reference: ResolvedRef, ref_type: RefType) -> LockEntry {
        LockEntry {
            reference,
            commit: Commit {
                sha: CommitSha::from(SHA),
                repository: Repository::from("actions/checkout"),
                ref_type: Some(ref_type),
                date: CommitDate::from("2026-01-01T00:00:00Z"),
            },
        }
    }

    #[test]
    fn accessors_match_the_viewed_row() {
        let spec = Spec::new(ActionId::from("actions/checkout"), Specifier::parse("^4"));
        let entry = entry(ResolvedRef::Tag(Version::from("v4.2.1")), RefType::Tag);
        let locked = LockedAction::new(&spec, &entry);

        assert_eq!(locked.id().as_str(), "actions/checkout");
        assert_eq!(locked.specifier().as_str(), "^4");
        assert_eq!(locked.sha().as_str(), SHA);
        assert_eq!(
            locked.repository().map(Repository::as_str),
            Some("actions/checkout")
        );
    }

    #[test]
    fn version_label_is_the_tag_for_a_tag_pin() {
        let spec = Spec::new(ActionId::from("actions/checkout"), Specifier::parse("^4"));
        let entry = entry(ResolvedRef::Tag(Version::from("v4.2.1")), RefType::Tag);

        assert_eq!(LockedAction::new(&spec, &entry).version_label(), "v4.2.1");
    }

    #[test]
    fn version_label_is_the_sha_for_a_bare_commit_pin() {
        let spec = Spec::new(ActionId::from("actions/checkout"), Specifier::parse(SHA));
        let entry = entry(ResolvedRef::Commit, RefType::Commit);

        assert_eq!(LockedAction::new(&spec, &entry).version_label(), SHA);
    }

    #[test]
    fn repository_is_none_when_the_row_never_stored_one() {
        let spec = Spec::new(ActionId::from("actions/checkout"), Specifier::parse("^4"));
        let entry = LockEntry {
            reference: ResolvedRef::Tag(Version::from("v4.2.1")),
            commit: Commit {
                sha: CommitSha::from(SHA),
                repository: Repository::from(""),
                ref_type: None,
                date: CommitDate::from(""),
            },
        };

        assert_eq!(LockedAction::new(&spec, &entry).repository(), None);
    }
}
