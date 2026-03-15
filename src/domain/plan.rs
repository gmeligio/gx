use super::action::identity::ActionId;
use super::action::resolved::Commit;
use super::action::spec::Spec;
use super::action::specifier::Specifier;
use super::lock::resolution::Resolution;
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
    pub added: Vec<(Spec, Resolution, Commit)>,
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
    pub pins: Vec<(ActionId, String)>,
}

#[cfg(test)]
mod tests {
    use super::{ActionId, Commit, LockDiff, ManifestDiff, Resolution, Spec, Specifier};
    use crate::domain::action::identity::{CommitSha, Version};
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
                Resolution {
                    version: Version::from("v4"),
                    comment: String::new(),
                },
                Commit {
                    sha: CommitSha::from("abc123"),
                    repository: "actions/checkout".to_owned(),
                    ref_type: Some(RefType::Tag),
                    date: "2026-01-01T00:00:00Z".to_owned(),
                },
            )],
            ..Default::default()
        };
        assert!(!diff.is_empty());
    }
}
