use super::action::identity::ActionId;
use super::action::resolved::ResolvedAction;
use super::action::spec::Spec;
use super::action::specifier::Specifier;
use super::lock::LockEntry;
use super::manifest::overrides::ActionOverride;
use std::path::PathBuf;

/// Describes the changes to apply to a manifest file.
#[derive(Debug, Default)]
pub struct ManifestDiff {
    pub added: Vec<(ActionId, Specifier)>,
    pub removed: Vec<ActionId>,
    pub updated: Vec<(ActionId, Specifier)>,
    pub overrides_added: Vec<(ActionId, ActionOverride)>,
    pub overrides_removed: Vec<(ActionId, Vec<ActionOverride>)>,
}

impl ManifestDiff {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.added.is_empty()
            && self.removed.is_empty()
            && self.updated.is_empty()
            && self.overrides_added.is_empty()
            && self.overrides_removed.is_empty()
    }
}

/// Patch for updating specific fields of a lock entry.
#[derive(Debug)]
pub struct LockEntryPatch {
    pub version: Option<Option<String>>,
    pub comment: Option<String>,
}

/// Describes the changes to apply to a lock file.
#[derive(Debug, Default)]
pub struct LockDiff {
    pub added: Vec<(Spec, LockEntry)>,
    pub removed: Vec<Spec>,
    pub updated: Vec<(Spec, LockEntryPatch)>,
}

impl LockDiff {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty() && self.updated.is_empty()
    }
}

/// A set of pin changes for a single workflow file.
#[derive(Debug)]
pub struct WorkflowPatch {
    pub path: PathBuf,
    pub pins: Vec<ResolvedAction>,
}

#[cfg(test)]
mod tests {
    use super::{LockDiff, LockEntry, ManifestDiff};
    use crate::domain::action::identity::{ActionId, CommitDate, CommitSha, Repository, Version};
    use crate::domain::action::resolved::Commit;
    use crate::domain::action::spec::Spec;
    use crate::domain::action::specifier::Specifier;
    use crate::domain::action::uses_ref::RefType;

    #[test]
    fn manifest_diff_is_empty_when_default() {
        let diff = ManifestDiff::default();
        assert!(diff.is_empty());
    }

    #[test]
    fn manifest_diff_is_not_empty_after_adding_entry() {
        let diff = ManifestDiff {
            added: vec![(ActionId::from("actions/checkout"), Specifier::parse("^4"))],
            ..Default::default()
        };
        assert!(!diff.is_empty());
    }

    #[test]
    fn lock_diff_is_empty_when_default() {
        let diff = LockDiff::default();
        assert!(diff.is_empty());
    }

    #[test]
    fn lock_diff_is_not_empty_after_adding_entry() {
        let diff = LockDiff {
            added: vec![(
                Spec::new(ActionId::from("actions/checkout"), Specifier::parse("^4")),
                LockEntry {
                    version: Version::from("v4"),
                    commit: Commit {
                        sha: CommitSha::from("abc123"),
                        repository: Repository::from("actions/checkout"),
                        ref_type: Some(RefType::Tag),
                        date: CommitDate::from("2026-01-01T00:00:00Z"),
                    },
                },
            )],
            ..Default::default()
        };
        assert!(!diff.is_empty());
    }
}
