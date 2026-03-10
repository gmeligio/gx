use crate::domain::workflow_actions::{LocatedAction, WorkflowActionSet, WorkflowLocation};
use crate::domain::{ActionId, ActionSpec, LockKey, Specifier};
use std::collections::HashSet;

/// A version override for a specific workflow location.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionOverride {
    /// Relative path from repo root, e.g. ".github/workflows/deploy.yml"
    pub workflow: String,
    /// Job id, if scoped to a job
    pub job: Option<String>,
    /// 0-based step index, if scoped to a step (requires job)
    pub step: Option<usize>,
    /// The specifier to use at this location
    pub version: Specifier,
}

/// Resolve the effective specifier for an action at a given workflow location.
///
/// Resolution order (most specific wins):
/// 1. Step-level override (workflow + job + step)
/// 2. Job-level override (workflow + job)
/// 3. Workflow-level override (workflow only)
/// 4. Global default (returned as `None` — caller falls back to it)
#[must_use]
pub fn resolve_version<'a>(
    overrides: &'a [ActionOverride],
    location: &WorkflowLocation,
) -> Option<&'a Specifier> {
    // Step-level: workflow + job + step all match
    if let (Some(job), Some(step)) = (&location.job, location.step) {
        for exc in overrides {
            if exc.workflow == location.workflow
                && exc.job.as_deref() == Some(job.as_str())
                && exc.step == Some(step)
            {
                return Some(&exc.version);
            }
        }
    }

    // Job-level: workflow + job match, no step in override
    if let Some(job) = &location.job {
        for exc in overrides {
            if exc.workflow == location.workflow
                && exc.job.as_deref() == Some(job.as_str())
                && exc.step.is_none()
            {
                return Some(&exc.version);
            }
        }
    }

    // Workflow-level: workflow matches, no job/step in override
    for exc in overrides {
        if exc.workflow == location.workflow && exc.job.is_none() && exc.step.is_none() {
            return Some(&exc.version);
        }
    }

    None
}

/// Compute all lock keys needed for overrides: one per (action, version) pair.
pub fn override_lock_keys<'a>(
    id: &'a ActionId,
    overrides: &'a [ActionOverride],
) -> impl Iterator<Item = LockKey> + 'a {
    overrides
        .iter()
        .map(move |exc| LockKey::new(id.clone(), exc.version.clone()))
}

/// Ensure overrides exist for every located step whose version differs from the manifest
/// global, **only when** multiple distinct versions of that action appear across workflows.
///
/// When only one version appears in workflows, no override is created.
pub fn sync_overrides(
    actions_overrides: &mut std::collections::HashMap<ActionId, Vec<ActionOverride>>,
    actions_global: &std::collections::HashMap<ActionId, ActionSpec>,
    located: &[LocatedAction],
    action_set: &WorkflowActionSet,
) {
    for action in located {
        let version_count = action_set.versions_for(&action.id).count();
        if version_count <= 1 {
            continue;
        }

        let global_specifier = match actions_global.get(&action.id) {
            Some(spec) => spec.version.clone(),
            None => continue,
        };

        let action_specifier = Specifier::from_v1(action.version.as_str());

        if action_specifier == global_specifier {
            continue;
        }

        let existing_overrides = actions_overrides
            .get(&action.id)
            .map_or(&[] as &[_], Vec::as_slice);

        let already_covered = existing_overrides.iter().any(|o| {
            o.workflow == action.location.workflow
                && o.job == action.location.job
                && o.step == action.location.step
        });

        if !already_covered {
            actions_overrides
                .entry(action.id.clone())
                .or_default()
                .push(ActionOverride {
                    workflow: action.location.workflow.clone(),
                    job: action.location.job.clone(),
                    step: action.location.step,
                    version: action_specifier,
                });
        }
    }
}

