use std::collections::HashMap;

use super::{ActionId, ActionSpec, Version};
use crate::domain::workflow_actions::WorkflowLocation;

/// A version override for a specific workflow location.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionException {
    /// Relative path from repo root, e.g. ".github/workflows/deploy.yml"
    pub workflow: String,
    /// Job id, if scoped to a job
    pub job: Option<String>,
    /// 0-based step index, if scoped to a step (requires job)
    pub step: Option<usize>,
    /// The version to use at this location
    pub version: Version,
}

/// Domain entity owning the manifest's action→version mapping and all domain behaviour.
/// No I/O — persistence is handled by infrastructure's `ManifestStore` trait.
#[derive(Debug, Default)]
pub struct Manifest {
    actions: HashMap<ActionId, ActionSpec>,
    exceptions: HashMap<ActionId, Vec<ActionException>>,
}

impl Manifest {
    /// Create a `Manifest` from an existing map of IDs to specs.
    #[must_use]
    pub fn new(actions: HashMap<ActionId, ActionSpec>) -> Self {
        Self {
            actions,
            exceptions: HashMap::new(),
        }
    }

    /// Create a `Manifest` with both actions and exceptions.
    #[must_use]
    pub fn with_exceptions(
        actions: HashMap<ActionId, ActionSpec>,
        exceptions: HashMap<ActionId, Vec<ActionException>>,
    ) -> Self {
        Self { actions, exceptions }
    }

    /// Get the version pinned for an action (global default).
    #[must_use]
    pub fn get(&self, id: &ActionId) -> Option<&Version> {
        self.actions.get(id).map(|s| &s.version)
    }

    /// Resolve the effective version for an action at a given workflow location.
    ///
    /// Resolution order (most specific wins):
    /// 1. Step-level exception (workflow + job + step)
    /// 2. Job-level exception (workflow + job)
    /// 3. Workflow-level exception (workflow only)
    /// 4. Global default
    #[must_use]
    pub fn resolve_version(&self, id: &ActionId, location: &WorkflowLocation) -> Option<&Version> {
        if let Some(exceptions) = self.exceptions.get(id) {
            // Step-level: workflow + job + step all match
            if let (Some(job), Some(step)) = (&location.job, location.step) {
                for exc in exceptions {
                    if exc.workflow == location.workflow
                        && exc.job.as_deref() == Some(job.as_str())
                        && exc.step == Some(step)
                    {
                        return Some(&exc.version);
                    }
                }
            }

            // Job-level: workflow + job match, no step in exception
            if let Some(job) = &location.job {
                for exc in exceptions {
                    if exc.workflow == location.workflow
                        && exc.job.as_deref() == Some(job.as_str())
                        && exc.step.is_none()
                    {
                        return Some(&exc.version);
                    }
                }
            }

            // Workflow-level: workflow matches, no job/step in exception
            for exc in exceptions {
                if exc.workflow == location.workflow && exc.job.is_none() && exc.step.is_none() {
                    return Some(&exc.version);
                }
            }
        }

        self.get(id)
    }

    /// Set or update the global version for an action.
    pub fn set(&mut self, id: ActionId, version: Version) {
        let spec = ActionSpec::new(id.clone(), version);
        self.actions.insert(id, spec);
    }

    /// Add an exception entry for an action.
    pub fn add_exception(&mut self, id: ActionId, exception: ActionException) {
        self.exceptions.entry(id).or_default().push(exception);
    }

    /// Get all exceptions for an action.
    #[must_use]
    pub fn exceptions_for(&self, id: &ActionId) -> &[ActionException] {
        self.exceptions.get(id).map_or(&[], Vec::as_slice)
    }

