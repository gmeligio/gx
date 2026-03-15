pub mod overrides;

use super::action::identity::ActionId;
use super::action::spec::Spec;
use super::action::specifier::Specifier;
use super::plan::ManifestDiff;
use super::workflow_actions::{ActionSet, Located, Location};
use overrides::ActionOverride;
use std::collections::{HashMap, HashSet};

/// Domain entity owning the manifest's action→specifier mapping and all domain behaviour.
/// No I/O — persistence is handled by infrastructure's file-backed save methods.
#[derive(Debug, Default, Clone)]
pub struct Manifest {
    /// Global action-to-spec mappings.
    actions: HashMap<ActionId, Spec>,
    /// Per-action override entries scoped to specific workflows, jobs, or steps.
    overrides: HashMap<ActionId, Vec<ActionOverride>>,
}

impl Manifest {
    /// Create a `Manifest` from an existing map of IDs to specs.
    #[must_use]
    pub fn new(actions: HashMap<ActionId, Spec>) -> Self {
        Self {
            actions,
            overrides: HashMap::new(),
        }
    }

    /// Create a `Manifest` with both actions and overrides.
    #[must_use]
    pub fn with_overrides(
        actions: HashMap<ActionId, Spec>,
        new_overrides: HashMap<ActionId, Vec<ActionOverride>>,
    ) -> Self {
        Self {
            actions,
            overrides: new_overrides,
        }
    }

    /// Get the specifier pinned for an action (global default).
    #[must_use]
    pub fn get(&self, id: &ActionId) -> Option<&Specifier> {
        self.actions.get(id).map(|s| &s.version)
    }

    /// Resolve the effective specifier for an action at a given workflow location.
    ///
    /// Resolution order (most specific wins):
    /// 1. Step-level override (workflow + job + step)
    /// 2. Job-level override (workflow + job)
    /// 3. Workflow-level override (workflow only)
    /// 4. Global default
    #[must_use]
    pub fn resolve_version(&self, id: &ActionId, location: &Location) -> Option<&Specifier> {
        if let Some(ovrs) = self.overrides.get(id)
            && let Some(v) = overrides::resolve_version(ovrs, location)
        {
            return Some(v);
        }
        self.get(id)
    }

    /// Set or update the global specifier for an action.
    pub fn set(&mut self, id: ActionId, version: Specifier) {
        let spec = Spec::new(id.clone(), version);
        self.actions.insert(id, spec);
    }

    /// Add an override entry for an action.
    pub fn add_override(&mut self, id: ActionId, action_override: ActionOverride) {
        self.overrides.entry(id).or_default().push(action_override);
    }

    /// Get all overrides for an action.
    #[must_use]
    pub fn overrides_for(&self, id: &ActionId) -> &[ActionOverride] {
        self.overrides.get(id).map_or(&[], Vec::as_slice)
    }

    /// Remove an action from the manifest (global default and all its overrides).
    pub fn remove(&mut self, id: &ActionId) {
        self.actions.remove(id);
        self.overrides.remove(id);
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

    /// Get all action specs (global defaults only).
    pub fn specs(&self) -> impl Iterator<Item = &Spec> {
        self.actions.values()
    }

    /// Get all overrides across all actions.
    #[must_use]
    pub fn all_overrides(&self) -> &HashMap<ActionId, Vec<ActionOverride>> {
        &self.overrides
    }

    /// Replace all overrides for an action (used by stale cleanup).
    pub fn replace_overrides(&mut self, id: ActionId, new_overrides: Vec<ActionOverride>) {
        if new_overrides.is_empty() {
            self.overrides.remove(&id);
        } else {
            self.overrides.insert(id, new_overrides);
        }
    }

    /// Ensure overrides exist for every located step whose version differs from the manifest
    /// global, **only when** multiple distinct versions of that action appear across workflows.
    ///
    /// When only one version appears in workflows, no override is created.
    pub fn sync_overrides(&mut self, located: &[Located], action_set: &ActionSet) {
        overrides::sync(&mut self.overrides, &self.actions, located, action_set);
    }

    /// Remove override entries whose referenced workflow/job/step no longer exists in the
    /// scanned set.
    pub fn prune_stale_overrides(&mut self, located: &[Located]) {
        overrides::prune_stale(&mut self.overrides, located);
    }

    /// Compute all lock keys needed: one per (action, version) pair across globals and overrides.
    #[must_use]
    pub fn lock_keys(&self) -> Vec<Spec> {
        let seen: HashSet<Spec> = self
            .specs()
            .cloned()
            .chain(
                self.all_overrides()
                    .iter()
                    .flat_map(|(id, ovrs)| overrides::override_lock_keys(id, ovrs)),
            )
            .collect();
        seen.into_iter().collect()
    }

    /// Compute the diff between this manifest (`before`) and `other` (`after`).
    ///
    /// Detects added, removed, updated actions and override changes (added/removed).
    #[must_use]
    pub fn diff(&self, other: &Manifest) -> ManifestDiff {
        let before_ids: HashSet<ActionId> = self.specs().map(|s| s.id.clone()).collect();
        let after_ids: HashSet<ActionId> = other.specs().map(|s| s.id.clone()).collect();

        let added: Vec<(ActionId, Specifier)> = after_ids
            .difference(&before_ids)
            .filter_map(|id| other.get(id).map(|v| (id.clone(), v.clone())))
            .collect();

        let removed: Vec<ActionId> = before_ids.difference(&after_ids).cloned().collect();

        let updated: Vec<(ActionId, Specifier)> = before_ids
            .intersection(&after_ids)
            .filter_map(|id| {
                let bv = self.get(id)?;
                let av = other.get(id)?;
                (bv != av).then(|| (id.clone(), av.clone()))
            })
            .collect();

        // Diff overrides
        let before_overrides = self.all_overrides();
        let after_overrides = other.all_overrides();

        let mut overrides_added = Vec::new();
        let mut overrides_removed = Vec::new();

        for (id, after_list) in after_overrides {
            let before_list = before_overrides.get(id).cloned().unwrap_or_default();
            for ovr in after_list {
                if !before_list.contains(ovr) {
                    overrides_added.push((id.clone(), ovr.clone()));
                }
            }
        }

        for (id, before_list) in before_overrides {
            let after_list = after_overrides.get(id).cloned().unwrap_or_default();
            let removed_for_id: Vec<ActionOverride> = before_list
                .iter()
                .filter(|ovr| !after_list.contains(ovr))
                .cloned()
                .collect();
            if !removed_for_id.is_empty() {
                overrides_removed.push((id.clone(), removed_for_id));
            }
        }

        ManifestDiff {
            added,
            removed,
            updated,
            overrides_added,
            overrides_removed,
        }
    }
}

#[cfg(test)]
#[expect(
    clippy::indexing_slicing,
    reason = "tests use unwrap, indexing, and other patterns freely"
)]
mod tests {
    use super::{ActionId, ActionOverride, Manifest, Specifier};
    use crate::domain::workflow_actions::{JobId, Location, StepIndex, WorkflowPath};