/// Remove override entries whose referenced workflow/job/step no longer exists in the
/// scanned set.
pub fn prune_stale_overrides(
    actions_overrides: &mut std::collections::HashMap<ActionId, Vec<ActionOverride>>,
    located: &[LocatedAction],
) {
    let live_workflows: HashSet<&str> = located
        .iter()
        .map(|a| a.location.workflow.as_str())
        .collect();

    let updates: Vec<(ActionId, Vec<ActionOverride>)> = actions_overrides
        .iter()
        .map(|(id, overrides)| {
            let pruned: Vec<ActionOverride> = overrides
                .iter()
                .filter(|exc| {
                    if !live_workflows.contains(exc.workflow.as_str()) {
                        return false;
                    }
                    if let Some(job) = &exc.job {
                        let job_exists = located.iter().any(|a| {
                            a.location.workflow == exc.workflow
                                && a.location.job.as_deref() == Some(job.as_str())
                        });
                        if !job_exists {
                            return false;
                        }
                    }
                    if let (Some(job), Some(step)) = (&exc.job, exc.step) {
                        let step_exists = located.iter().any(|a| {
                            a.location.workflow == exc.workflow
                                && a.location.job.as_deref() == Some(job.as_str())
                                && a.location.step == Some(step)
                        });
                        if !step_exists {
                            return false;
                        }
                    }
                    true
                })
                .cloned()
                .collect();
            (id.clone(), pruned)
        })
        .collect();

    for (id, pruned) in updates {
        if pruned.is_empty() {
            actions_overrides.remove(&id);
        } else {
            actions_overrides.insert(id, pruned);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ActionOverride, LocatedAction, prune_stale_overrides, resolve_version, sync_overrides,
    };
    use crate::domain::action::Version;
    use crate::domain::workflow_actions::{WorkflowActionSet, WorkflowLocation};
    use crate::domain::{ActionId, ActionSpec, Specifier};

    use std::collections::HashMap;
    fn make_loc(workflow: &str, job: Option<&str>, step: Option<usize>) -> WorkflowLocation {
        WorkflowLocation {
            workflow: workflow.to_string(),
            job: job.map(str::to_string),
            step,
        }
    }

    fn make_located(workflow: &str, action: &str, version: &str) -> LocatedAction {
        LocatedAction {
            id: ActionId::from(action),
            version: Version::from(version),
            sha: None,
            location: make_loc(workflow, None, None),
        }
    }

    #[test]
    fn test_resolve_version_returns_none_when_no_overrides() {
        let overrides: Vec<ActionOverride> = vec![];
        let loc = make_loc(".github/workflows/ci.yml", Some("build"), Some(0));
        assert_eq!(resolve_version(&overrides, &loc), None);
    }

    #[test]
    fn test_resolve_version_workflow_level() {
        let overrides = vec![ActionOverride {
            workflow: ".github/workflows/ci.yml".to_string(),
            job: None,
            step: None,
            version: Specifier::parse("^3"),
        }];
        let loc = make_loc(".github/workflows/ci.yml", Some("build"), Some(0));
        assert_eq!(
            resolve_version(&overrides, &loc),
            Some(&Specifier::parse("^3"))
        );
    }

    #[test]
    fn test_resolve_version_step_level_wins_over_workflow() {
        let overrides = vec![
            ActionOverride {
                workflow: ".github/workflows/ci.yml".to_string(),
                job: None,
                step: None,
                version: Specifier::parse("^3"),
            },
            ActionOverride {
                workflow: ".github/workflows/ci.yml".to_string(),
                job: Some("build".to_string()),
                step: Some(0),
                version: Specifier::parse("^2"),
            },
        ];
        let loc = make_loc(".github/workflows/ci.yml", Some("build"), Some(0));
        assert_eq!(
            resolve_version(&overrides, &loc),
            Some(&Specifier::parse("^2"))
        );
    }

    #[test]
    fn sync_overrides_no_op_when_single_version() {
        let mut actions_overrides: HashMap<ActionId, Vec<ActionOverride>> = HashMap::new();
        let mut actions_global: HashMap<ActionId, ActionSpec> = HashMap::new();
        actions_global.insert(
            ActionId::from("actions/checkout"),
            ActionSpec::new(ActionId::from("actions/checkout"), Specifier::parse("^4")),
        );

        let mut action_set = WorkflowActionSet::new();
        let located = vec![make_located(
            ".github/workflows/ci.yml",
            "actions/checkout",
            "v4",
        )];
        for a in &located {
            action_set.add_located(a);
        }

        sync_overrides(
            &mut actions_overrides,
            &actions_global,
            &located,
            &action_set,
        );
        assert!(
            actions_overrides
                .get(&ActionId::from("actions/checkout"))
                .is_none_or(Vec::is_empty)
        );
    }

    #[test]
    fn sync_overrides_adds_override_for_minority_version() {
        let mut actions_overrides: HashMap<ActionId, Vec<ActionOverride>> = HashMap::new();
        let mut actions_global: HashMap<ActionId, ActionSpec> = HashMap::new();
        actions_global.insert(
            ActionId::from("actions/checkout"),
            ActionSpec::new(ActionId::from("actions/checkout"), Specifier::parse("^4")),
        );

        let mut action_set = WorkflowActionSet::new();
        let located = vec![
            make_located(".github/workflows/ci.yml", "actions/checkout", "v4"),
            make_located(".github/workflows/ci.yml", "actions/checkout", "v4"),
            make_located(".github/workflows/windows.yml", "actions/checkout", "v3"),
        ];
        for a in &located {
            action_set.add_located(a);
        }

        sync_overrides(
            &mut actions_overrides,
            &actions_global,
            &located,
            &action_set,
        );
        let overrides = actions_overrides
            .get(&ActionId::from("actions/checkout"))
            .unwrap();
        assert_eq!(overrides.len(), 1);
        assert_eq!(overrides[0].workflow, ".github/workflows/windows.yml");
        assert_eq!(overrides[0].version, Specifier::from_v1("v3"));
    }

    #[test]
    fn prune_stale_overrides_removes_override_for_missing_workflow() {
        let mut actions_overrides: HashMap<ActionId, Vec<ActionOverride>> = HashMap::new();
        actions_overrides.insert(
            ActionId::from("actions/checkout"),
            vec![ActionOverride {
                workflow: ".github/workflows/deploy.yml".to_string(),
                job: None,
                step: None,
                version: Specifier::parse("v3"),
            }],
        );

        let located = vec![make_located(
            ".github/workflows/ci.yml",
            "actions/checkout",
            "v4",
        )];
        prune_stale_overrides(&mut actions_overrides, &located);

        assert!(
            actions_overrides
                .get(&ActionId::from("actions/checkout"))
                .is_none_or(Vec::is_empty)
        );
    }

    #[test]
    fn prune_stale_overrides_keeps_live_overrides() {
        let mut actions_overrides: HashMap<ActionId, Vec<ActionOverride>> = HashMap::new();
        actions_overrides.insert(
            ActionId::from("actions/checkout"),
            vec![ActionOverride {
                workflow: ".github/workflows/ci.yml".to_string(),
                job: None,
                step: None,
                version: Specifier::parse("v3"),
            }],
        );

        let located = vec![make_located(
            ".github/workflows/ci.yml",
            "actions/checkout",
            "v4",
        )];
        prune_stale_overrides(&mut actions_overrides, &located);

        assert_eq!(
            actions_overrides
                .get(&ActionId::from("actions/checkout"))
                .map(Vec::len),
            Some(1)
        );
    }

    // --- Override lifecycle tests (migrated from tidy/tests.rs) ---

    /// Multiple workflows with v6.0.1 + one with v5 → `sync_overrides` creates override for v5.
    #[test]
    fn sync_overrides_multiple_sha_workflows_with_minority_version() {
        let mut actions_overrides: HashMap<ActionId, Vec<ActionOverride>> = HashMap::new();
        let mut actions_global: HashMap<ActionId, ActionSpec> = HashMap::new();
        // Global is v6.0.1 (dominant version)
        actions_global.insert(
            ActionId::from("actions/checkout"),
            ActionSpec::new(
                ActionId::from("actions/checkout"),
                Specifier::from_v1("v6.0.1"),
            ),
        );

        let mut action_set = WorkflowActionSet::new();
        let located = vec![
            make_located(".github/workflows/ci.yml", "actions/checkout", "v6.0.1"),
            make_located(".github/workflows/build.yml", "actions/checkout", "v6.0.1"),
            make_located(".github/workflows/windows.yml", "actions/checkout", "v5"),
        ];
        for a in &located {
            action_set.add_located(a);
        }

        sync_overrides(
            &mut actions_overrides,
            &actions_global,
            &located,
            &action_set,
        );

        let overrides = actions_overrides
            .get(&ActionId::from("actions/checkout"))
            .expect("override must exist for minority version");
        assert_eq!(overrides.len(), 1, "exactly one override for v5");
        assert!(
            overrides[0].workflow.ends_with("windows.yml"),
            "override must be scoped to windows.yml"
        );
        assert_eq!(
            overrides[0].version,
            Specifier::from_v1("v5"),
            "override version must be v5"
        );
    }

    /// Stale override for deploy.yml (which no longer exists) is removed by prune.
    #[test]
    fn prune_stale_overrides_removes_deploy_yml_when_only_ci_exists() {
        let mut actions_overrides: HashMap<ActionId, Vec<ActionOverride>> = HashMap::new();
        actions_overrides.insert(
            ActionId::from("actions/checkout"),
            vec![ActionOverride {
                workflow: ".github/workflows/deploy.yml".to_string(),
                job: None,
                step: None,
                version: Specifier::from_v1("v3"),
            }],
        );

        // Only ci.yml is live — deploy.yml has been deleted
        let located = vec![make_located(
            ".github/workflows/ci.yml",
            "actions/checkout",
            "v4",
        )];
        prune_stale_overrides(&mut actions_overrides, &located);

        assert!(
            actions_overrides
                .get(&ActionId::from("actions/checkout"))
                .is_none_or(Vec::is_empty),
            "stale deploy.yml override must be removed"
        );
    }
}
