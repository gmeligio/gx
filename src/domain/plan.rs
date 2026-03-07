use std::path::PathBuf;

use super::{ActionId, ActionOverride, LockEntry, LockKey, Version};

/// Describes the changes to apply to a manifest file.
#[derive(Debug, Default)]
pub struct ManifestDiff {
    pub added: Vec<(ActionId, Version)>,
    pub removed: Vec<ActionId>,
    pub updated: Vec<(ActionId, Version)>,
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
    pub specifier: Option<Option<String>>,
}

/// Describes the changes to apply to a lock file.
#[derive(Debug, Default)]
pub struct LockDiff {
    pub added: Vec<(LockKey, LockEntry)>,
    pub removed: Vec<LockKey>,
    pub updated: Vec<(LockKey, LockEntryPatch)>,
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
    use super::*;

    #[test]
    fn manifest_diff_is_empty_when_default() {
        let diff = ManifestDiff::default();
        assert!(diff.is_empty());
    }

    #[test]
    fn manifest_diff_is_not_empty_after_adding_entry() {
        let diff = ManifestDiff {
            added: vec![(ActionId::from("actions/checkout"), Version::from("v4"))],
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
        use crate::domain::{CommitSha, RefType};

        let diff = LockDiff {
            added: vec![(
                LockKey::new(ActionId::from("actions/checkout"), Version::from("v4")),
                LockEntry::new(
                    CommitSha::from("abc123"),
                    "actions/checkout".to_string(),
                    RefType::Tag,
                    "2026-01-01T00:00:00Z".to_string(),
                ),
            )],
            ..Default::default()
        };
        assert!(!diff.is_empty());
    }
}
