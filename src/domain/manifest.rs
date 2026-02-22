use std::collections::HashMap;
use std::fmt;

use super::{ActionId, ActionSpec, Version, WorkflowActionSet};

/// Domain entity owning the manifest's action→version mapping and all domain behaviour.
/// No I/O — persistence is handled by infrastructure's `ManifestStore` trait.
#[derive(Debug, Default)]
pub struct Manifest {
    actions: HashMap<ActionId, ActionSpec>,
}

impl Manifest {
    /// Create a `Manifest` from an existing map of IDs to specs.
    #[must_use]
    pub fn new(actions: HashMap<ActionId, ActionSpec>) -> Self {
        Self { actions }
    }

    /// Get the version pinned for an action.
    #[must_use]
    pub fn get(&self, id: &ActionId) -> Option<&Version> {
        self.actions.get(id).map(|s| &s.version)
    }

    /// Set or update the version for an action.
    pub fn set(&mut self, id: ActionId, version: Version) {
        let spec = ActionSpec::new(id.clone(), version);
        self.actions.insert(id, spec);
    }

    /// Remove an action from the manifest.
    pub fn remove(&mut self, id: &ActionId) {
        self.actions.remove(id);
    }

    /// Check if the manifest contains an action.
    #[must_use]
    pub fn has(&self, id: &ActionId) -> bool {
        self.actions.contains_key(id)
    }

    /// Check if the manifest has no actions.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.actions.is_empty()
    }

    /// Get all action specs.
    #[must_use]
    pub fn specs(&self) -> Vec<&ActionSpec> {
        self.actions.values().collect()
    }

    /// Detect drift between the manifest and the given workflow action set.
    ///
    /// If `filter` is `Some(id)`, only that action is checked (used for targeted upgrades).
    /// If `filter` is `None`, all actions on both sides are checked.
    ///
    /// Drift is any of:
    /// - Action in workflow but absent from manifest
    /// - Action in manifest but absent from all workflows
    /// - Action present in both but with differing versions
    ///
    /// # Panics
    ///
    /// Panics if an action is found in both workflow and manifest but `get` returns `None`
    /// (this cannot happen; the `has` check ensures the entry exists).
    #[must_use]
    pub fn detect_drift(
        &self,
        action_set: &WorkflowActionSet,
        filter: Option<&ActionId>,
    ) -> Vec<DriftItem> {
        let mut drift = Vec::new();

        // Determine which action IDs to check
        let workflow_ids: Vec<ActionId> = action_set.action_ids();
        let manifest_ids: Vec<&ActionId> = self.actions.keys().collect();

        // Check actions in workflow not in manifest (or just the filtered one)
        for id in &workflow_ids {
            if let Some(f) = filter
                && id != f
            {
                continue;
            }
            if self.has(id) {
                // Both sides have it — compare versions
                let manifest_version = self.get(id).expect("checked with has()");
                let workflow_versions = action_set.versions_for(id);
                let workflow_version = Version::highest(&workflow_versions)
                    .unwrap_or_else(|| workflow_versions[0].clone());
                if &workflow_version != manifest_version {
                    drift.push(DriftItem::VersionMismatch {
                        id: id.clone(),
                        manifest_version: manifest_version.clone(),
                        workflow_version,
                    });
                }
            } else {
                drift.push(DriftItem::MissingFromManifest { id: id.clone() });
            }
        }

        // Check actions in manifest not in workflow (or just the filtered one)
        for id in &manifest_ids {
            if let Some(f) = filter
                && *id != f
            {
                continue;
            }
            if action_set.versions_for(id).is_empty() {
                drift.push(DriftItem::MissingFromWorkflow { id: (*id).clone() });
            }
        }

        drift
    }
}

/// Represents a single point of drift between the manifest and the workflow action set.
#[derive(Debug)]
pub enum DriftItem {
    /// Action found in a workflow but absent from `gx.toml`.
    MissingFromManifest { id: ActionId },
    /// Action in `gx.toml` but not referenced in any workflow.
    MissingFromWorkflow { id: ActionId },
    /// Action is in both but with different versions.
    VersionMismatch {
        id: ActionId,
        manifest_version: Version,
        workflow_version: Version,
    },
}

