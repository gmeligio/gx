use std::collections::{HashMap, HashSet};

use super::action::{ActionId, CommitSha, InterpretedRef, Version};

/// Aggregates action versions discovered across all workflows.
/// This handles the domain logic of deciding which version "wins"
/// when multiple versions exist for the same action.
#[derive(Debug, Default)]
pub struct WorkflowActionSet {
    /// Maps action ID to set of versions found in workflows
    versions: HashMap<ActionId, HashSet<Version>>,
    /// Count of how many times each version appears for each action (across all steps)
    counts: HashMap<ActionId, HashMap<Version, usize>>,
}

impl WorkflowActionSet {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a `WorkflowActionSet` from a slice of `LocatedAction`.
    /// Builds the `versions` and `counts` maps from the actions (no `shas` field).
    #[must_use]
    pub fn from_located(actions: &[LocatedAction]) -> Self {
        let mut set = Self::new();
        for action in actions {
            // Build versions and counts maps from the actions
            set.versions
                .entry(action.id.clone())
                .or_default()
                .insert(action.version.clone());

            *set.counts
                .entry(action.id.clone())
                .or_default()
                .entry(action.version.clone())
                .or_insert(0) += 1;
        }
        set
    }

    /// Add an interpreted action reference to the set.
    pub fn add(&mut self, interpreted: &InterpretedRef) {
        self.versions
            .entry(interpreted.id.clone())
            .or_default()
            .insert(interpreted.version.clone());

        // Track occurrence count for dominant_version selection
        *self
            .counts
            .entry(interpreted.id.clone())
            .or_default()
            .entry(interpreted.version.clone())
            .or_insert(0) += 1;
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
        Some(Version::highest(&candidates).unwrap_or_else(|| candidates[0].clone()))
    }

    /// Returns true if no actions have been added.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.versions.is_empty()
    }

    /// Get all unique versions found for an action.
    #[must_use]
    pub fn versions_for(&self, id: &ActionId) -> Vec<Version> {
        self.versions
            .get(id)
            .map(|v| v.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Get all action IDs discovered across workflows.
    #[must_use]
    pub fn action_ids(&self) -> Vec<ActionId> {
        self.versions.keys().cloned().collect()
    }
}

/// The precise location of a `uses:` reference within the workflow tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowLocation {
    /// Relative path from repo root, e.g. ".github/workflows/ci.yml"
    pub workflow: String,
    /// Job id, e.g. "build"
    pub job: Option<String>,
    /// 0-based step index within the job
    pub step: Option<usize>,
}

/// A single action reference with its full location context.
#[derive(Debug, Clone)]
pub struct LocatedAction {
    pub id: ActionId,
    pub version: Version,
    pub sha: Option<CommitSha>,
    pub location: WorkflowLocation,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_interpreted(name: &str, version: &str, sha: Option<&str>) -> InterpretedRef {
        InterpretedRef {
            id: ActionId::from(name),
            version: Version::from(version),
            sha: sha.map(CommitSha::from),
        }
    }

    #[test]
    fn test_most_used_version_two_vs_one() {
        let mut set = WorkflowActionSet::new();
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
    fn test_dominant_version_tiebreak_highest_semver() {
        let mut set = WorkflowActionSet::new();
        // Both versions appear once — tiebreak by highest semver
        set.add(&make_interpreted("actions/checkout", "v3", None));
        set.add(&make_interpreted("actions/checkout", "v4", None));

        let dominant = set.dominant_version(&ActionId::from("actions/checkout"));
        assert_eq!(dominant, Some(Version::from("v4")));
    }

    #[test]
    fn test_workflow_location_equality() {
        let loc1 = WorkflowLocation {
            workflow: ".github/workflows/ci.yml".to_string(),
            job: Some("build".to_string()),
            step: Some(0),
        };
        let loc2 = WorkflowLocation {
            workflow: ".github/workflows/ci.yml".to_string(),
            job: Some("build".to_string()),
            step: Some(0),
        };
        assert_eq!(loc1, loc2);
    }

    #[test]
    fn test_located_action_stores_location() {
        let loc = WorkflowLocation {
            workflow: ".github/workflows/ci.yml".to_string(),
            job: Some("build".to_string()),
            step: Some(0),
        };
        let action = LocatedAction {
            id: ActionId::from("actions/checkout"),
            version: Version::from("v4"),
            sha: None,
            location: loc.clone(),
        };
        assert_eq!(action.location, loc);
        assert_eq!(action.id.as_str(), "actions/checkout");
    }

    #[test]
    fn test_add_single_version() {
        let mut set = WorkflowActionSet::new();
        set.add(&make_interpreted("actions/checkout", "v4", None));

        let versions = set.versions_for(&ActionId::from("actions/checkout"));
        assert_eq!(versions.len(), 1);
        assert!(versions.contains(&Version::from("v4")));
    }

    #[test]
    fn test_add_multiple_versions() {
        let mut set = WorkflowActionSet::new();
        set.add(&make_interpreted("actions/checkout", "v4", None));
        set.add(&make_interpreted("actions/checkout", "v3", None));

        let versions = set.versions_for(&ActionId::from("actions/checkout"));
        assert_eq!(versions.len(), 2);
        assert!(versions.contains(&Version::from("v4")));
        assert!(versions.contains(&Version::from("v3")));
    }

    #[test]
    fn test_add_duplicate_version() {
        let mut set = WorkflowActionSet::new();
        set.add(&make_interpreted("actions/checkout", "v4", None));
        set.add(&make_interpreted("actions/checkout", "v4", None));

        let versions = set.versions_for(&ActionId::from("actions/checkout"));
        assert_eq!(versions.len(), 1);
    }

    #[test]
    fn test_action_ids() {
        let mut set = WorkflowActionSet::new();
        set.add(&make_interpreted("actions/checkout", "v4", None));
        set.add(&make_interpreted("actions/setup-node", "v3", None));

        let ids = set.action_ids();
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&ActionId::from("actions/checkout")));
        assert!(ids.contains(&ActionId::from("actions/setup-node")));
    }

    #[test]
    fn test_versions_for_unknown_action() {
        let set = WorkflowActionSet::new();
        let versions = set.versions_for(&ActionId::from("unknown/action"));
        assert!(versions.is_empty());
    }
}
