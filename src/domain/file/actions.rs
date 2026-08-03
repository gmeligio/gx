use super::site::{Id, Origin};
use crate::domain::action::identity::{ActionId, Version};
use crate::domain::action::uses_ref::ParsedRef;
use std::collections::{HashMap, HashSet};

/// An action as declared in a workflow file.
///
/// Represents the interpreted form of a `uses:` line: the action identity and
/// its typed reference (tag/branch, bare SHA, or SHA-with-comment pin).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowAction {
    /// The parsed action identifier.
    pub id: ActionId,
    /// The typed parsed reference.
    pub reference: ParsedRef,
}

/// Aggregates action versions discovered across all workflows.
/// This handles the domain logic of deciding which version "wins"
/// when multiple versions exist for the same action.
#[derive(Debug, Default)]
pub struct ActionSet {
    /// Maps action ID to set of versions found in workflows.
    versions: HashMap<ActionId, HashSet<Version>>,
    /// Count of how many times each version appears for each action (across all steps).
    counts: HashMap<ActionId, HashMap<Version, usize>>,
}

impl ActionSet {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Create an `ActionSet` from a slice of `Located`.
    /// Builds the `versions` and `counts` maps from the actions.
    #[must_use]
    pub fn from_located(actions: &[Located]) -> Self {
        let mut set = Self::new();
        for action in actions {
            set.add(&action.action);
        }
        set
    }

    /// Add an interpreted action reference to the set.
    pub fn add(&mut self, interpreted: &WorkflowAction) {
        let version = interpreted.reference.label_version();
        self.versions
            .entry(interpreted.id.clone())
            .or_default()
            .insert(version.clone());

        // Track occurrence count for dominant_version selection
        let count = self
            .counts
            .entry(interpreted.id.clone())
            .or_default()
            .entry(version)
            .or_insert(0);
        *count = count.saturating_add(1);
    }

    /// Select the dominant version for an action:
    /// 1. Most-used (highest occurrence count across all steps)
    /// 2. Tiebreak: highest semver
    #[must_use]
    pub fn dominant_version(&self, id: &ActionId) -> Option<Version> {
        let counts = self.counts.get(id)?;
        let max_count = counts.values().max().copied()?;
        let candidates: Vec<Version> = counts
            .iter()
            .filter(|(_, c)| **c == max_count)
            .map(|(v, _)| v.clone())
            .collect();
        Version::highest(&candidates)
    }

    /// Returns true if no actions have been added.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.versions.is_empty()
    }

    /// Get all unique versions found for an action.
    pub fn versions_for(&self, id: &ActionId) -> impl Iterator<Item = &Version> {
        self.versions
            .get(id)
            .map(|v| v.iter())
            .into_iter()
            .flatten()
    }

    /// Get all action IDs discovered across workflows.
    pub fn action_ids(&self) -> impl Iterator<Item = &ActionId> {
        self.versions.keys()
    }
}

/// A single action reference: what it is, where it lives, and where it was read from.
#[derive(Debug, Clone)]
pub struct Located {
    /// The interpreted action reference (id, version, optional SHA).
    pub action: WorkflowAction,
    /// Which file and position — the identity user config addresses.
    pub site: Id,
    /// Where it was read from, for reporting only.
    pub origin: Origin,
}

#[cfg(test)]
mod tests {
    use super::{ActionId, ActionSet, Located, ParsedRef, Version, WorkflowAction};
    use crate::domain::action::identity::CommitSha;
    use crate::domain::file::site::{Id, JobId, Origin, Slot, StepIndex, WorkflowPath};

    /// Build a `WorkflowAction` in the same shape `UsesRef::interpret` would:
    /// a bare `version` becomes a `Ref`; a `version` + `sha` becomes a `Pinned`.
    fn make_interpreted(name: &str, version: &str, sha: Option<&str>) -> WorkflowAction {
        let reference = sha.map_or_else(
            || ParsedRef::Ref(Version::from(version)),
            |sha_str| ParsedRef::Pinned {
                sha: CommitSha::from(sha_str),
                comment: Version::from(version),
            },
        );
        WorkflowAction {
            id: ActionId::from(name),
            reference,
        }
    }