impl fmt::Display for DriftItem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingFromManifest { id } => {
                write!(f, "{id}: in workflow but not in gx.toml")
            }
            Self::MissingFromWorkflow { id } => {
                write!(f, "{id}: in gx.toml but not in any workflow")
            }
            Self::VersionMismatch {
                id,
                manifest_version,
                workflow_version,
            } => {
                write!(
                    f,
                    "{id}: workflow has {workflow_version}, gx.toml has {manifest_version}"
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{ActionId, InterpretedRef, Version, WorkflowActionSet};

    fn make_manifest(entries: &[(&str, &str)]) -> Manifest {
        let mut m = Manifest::default();
        for (id, ver) in entries {
            m.set(ActionId::from(*id), Version::from(*ver));
        }
        m
    }

    fn make_action_set(entries: &[(&str, &str)]) -> WorkflowActionSet {
        let mut set = WorkflowActionSet::new();
        for (id, ver) in entries {
            set.add(&InterpretedRef {
                id: ActionId::from(*id),
                version: Version::from(*ver),
                sha: None,
            });
        }
        set
    }

    // --- Manifest CRUD ---

    #[test]
    fn test_set_and_get() {
        let mut m = Manifest::default();
        m.set(ActionId::from("actions/checkout"), Version::from("v4"));
        assert_eq!(
            m.get(&ActionId::from("actions/checkout")),
            Some(&Version::from("v4"))
        );
    }

    #[test]
    fn test_has_and_remove() {
        let mut m = Manifest::default();
        m.set(ActionId::from("actions/checkout"), Version::from("v4"));
        assert!(m.has(&ActionId::from("actions/checkout")));
        m.remove(&ActionId::from("actions/checkout"));
        assert!(!m.has(&ActionId::from("actions/checkout")));
    }

    #[test]
    fn test_is_empty() {
        let mut m = Manifest::default();
        assert!(m.is_empty());
        m.set(ActionId::from("actions/checkout"), Version::from("v4"));
        assert!(!m.is_empty());
    }

    #[test]
    fn test_specs() {
        let mut m = Manifest::default();
        m.set(ActionId::from("actions/checkout"), Version::from("v4"));
        m.set(ActionId::from("actions/setup-node"), Version::from("v3"));
        assert_eq!(m.specs().len(), 2);
    }

    // --- detect_drift ---

    #[test]
    fn test_no_drift_returns_empty() {
        let manifest = make_manifest(&[("actions/checkout", "v4")]);
        let action_set = make_action_set(&[("actions/checkout", "v4")]);
        assert!(manifest.detect_drift(&action_set, None).is_empty());
    }

    #[test]
    fn test_missing_from_manifest() {
        let manifest = make_manifest(&[]);
        let action_set = make_action_set(&[("actions/checkout", "v4")]);
        let drift = manifest.detect_drift(&action_set, None);
        assert_eq!(drift.len(), 1);
        assert!(matches!(
            &drift[0],
            DriftItem::MissingFromManifest { id } if id.as_str() == "actions/checkout"
        ));
    }

    #[test]
    fn test_missing_from_workflow() {
        let manifest = make_manifest(&[("actions/checkout", "v4")]);
        let action_set = make_action_set(&[]);
        let drift = manifest.detect_drift(&action_set, None);
        assert_eq!(drift.len(), 1);
        assert!(matches!(
            &drift[0],
            DriftItem::MissingFromWorkflow { id } if id.as_str() == "actions/checkout"
        ));
    }

    #[test]
    fn test_version_mismatch() {
        let manifest = make_manifest(&[("actions/checkout", "v3")]);
        let action_set = make_action_set(&[("actions/checkout", "v4")]);
        let drift = manifest.detect_drift(&action_set, None);
        assert_eq!(drift.len(), 1);
        assert!(matches!(
            &drift[0],
            DriftItem::VersionMismatch { id, manifest_version, workflow_version }
            if id.as_str() == "actions/checkout"
                && manifest_version.as_str() == "v3"
                && workflow_version.as_str() == "v4"
        ));
    }

    #[test]
    fn test_filter_only_checks_target_action() {
        // actions/checkout has drift (v3 vs v4), actions/setup-node is missing from manifest
        let manifest = make_manifest(&[("actions/checkout", "v3")]);
        let action_set =
            make_action_set(&[("actions/checkout", "v4"), ("actions/setup-node", "v4")]);
        let filter = ActionId::from("actions/checkout");
        let drift = manifest.detect_drift(&action_set, Some(&filter));
        // Only checkout is checked — setup-node MissingFromManifest is ignored
        assert_eq!(drift.len(), 1);
        assert!(matches!(&drift[0], DriftItem::VersionMismatch { .. }));
    }

    #[test]
    fn test_filter_no_drift_on_target_returns_empty() {
        // actions/checkout is fine; actions/setup-node missing from manifest — but filter is checkout
        let manifest = make_manifest(&[("actions/checkout", "v4")]);
        let action_set =
            make_action_set(&[("actions/checkout", "v4"), ("actions/setup-node", "v4")]);
        let filter = ActionId::from("actions/checkout");
        let drift = manifest.detect_drift(&action_set, Some(&filter));
        assert!(drift.is_empty());
    }

    // --- Display ---

    #[test]
    fn test_drift_item_display_missing_from_manifest() {
        let item = DriftItem::MissingFromManifest {
            id: ActionId::from("actions/checkout"),
        };
        assert_eq!(
            item.to_string(),
            "actions/checkout: in workflow but not in gx.toml"
        );
    }

    #[test]
    fn test_drift_item_display_missing_from_workflow() {
        let item = DriftItem::MissingFromWorkflow {
            id: ActionId::from("actions/checkout"),
        };
        assert_eq!(
            item.to_string(),
            "actions/checkout: in gx.toml but not in any workflow"
        );
    }

    #[test]
    fn test_drift_item_display_version_mismatch() {
        let item = DriftItem::VersionMismatch {
            id: ActionId::from("actions/checkout"),
            manifest_version: Version::from("v3"),
            workflow_version: Version::from("v4"),
        };
        assert_eq!(
            item.to_string(),
            "actions/checkout: workflow has v4, gx.toml has v3"
        );
    }
}
