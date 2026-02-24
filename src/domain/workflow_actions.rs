use std::collections::{HashMap, HashSet};

use super::action::{ActionId, CommitSha, InterpretedRef, Version};

/// Aggregates action versions discovered across all workflows.
/// This handles the domain logic of deciding which version "wins"
/// when multiple versions exist for the same action.
#[derive(Debug, Default)]
pub struct WorkflowActionSet {
    /// Maps action ID to set of versions found in workflows
    versions: HashMap<ActionId, HashSet<Version>>,
    /// Maps action ID to SHA if present in workflow (first one wins)
    shas: HashMap<ActionId, CommitSha>,
}

impl WorkflowActionSet {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add an interpreted action reference to the set.
    pub fn add(&mut self, interpreted: &InterpretedRef) {
        self.versions
            .entry(interpreted.id.clone())
            .or_default()
            .insert(interpreted.version.clone());

        // Store SHA if present (first one wins for consistency)
        if let Some(sha) = &interpreted.sha {
            self.shas
                .entry(interpreted.id.clone())
                .or_insert_with(|| sha.clone());
        }
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

    /// Get the SHA for an action if one was found in workflows.
    #[must_use]
    pub fn sha_for(&self, id: &ActionId) -> Option<&CommitSha> {
        self.shas.get(id)
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
    fn test_sha_first_wins() {
        let mut set = WorkflowActionSet::new();
        set.add(&make_interpreted(
            "actions/checkout",
            "v4",
            Some("first_sha_12345678901234567890123456789012"),
        ));
        set.add(&make_interpreted(
            "actions/checkout",
            "v4",
            Some("second_sha_1234567890123456789012345678901"),
        ));

        assert_eq!(
            set.sha_for(&ActionId::from("actions/checkout")),
            Some(&CommitSha::from(
                "first_sha_12345678901234567890123456789012"
            ))
        );
    }

    #[test]
    fn test_sha_none_when_not_present() {
        let mut set = WorkflowActionSet::new();
        set.add(&make_interpreted("actions/checkout", "v4", None));

        assert!(set.sha_for(&ActionId::from("actions/checkout")).is_none());
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