    #[test]
    fn most_used_version_two_vs_one() {
        let mut set = ActionSet::new();
        // Add v3 twice (two different steps)
        set.add(&make_interpreted("actions/checkout", "v3", None));
        set.add(&make_interpreted("actions/checkout", "v3", None));
        // Add v4 once
        set.add(&make_interpreted("actions/checkout", "v4", None));

        // v3 appears 2 times, v4 appears 1 time — v3 wins even though v4 is higher semver
        let dominant = set.dominant_version(&ActionId::from("actions/checkout"));
        assert_eq!(dominant, Some(Version::from("v3")));
    }

    #[test]
    fn dominant_version_tiebreak_highest_semver() {
        let mut set = ActionSet::new();
        // Both versions appear once — tiebreak by highest semver
        set.add(&make_interpreted("actions/checkout", "v3", None));
        set.add(&make_interpreted("actions/checkout", "v4", None));

        let dominant = set.dominant_version(&ActionId::from("actions/checkout"));
        assert_eq!(dominant, Some(Version::from("v4")));
    }

    #[test]
    fn workflow_location_equality() {
        let loc1 = Id {
            file: WorkflowPath::new(".github/workflows/ci.yml"),
            slot: Slot::WorkflowStep {
                job: JobId::from("build"),
                step: StepIndex::from(0_u16),
            },
        };
        let loc2 = Id {
            file: WorkflowPath::new(".github/workflows/ci.yml"),
            slot: Slot::WorkflowStep {
                job: JobId::from("build"),
                step: StepIndex::from(0_u16),
            },
        };
        assert_eq!(loc1, loc2);
    }

    #[test]
    fn located_action_stores_location() {
        let loc = Id {
            file: WorkflowPath::new(".github/workflows/ci.yml"),
            slot: Slot::WorkflowStep {
                job: JobId::from("build"),
                step: StepIndex::from(0_u16),
            },
        };
        let action = Located {
            action: WorkflowAction {
                id: ActionId::from("actions/checkout"),
                reference: ParsedRef::Ref(Version::from("v4")),
            },
            site: loc.clone(),
            origin: Origin::default(),
        };
        assert_eq!(action.site, loc);
        assert_eq!(action.action.id.as_str(), "actions/checkout");
    }

    #[test]
    fn add_single_version() {
        let mut set = ActionSet::new();
        set.add(&make_interpreted("actions/checkout", "v4", None));

        let versions: Vec<_> = set
            .versions_for(&ActionId::from("actions/checkout"))
            .collect();
        assert_eq!(versions.len(), 1);
        assert!(versions.contains(&&Version::from("v4")));
    }

    #[test]
    fn add_multiple_versions() {
        let mut set = ActionSet::new();
        set.add(&make_interpreted("actions/checkout", "v4", None));
        set.add(&make_interpreted("actions/checkout", "v3", None));

        let versions: Vec<_> = set
            .versions_for(&ActionId::from("actions/checkout"))
            .collect();
        assert_eq!(versions.len(), 2);
        assert!(versions.contains(&&Version::from("v4")));
        assert!(versions.contains(&&Version::from("v3")));
    }

    #[test]
    fn add_duplicate_version() {
        let mut set = ActionSet::new();
        set.add(&make_interpreted("actions/checkout", "v4", None));
        set.add(&make_interpreted("actions/checkout", "v4", None));

        assert_eq!(
            set.versions_for(&ActionId::from("actions/checkout"))
                .count(),
            1
        );
    }

    #[test]
    fn action_ids() {
        let mut set = ActionSet::new();
        set.add(&make_interpreted("actions/checkout", "v4", None));
        set.add(&make_interpreted("actions/setup-node", "v3", None));

        let ids: Vec<_> = set.action_ids().collect();
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&&ActionId::from("actions/checkout")));
        assert!(ids.contains(&&ActionId::from("actions/setup-node")));
    }

    #[test]
    fn versions_for_unknown_action() {
        let set = ActionSet::new();
        assert_eq!(
            set.versions_for(&ActionId::from("unknown/action")).count(),
            0
        );
    }
}