    fn make_loc(workflow: &str, job: Option<&str>, step: Option<u16>) -> Location {
        Location {
            workflow: WorkflowPath::new(workflow),
            job: job.map(JobId::from),
            step: step.map(StepIndex::from),
        }
    }

    #[test]
    fn set_and_get() {
        let mut m = Manifest::default();
        m.set(ActionId::from("actions/checkout"), Specifier::parse("^4"));
        assert_eq!(
            m.get(&ActionId::from("actions/checkout")),
            Some(&Specifier::parse("^4"))
        );
    }

    #[test]
    fn has_and_remove() {
        let mut m = Manifest::default();
        m.set(ActionId::from("actions/checkout"), Specifier::parse("^4"));
        assert!(m.has(&ActionId::from("actions/checkout")));
        m.remove(&ActionId::from("actions/checkout"));
        assert!(!m.has(&ActionId::from("actions/checkout")));
    }

    #[test]
    fn remove_also_clears_overrides() {
        let mut m = Manifest::default();
        m.set(ActionId::from("actions/checkout"), Specifier::parse("^4"));
        m.add_override(
            ActionId::from("actions/checkout"),
            ActionOverride {
                workflow: WorkflowPath::new(".github/workflows/ci.yml"),
                job: None,
                step: None,
                version: Specifier::parse("^3"),
            },
        );
        m.remove(&ActionId::from("actions/checkout"));
        assert!(
            m.overrides_for(&ActionId::from("actions/checkout"))
                .is_empty()
        );
    }

    #[test]
    fn is_empty() {
        let mut m = Manifest::default();
        assert!(m.is_empty());
        m.set(ActionId::from("actions/checkout"), Specifier::parse("^4"));
        assert!(!m.is_empty());
    }

    #[test]
    fn specs() {
        let mut m = Manifest::default();
        m.set(ActionId::from("actions/checkout"), Specifier::parse("^4"));
        m.set(ActionId::from("actions/setup-node"), Specifier::parse("^3"));
        assert_eq!(m.specs().count(), 2);
    }

    #[test]
    fn resolve_version_returns_global_when_no_override() {
        let mut m = Manifest::default();
        m.set(ActionId::from("actions/checkout"), Specifier::parse("^4"));
        let loc = make_loc(".github/workflows/ci.yml", Some("build"), Some(0));
        assert_eq!(
            m.resolve_version(&ActionId::from("actions/checkout"), &loc),
            Some(&Specifier::parse("^4"))
        );
    }

    #[test]
    fn resolve_version_returns_none_when_not_in_manifest() {
        let m = Manifest::default();
        assert_eq!(
            m.resolve_version(
                &ActionId::from("actions/checkout"),
                &make_loc(".github/workflows/ci.yml", None, None)
            ),
            None
        );
    }

    // --- Manifest::diff tests ---

    #[test]
    fn diff_empty_manifests_is_empty() {
        let before = Manifest::default();
        let after = Manifest::default();
        assert!(before.diff(&after).is_empty());
    }

