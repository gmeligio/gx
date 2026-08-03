use crate::domain::action::identity::ActionId;
use crate::domain::action::spec::Spec;
use crate::domain::action::specifier::Specifier;
use crate::domain::workflow_actions::{
    ActionSet as WorkflowActionSet, JobId, Located as LocatedAction, Location as WorkflowLocation,
    StepIndex, WorkflowPath,
};
use std::collections::HashSet;

/// A version override for a specific file location.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionOverride {
    /// Relative path from repo root, e.g. ".github/workflows/deploy.yml".
    pub workflow: WorkflowPath,
    /// Job id, if scoped to a job. Always `None` for a composite action's step.
    pub job: Option<JobId>,
    /// 0-based step index, if scoped to a step. Requires a job in a workflow;
    /// stands alone in a composite action, which has none.
    pub step: Option<StepIndex>,
    /// The specifier to use at this location.
    pub version: Specifier,
}

/// Resolve the effective specifier for an action at a given file location.
///
/// Resolution order, most specific first: job-step, job, file-step, file, then the global
/// default (returned as `None` — the caller falls back to it). File-step addresses a
/// composite action's step, which belongs to no job.
///
/// The job-bearing tiers and file-step cannot collide — a composite location has no job,
/// a workflow location always has one.
#[must_use]
pub fn resolve_version<'ovr>(
    overrides: &'ovr [ActionOverride],
    location: &WorkflowLocation,
) -> Option<&'ovr Specifier> {
    if let (Some(job), Some(step)) = (&location.job, location.step) {
        for exc in overrides {
            if exc.workflow == location.workflow
                && exc.job.as_ref() == Some(job)
                && exc.step == Some(step)
            {
                return Some(&exc.version);
            }
        }
    }

    if let Some(job) = &location.job {
        for exc in overrides {
            if exc.workflow == location.workflow
                && exc.job.as_ref() == Some(job)
                && exc.step.is_none()
            {
                return Some(&exc.version);
            }
        }
    }

    if let Some(step) = location.step
        && location.job.is_none()
    {
        for exc in overrides {
            if exc.workflow == location.workflow && exc.job.is_none() && exc.step == Some(step) {
                return Some(&exc.version);
            }
        }
    }

    for exc in overrides {
        if exc.workflow == location.workflow && exc.job.is_none() && exc.step.is_none() {
            return Some(&exc.version);
        }
    }

    None
}

/// Compute all lock keys needed for overrides: one per (action, version) pair.
pub fn override_lock_keys<'ovr>(
    id: &'ovr ActionId,
    overrides: &'ovr [ActionOverride],
) -> impl Iterator<Item = Spec> + 'ovr {
    overrides
        .iter()
        .map(move |exc| Spec::new(id.clone(), exc.version.clone()))
}

