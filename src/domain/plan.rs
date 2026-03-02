use std::path::PathBuf;

use super::{
    ActionId, ActionOverride, LockEntry, LockKey, UpgradeCandidate, Version, VersionCorrection,
};

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

/// The complete plan produced by a tidy operation.
#[derive(Debug, Default)]
pub struct TidyPlan {
    pub manifest: ManifestDiff,
    pub lock: LockDiff,
    pub workflows: Vec<WorkflowPatch>,
    pub corrections: Vec<VersionCorrection>,
}

impl TidyPlan {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.manifest.is_empty() && self.lock.is_empty() && self.workflows.is_empty()
    }
}

/// The complete plan produced by an upgrade operation.
#[derive(Debug)]
pub struct UpgradePlan {
    pub manifest: ManifestDiff,
    pub lock: LockDiff,
    pub workflows: Vec<WorkflowPatch>,
    pub upgrades: Vec<UpgradeCandidate>,
}

impl UpgradePlan {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.manifest.is_empty() && self.lock.is_empty() && self.workflows.is_empty()
    }
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

    #[test]
    fn tidy_plan_is_empty_when_all_diffs_empty() {
        let plan = TidyPlan::default();
        assert!(plan.is_empty());
    }

    #[test]
    fn tidy_plan_is_not_empty_when_manifest_has_changes() {
        let plan = TidyPlan {
            manifest: ManifestDiff {
                added: vec![(ActionId::from("actions/checkout"), Version::from("v4"))],
                ..Default::default()
            },
            ..Default::default()
        };
        assert!(!plan.is_empty());
    }

    #[test]
    fn tidy_plan_is_not_empty_when_workflows_have_patches() {
        let plan = TidyPlan {
            workflows: vec![WorkflowPatch {
                path: PathBuf::from("ci.yml"),
                pins: vec![(
                    ActionId::from("actions/checkout"),
                    "abc123 # v4".to_string(),
                )],
            }],
            ..Default::default()
        };
        assert!(!plan.is_empty());
    }
}