    #[test]
    fn diff_detects_added_action() {
        let before = Manifest::default();
        let mut after = Manifest::default();
        after.set(ActionId::from("actions/checkout"), Specifier::parse("^4"));

        let diff = before.diff(&after);
        assert_eq!(diff.added.len(), 1);
        assert_eq!(diff.added[0].0, ActionId::from("actions/checkout"));
        assert_eq!(diff.added[0].1, Specifier::parse("^4"));
        assert!(diff.removed.is_empty());
        assert!(diff.updated.is_empty());
    }

    #[test]
    fn diff_detects_removed_action() {
        let mut before = Manifest::default();
        before.set(ActionId::from("actions/checkout"), Specifier::parse("^4"));
        let after = Manifest::default();

        let diff = before.diff(&after);
        assert!(diff.added.is_empty());
        assert_eq!(diff.removed.len(), 1);
        assert_eq!(diff.removed[0], ActionId::from("actions/checkout"));
        assert!(diff.updated.is_empty());
    }

    #[test]
    fn diff_detects_updated_action() {
        let mut before = Manifest::default();
        before.set(ActionId::from("actions/checkout"), Specifier::parse("^3"));
        let mut after = Manifest::default();
        after.set(ActionId::from("actions/checkout"), Specifier::parse("^4"));

        let diff = before.diff(&after);
        assert!(diff.added.is_empty());
        assert!(diff.removed.is_empty());
        assert_eq!(diff.updated.len(), 1);
        assert_eq!(diff.updated[0].0, ActionId::from("actions/checkout"));
        assert_eq!(diff.updated[0].1, Specifier::parse("^4"));
    }

    #[test]
    fn diff_unchanged_action_not_in_diff() {
        let mut before = Manifest::default();
        before.set(ActionId::from("actions/checkout"), Specifier::parse("^4"));
        let after = before.clone();

        let diff = before.diff(&after);
        assert!(diff.is_empty());
    }

    #[test]
    fn diff_detects_override_added() {
        let mut before = Manifest::default();
        before.set(ActionId::from("actions/checkout"), Specifier::parse("^4"));
        let mut after = before.clone();
        after.add_override(
            ActionId::from("actions/checkout"),
            ActionOverride {
                workflow: WorkflowPath::new(".github/workflows/ci.yml"),
                job: None,
                step: None,
                version: Specifier::parse("^3"),
            },
        );

        let diff = before.diff(&after);
        assert_eq!(diff.overrides_added.len(), 1);
        assert!(diff.overrides_removed.is_empty());
    }

    #[test]
    fn diff_detects_override_removed() {
        let mut before = Manifest::default();
        before.set(ActionId::from("actions/checkout"), Specifier::parse("^4"));
        before.add_override(
            ActionId::from("actions/checkout"),
            ActionOverride {
                workflow: WorkflowPath::new(".github/workflows/ci.yml"),
                job: None,
                step: None,
                version: Specifier::parse("^3"),
            },
        );
        let mut after = Manifest::default();
        after.set(ActionId::from("actions/checkout"), Specifier::parse("^4"));

        let diff = before.diff(&after);
        assert!(diff.overrides_added.is_empty());
        assert_eq!(diff.overrides_removed.len(), 1);
    }

    // --- lock_keys tests ---

    #[test]
    fn lock_keys_returns_global_keys() {
        let mut m = Manifest::default();
        m.set(ActionId::from("actions/checkout"), Specifier::parse("^4"));
        m.set(ActionId::from("actions/setup-node"), Specifier::parse("^3"));

        let keys = m.lock_keys();
        assert_eq!(keys.len(), 2);
    }

    #[test]
    fn lock_keys_includes_override_versions() {
        let mut m = Manifest::default();
        m.set(ActionId::from("actions/checkout"), Specifier::parse("^4"));
        m.add_override(
            ActionId::from("actions/checkout"),
            ActionOverride {
                workflow: WorkflowPath::new(".github/workflows/windows.yml"),
                job: None,
                step: None,
                version: Specifier::parse("^3"),
            },
        );

        let keys = m.lock_keys();
        assert_eq!(keys.len(), 2, "should have keys for ^4 and ^3");
    }

    #[test]
    fn lock_keys_deduplicates() {
        let mut m = Manifest::default();
        m.set(ActionId::from("actions/checkout"), Specifier::parse("^4"));
        m.add_override(
            ActionId::from("actions/checkout"),
            ActionOverride {
                workflow: WorkflowPath::new(".github/workflows/ci.yml"),
                job: None,
                step: None,
                version: Specifier::parse("^3"),
            },
        );
        m.add_override(
            ActionId::from("actions/checkout"),
            ActionOverride {
                workflow: WorkflowPath::new(".github/workflows/deploy.yml"),
                job: None,
                step: None,
                version: Specifier::parse("^3"),
            },
        );

        let keys = m.lock_keys();
        assert_eq!(
            keys.len(),
            2,
            "^4 and ^3 — duplicated ^3 overrides deduplicated"
        );
    }
}