    /// Remove an action from the manifest (global default and all its exceptions).
    pub fn remove(&mut self, id: &ActionId) {
        self.actions.remove(id);
        self.exceptions.remove(id);
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
    #[must_use]
    pub fn specs(&self) -> Vec<&ActionSpec> {
        self.actions.values().collect()
    }

    /// Get all exceptions across all actions.
    #[must_use]
    pub fn all_exceptions(&self) -> &HashMap<ActionId, Vec<ActionException>> {
        &self.exceptions
    }

    /// Replace all exceptions for an action (used by stale cleanup).
    pub fn replace_exceptions(&mut self, id: ActionId, exceptions: Vec<ActionException>) {
        if exceptions.is_empty() {
            self.exceptions.remove(&id);
        } else {
            self.exceptions.insert(id, exceptions);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{ActionId, Version};
    use crate::domain::workflow_actions::WorkflowLocation;

    fn make_loc(workflow: &str, job: Option<&str>, step: Option<usize>) -> WorkflowLocation {
        WorkflowLocation {
            workflow: workflow.to_string(),
            job: job.map(str::to_string),
            step,
        }
    }

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
    fn test_remove_also_clears_exceptions() {
        let mut m = Manifest::default();
        m.set(ActionId::from("actions/checkout"), Version::from("v4"));
        m.add_exception(
            ActionId::from("actions/checkout"),
            ActionException {
                workflow: ".github/workflows/ci.yml".to_string(),
                job: None,
                step: None,
                version: Version::from("v3"),
            },
        );
        m.remove(&ActionId::from("actions/checkout"));
        assert!(m.exceptions_for(&ActionId::from("actions/checkout")).is_empty());
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

    #[test]
    fn test_resolve_version_returns_global_when_no_exception() {
        let mut m = Manifest::default();
        m.set(ActionId::from("actions/checkout"), Version::from("v4"));
        let loc = make_loc(".github/workflows/ci.yml", Some("build"), Some(0));
        assert_eq!(
            m.resolve_version(&ActionId::from("actions/checkout"), &loc),
            Some(&Version::from("v4"))
        );
    }

    #[test]
    fn test_resolve_version_exception_workflow_overrides_global() {
        let mut m = Manifest::default();
        m.set(ActionId::from("actions/checkout"), Version::from("v4"));
        m.add_exception(
            ActionId::from("actions/checkout"),
            ActionException {
                workflow: ".github/workflows/deploy.yml".to_string(),
                job: None,
                step: None,
                version: Version::from("v3"),
            },
        );
        assert_eq!(
            m.resolve_version(
                &ActionId::from("actions/checkout"),
                &make_loc(".github/workflows/deploy.yml", Some("deploy"), Some(0))
            ),
            Some(&Version::from("v3"))
        );
        assert_eq!(
            m.resolve_version(
                &ActionId::from("actions/checkout"),
                &make_loc(".github/workflows/ci.yml", Some("build"), Some(0))
            ),
            Some(&Version::from("v4"))
        );
    }

    #[test]
    fn test_resolve_version_job_overrides_workflow() {
        let mut m = Manifest::default();
        m.set(ActionId::from("actions/checkout"), Version::from("v4"));
        m.add_exception(
            ActionId::from("actions/checkout"),
            ActionException {
                workflow: ".github/workflows/ci.yml".to_string(),
                job: None,
                step: None,
                version: Version::from("v3"),
            },
        );
        m.add_exception(
            ActionId::from("actions/checkout"),
            ActionException {
                workflow: ".github/workflows/ci.yml".to_string(),
                job: Some("legacy-build".to_string()),
                step: None,
                version: Version::from("v2"),
            },
        );
        assert_eq!(
            m.resolve_version(
                &ActionId::from("actions/checkout"),
                &make_loc(".github/workflows/ci.yml", Some("legacy-build"), Some(0))
            ),
            Some(&Version::from("v2"))
        );
        assert_eq!(
            m.resolve_version(
                &ActionId::from("actions/checkout"),
                &make_loc(".github/workflows/ci.yml", Some("build"), Some(0))
            ),
            Some(&Version::from("v3"))
        );
    }

    #[test]
    fn test_resolve_version_step_overrides_job() {
        let mut m = Manifest::default();
        m.set(ActionId::from("actions/checkout"), Version::from("v4"));
        m.add_exception(
            ActionId::from("actions/checkout"),
            ActionException {
                workflow: ".github/workflows/ci.yml".to_string(),
                job: Some("build".to_string()),
                step: None,
                version: Version::from("v3"),
            },
        );
        m.add_exception(
            ActionId::from("actions/checkout"),
            ActionException {
                workflow: ".github/workflows/ci.yml".to_string(),
                job: Some("build".to_string()),
                step: Some(0),
                version: Version::from("v2"),
            },
        );
        assert_eq!(
            m.resolve_version(
                &ActionId::from("actions/checkout"),
                &make_loc(".github/workflows/ci.yml", Some("build"), Some(0))
            ),
            Some(&Version::from("v2"))
        );
        assert_eq!(
            m.resolve_version(
                &ActionId::from("actions/checkout"),
                &make_loc(".github/workflows/ci.yml", Some("build"), Some(1))
            ),
            Some(&Version::from("v3"))
        );
    }

    #[test]
    fn test_resolve_version_returns_none_when_not_in_manifest() {
        let m = Manifest::default();
        assert_eq!(
            m.resolve_version(
                &ActionId::from("actions/checkout"),
                &make_loc(".github/workflows/ci.yml", None, None)
            ),
            None
        );
    }
}
