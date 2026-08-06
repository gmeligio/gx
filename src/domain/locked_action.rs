use super::action::identity::{ActionId, CommitSha, Repository};
use super::action::resolved::{Commit, ResolvedRef};
use super::action::spec::Spec;
use super::action::specifier::Specifier;
use super::lock::LockEntry;

/// One managed dependency as recorded in the lock: a borrowed view over a
/// single lock row, carrying its [`Spec`] alongside the resolution it maps to.
///
/// The lock stores rows as a `HashMap<Spec, LockEntry>`, so the key lives
/// outside the value and a row is only a whole thought once the two are paired.
/// [`LockEntry`] is the storage counterpart — the shape the map owns and
/// mutates in place. `LockedAction` is the read view yielded by
/// [`Lock::entries`](super::lock::Lock::entries), and it is what a consumer
/// iterating the lock should name.
///
/// # Completeness
///
/// Accessors surface stored values verbatim and make no completeness
/// guarantee. The lock's loaded shape permits rows whose fields are empty, so
/// [`repository`](Self::repository) may return an empty string and
/// [`version_label`](Self::version_label) an empty label. A caller that needs a
/// guarantee asks [`Lock::is_complete`](super::lock::Lock::is_complete).
#[derive(Debug, Clone, Copy)]
pub struct LockedAction<'lock> {
    /// The lock key: action ID plus the specifier it is recorded under.
    spec: &'lock Spec,
    /// What the specifier resolved to — a tag, a branch, or a bare commit pin.
    reference: &'lock ResolvedRef,
    /// Commit metadata for the resolution.
    commit: &'lock Commit,
}

impl<'lock> LockedAction<'lock> {
    /// View a stored lock row as a managed dependency.
    pub(super) fn new(spec: &'lock Spec, entry: &'lock LockEntry) -> Self {
        Self {
            spec,
            reference: &entry.reference,
            commit: &entry.commit,
        }
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

    /// What the specifier resolved to.
    #[must_use]
    pub fn reference(&self) -> &'lock ResolvedRef {
        self.reference
    }

    /// The pinned commit SHA.
    #[must_use]
    pub fn sha(&self) -> &'lock CommitSha {
        &self.commit.sha
    }

    /// The repository the commit was resolved against, as stored.
    #[must_use]
    pub fn repository(&self) -> &'lock Repository {
        &self.commit.repository
    }

    /// The full commit metadata, for callers that persist every field.
    #[must_use]
    pub fn commit(&self) -> &'lock Commit {
        self.commit
    }

    /// The string recorded in the lock's `version` slot. A bare commit pin
    /// round-trips through its SHA (see [`ResolvedRef::label`]).
    #[must_use]
    pub fn version_label(&self) -> &'lock str {
        self.reference.label(&self.commit.sha)
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
        assert_eq!(locked.repository().as_str(), "actions/checkout");
        assert_eq!(locked.reference(), &entry.reference);
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
    fn accessors_surface_an_incomplete_row_verbatim() {
        // The loaded shape permits empty fields; the view reports them as
        // stored rather than hiding or rejecting them.
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

        assert!(
            LockedAction::new(&spec, &entry)
                .repository()
                .as_str()
                .is_empty()
        );
    }
}
