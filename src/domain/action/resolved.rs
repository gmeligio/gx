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

/// What a resolved action's reference actually is.
///
/// This encodes the *kind* of a resolved ref at construction, so consumers never
/// re-derive it from a stringly-typed [`Version`] (plus a sibling `ref_type`).
/// Only a [`ResolvedRef::Tag`] can be constrained by a semver range; a branch or
/// a bare commit pin has no version a range could apply to.
#[derive(Debug, Clone, PartialEq, Eq)]
#[expect(
    clippy::module_name_repetitions,
    reason = "ResolvedRef is the canonical domain name for a resolved reference"
)]
pub enum ResolvedRef {
    /// A semver tag (e.g. `"v6.0.2"`) — the only case a range can constrain.
    Tag(Version),
    /// A branch ref (e.g. `"main"`) — no range applies.
    Branch(Version),
    /// A bare commit SHA with no version label — range inapplicable by construction.
    Commit,
}

impl ResolvedRef {
    /// Reconstruct a `ResolvedRef` from a persisted version string and ref type.
    ///
    /// This is the single boundary where the on-disk lock representation
    /// (`version` string + `ref_type`) is turned back into a typed reference.
    #[must_use]
    pub fn from_stored(version: Version, ref_type: Option<&RefType>) -> Self {
        match ref_type {
            Some(RefType::Commit) => Self::Commit,
            Some(RefType::Branch) => Self::Branch(version),
            // Tag, Release, or unknown legacy values are all real version tags.
            _ => Self::Tag(version),
        }
    }

    /// The tag this reference constrains, if any. Only a [`ResolvedRef::Tag`]
    /// yields a version a range can be checked against.
    #[must_use]
    pub fn tag(&self) -> Option<&Version> {
        match self {
            Self::Tag(v) => Some(v),
            Self::Branch(_) | Self::Commit => None,
        }
    }

    /// The `# comment` annotation for workflow output.
    ///
    /// `Tag`/`Branch` carry a human-readable version; a bare `Commit` has none.
    #[must_use]
    pub fn annotation(&self) -> Option<&Version> {
        match self {
            Self::Tag(v) | Self::Branch(v) => Some(v),
            Self::Commit => None,
        }
    }

    /// The string written to the lock file's `version` slot.
    ///
    /// A `Commit` has no version label, so it round-trips through its SHA —
    /// preserving the existing on-disk format exactly.
    #[must_use]
    pub fn label<'ref_>(&'ref_ self, sha: &'ref_ CommitSha) -> &'ref_ str {
        match self {
            Self::Tag(v) | Self::Branch(v) => v.as_str(),
            Self::Commit => sha.as_str(),
        }
    }
}

/// The result of resolving an action spec via the registry.
///
/// Contains only the discovered data — the `Spec` (id + specifier) is already
/// known by the caller and not duplicated here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    pub reference: ResolvedRef,
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
    use super::{
        Commit, CommitDate, CommitSha, RefType, Repository, Resolved, ResolvedRef, Version,
    };

    #[test]
    fn resolved_holds_reference_and_commit() {
        let resolved = Resolved {
            reference: ResolvedRef::Tag(Version::from("v4.2.1")),
            commit: Commit {
                sha: CommitSha::from("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
                repository: Repository::from("actions/checkout"),
                ref_type: Some(RefType::Tag),
                date: CommitDate::from("2026-01-01T00:00:00Z"),
            },
        };
        assert_eq!(
            resolved.reference.tag().map(Version::as_str),
            Some("v4.2.1")
        );
        assert_eq!(
            resolved.commit.sha,
            CommitSha::from("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
        );
    }

    #[test]
    fn only_tag_is_range_constrainable() {
        // A range can only apply to a real version tag. Branch and Commit
        // expose no tag, so a range is inapplicable by construction.
        assert!(ResolvedRef::Tag(Version::from("v6.0.2")).tag().is_some());
        assert!(ResolvedRef::Branch(Version::from("main")).tag().is_none());
        assert!(ResolvedRef::Commit.tag().is_none());
    }

    #[test]
    fn commit_has_no_annotation_but_tag_and_branch_do() {
        assert_eq!(
            ResolvedRef::Tag(Version::from("v4"))
                .annotation()
                .map(Version::as_str),
            Some("v4")
        );
        assert_eq!(
            ResolvedRef::Branch(Version::from("main"))
                .annotation()
                .map(Version::as_str),
            Some("main")
        );
        assert!(ResolvedRef::Commit.annotation().is_none());
    }

    #[test]
    fn commit_labels_round_trip_through_sha() {
        // The lock's `version` slot for a bare commit pin is its SHA string,
        // preserving the on-disk format without a sentinel Version.
        let sha = CommitSha::from("6d1e696000000000000000000000000000000000");
        assert_eq!(ResolvedRef::Commit.label(&sha), sha.as_str());
        assert_eq!(
            ResolvedRef::Tag(Version::from("v4.2.1")).label(&sha),
            "v4.2.1"
        );
    }

    #[test]
    fn from_stored_maps_ref_type_to_kind() {
        let v = Version::from("v6.0.2");
        assert_eq!(
            ResolvedRef::from_stored(v.clone(), Some(&RefType::Commit)),
            ResolvedRef::Commit
        );
        assert_eq!(
            ResolvedRef::from_stored(Version::from("main"), Some(&RefType::Branch)),
            ResolvedRef::Branch(Version::from("main"))
        );
        assert_eq!(
            ResolvedRef::from_stored(v.clone(), Some(&RefType::Tag)),
            ResolvedRef::Tag(v.clone())
        );
        assert_eq!(
            ResolvedRef::from_stored(v.clone(), Some(&RefType::Release)),
            ResolvedRef::Tag(v.clone())
        );
        // A bare commit SHA stored with no ref_type resolves to a Tag label
        // faithfully round-tripping whatever string was persisted.
        assert_eq!(
            ResolvedRef::from_stored(v.clone(), None),
            ResolvedRef::Tag(v)
        );
    }
}