/// Ensure overrides exist for every located step whose version differs from the manifest
/// global, **only when** multiple distinct versions of that action appear across workflows.
///
/// When only one version appears in workflows, no override is created.
#[expect(clippy::implicit_hasher, reason = "callers always use std HashMap")]
pub fn sync(
    actions_overrides: &mut std::collections::HashMap<ActionId, Vec<ActionOverride>>,
    actions_global: &std::collections::HashMap<ActionId, Spec>,
    located: &[LocatedAction],
    action_set: &WorkflowActionSet,
) {
    for action in located {
        let version_count = action_set.versions_for(&action.action.id).count();
        if version_count <= 1 {
            continue;
        }

        let global_specifier = match actions_global.get(&action.action.id) {
            Some(spec) => spec.specifier.clone(),
            None => continue,
        };

        let action_specifier = Specifier::from_v1(action.action.reference.label());

        if action_specifier == global_specifier {
            continue;
        }

        let empty: &[ActionOverride] = &[];
        let existing_overrides = actions_overrides
            .get(&action.action.id)
            .map_or(empty, Vec::as_slice);

        let already_covered = existing_overrides.iter().any(|o| {
            o.workflow == action.location.workflow
                && o.job == action.location.job
                && o.step == action.location.step
        });

        if !already_covered {
            actions_overrides
                .entry(action.action.id.clone())
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
#[expect(clippy::implicit_hasher, reason = "callers always use std HashMap")]
pub fn prune_stale(
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
                // An override lives while one scanned location matches every field it
                // names. Job and step must match the *same* location, so a composite
                // override — step, no job — is checked against job-less locations.
                .filter(|exc| {
                    if !live_workflows.contains(exc.workflow.as_str()) {
                        return false;
                    }
                    if exc.job.is_none() && exc.step.is_none() {
                        return true;
                    }
                    located.iter().any(|a| {
                        a.location.workflow == exc.workflow
                            && a.location.job == exc.job
                            && (exc.step.is_none() || a.location.step == exc.step)
                    })
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
#[expect(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::get_unwrap,
    reason = "tests use unwrap, indexing, and other patterns freely"
)]
mod tests {
    use super::{ActionOverride, LocatedAction, prune_stale, resolve_version, sync};
    use crate::domain::action::identity::{ActionId, Version};
    use crate::domain::action::spec::Spec;
    use crate::domain::action::specifier::Specifier;
    use crate::domain::workflow_actions::{
        ActionSet as WorkflowActionSet, JobId, Location as WorkflowLocation, StepIndex,
        WorkflowPath,
    };

    use std::collections::HashMap;
    fn make_loc(workflow: &str, job: Option<&str>, step: Option<u16>) -> WorkflowLocation {
        WorkflowLocation {
            workflow: WorkflowPath::new(workflow),
            job: job.map(JobId::from),
            step: step.map(StepIndex::from),
            line: None,
        }
    }

    fn make_located(workflow: &str, action: &str, version: &str) -> LocatedAction {
        use crate::domain::action::uses_ref::ParsedRef;
        use crate::domain::workflow_actions::WorkflowAction;
        LocatedAction {
            action: WorkflowAction {
                id: ActionId::from(action),
                reference: ParsedRef::Ref(Version::from(version)),
            },
            location: make_loc(workflow, None, None),
        }
    }

    #[test]
    fn resolve_version_returns_none_when_no_overrides() {
        let overrides: Vec<ActionOverride> = vec![];
        let loc = make_loc(".github/workflows/ci.yml", Some("build"), Some(0));
        assert_eq!(resolve_version(&overrides, &loc), None);
    }

    #[test]
    fn resolve_version_workflow_level() {
        let overrides = vec![ActionOverride {
            workflow: WorkflowPath::new(".github/workflows/ci.yml"),
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
    fn resolve_version_step_level_wins_over_workflow() {
        let overrides = vec![
            ActionOverride {
                workflow: WorkflowPath::new(".github/workflows/ci.yml"),
                job: None,
                step: None,
                version: Specifier::parse("^3"),
            },
            ActionOverride {
                workflow: WorkflowPath::new(".github/workflows/ci.yml"),
                job: Some(JobId::from("build")),
                step: Some(StepIndex::from(0_u16)),
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
    fn sync_no_op_when_single_version() {
        let mut actions_overrides: HashMap<ActionId, Vec<ActionOverride>> = HashMap::new();
        let mut actions_global: HashMap<ActionId, Spec> = HashMap::new();
        actions_global.insert(
            ActionId::from("actions/checkout"),
            Spec::new(ActionId::from("actions/checkout"), Specifier::parse("^4")),
        );

        let mut action_set = WorkflowActionSet::new();
        let located = vec![make_located(
            ".github/workflows/ci.yml",
            "actions/checkout",
            "v4",
        )];
        for a in &located {
            action_set.add(&a.action);
        }

        sync(
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
    fn sync_adds_override_for_minority_version() {
        let mut actions_overrides: HashMap<ActionId, Vec<ActionOverride>> = HashMap::new();
        let mut actions_global: HashMap<ActionId, Spec> = HashMap::new();
        actions_global.insert(
            ActionId::from("actions/checkout"),
            Spec::new(ActionId::from("actions/checkout"), Specifier::parse("^4")),
        );

        let mut action_set = WorkflowActionSet::new();
        let located = vec![
            make_located(".github/workflows/ci.yml", "actions/checkout", "v4"),
            make_located(".github/workflows/ci.yml", "actions/checkout", "v4"),
            make_located(".github/workflows/windows.yml", "actions/checkout", "v3"),
        ];
        for a in &located {
            action_set.add(&a.action);
        }

        sync(
            &mut actions_overrides,
            &actions_global,
            &located,
            &action_set,
        );
        let overrides = actions_overrides
            .get(&ActionId::from("actions/checkout"))
            .unwrap();
        assert_eq!(overrides.len(), 1);
        assert_eq!(
            overrides[0].workflow,
            WorkflowPath::new(".github/workflows/windows.yml")
        );
        assert_eq!(overrides[0].version, Specifier::from_v1("v3"));
    }

    #[test]
    fn prune_stale_removes_override_for_missing_workflow() {
        let mut actions_overrides: HashMap<ActionId, Vec<ActionOverride>> = HashMap::new();
        actions_overrides.insert(
            ActionId::from("actions/checkout"),
            vec![ActionOverride {
                workflow: WorkflowPath::new(".github/workflows/deploy.yml"),
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
        prune_stale(&mut actions_overrides, &located);

        assert!(
            actions_overrides
                .get(&ActionId::from("actions/checkout"))
                .is_none_or(Vec::is_empty)
        );
    }

    #[test]
    fn prune_stale_keeps_live_overrides() {
        let mut actions_overrides: HashMap<ActionId, Vec<ActionOverride>> = HashMap::new();
        actions_overrides.insert(
            ActionId::from("actions/checkout"),
            vec![ActionOverride {
                workflow: WorkflowPath::new(".github/workflows/ci.yml"),
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
        prune_stale(&mut actions_overrides, &located);

        assert_eq!(
            actions_overrides
                .get(&ActionId::from("actions/checkout"))
                .map(Vec::len),
            Some(1)
        );
    }

    /// Multiple workflows with v6.0.1 + one with v5 → `sync` creates override for v5.
    #[test]
    fn sync_multiple_sha_workflows_with_minority_version() {
        let mut actions_overrides: HashMap<ActionId, Vec<ActionOverride>> = HashMap::new();
        let mut actions_global: HashMap<ActionId, Spec> = HashMap::new();
        // Global is v6.0.1 (dominant version)
        actions_global.insert(
            ActionId::from("actions/checkout"),
            Spec::new(
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
            action_set.add(&a.action);
        }

        sync(
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
            overrides[0].workflow.as_str().ends_with("windows.yml"),
            "override must be scoped to windows.yml"
        );
        assert_eq!(
            overrides[0].version,
            Specifier::from_v1("v5"),
            "override version must be v5"
        );
    }

    #[test]
    fn prune_stale_removes_deploy_yml_when_only_ci_exists() {
        let mut actions_overrides: HashMap<ActionId, Vec<ActionOverride>> = HashMap::new();
        actions_overrides.insert(
            ActionId::from("actions/checkout"),
            vec![ActionOverride {
                workflow: WorkflowPath::new(".github/workflows/deploy.yml"),
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
        prune_stale(&mut actions_overrides, &located);

        assert!(
            actions_overrides
                .get(&ActionId::from("actions/checkout"))
                .is_none_or(Vec::is_empty),
            "stale deploy.yml override must be removed"
        );
    }

    #[test]
    fn prune_stale_removes_job_override_when_job_is_gone() {
        let mut actions_overrides: HashMap<ActionId, Vec<ActionOverride>> = HashMap::new();
        actions_overrides.insert(
            ActionId::from("actions/checkout"),
            vec![ActionOverride {
                workflow: WorkflowPath::new(".github/workflows/ci.yml"),
                job: Some(JobId::from("deleted")),
                step: None,
                version: Specifier::from_v1("v3"),
            }],
        );

        let mut live = make_located(".github/workflows/ci.yml", "actions/checkout", "v4");
        live.location = make_loc(".github/workflows/ci.yml", Some("build"), Some(0));
        prune_stale(&mut actions_overrides, &[live]);

        assert!(
            actions_overrides
                .get(&ActionId::from("actions/checkout"))
                .is_none_or(Vec::is_empty),
            "override naming a job that no longer exists must be pruned"
        );
    }

    #[test]
    fn prune_stale_keeps_job_override_while_job_exists() {
        let mut actions_overrides: HashMap<ActionId, Vec<ActionOverride>> = HashMap::new();
        actions_overrides.insert(
            ActionId::from("actions/checkout"),
            vec![ActionOverride {
                workflow: WorkflowPath::new(".github/workflows/ci.yml"),
                job: Some(JobId::from("build")),
                step: None,
                version: Specifier::from_v1("v3"),
            }],
        );

        let mut live = make_located(".github/workflows/ci.yml", "actions/checkout", "v4");
        live.location = make_loc(".github/workflows/ci.yml", Some("build"), Some(0));
        prune_stale(&mut actions_overrides, &[live]);

        assert_eq!(
            actions_overrides
                .get(&ActionId::from("actions/checkout"))
                .map(Vec::len),
            Some(1)
        );
    }
}

#[cfg(test)]
#[path = "overrides_composite_tests.rs"]
mod composite_tests;
