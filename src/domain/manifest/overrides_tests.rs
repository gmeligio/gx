//! Override resolution, sync, and pruning tests.
//!
//! Split from `overrides.rs` to keep that file within the 550-line budget enforced by
//! `tests/code_health.rs`; the composite-specific cases live in
//! `overrides_composite_tests.rs`.

#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::get_unwrap,
    reason = "tests use unwrap, indexing, and other patterns freely"
)]

use super::{ActionOverride, LocatedAction, prune_stale, resolve_version, sync};
use crate::domain::action::identity::{ActionId, Version};
use crate::domain::action::spec::Spec;
use crate::domain::action::specifier::Specifier;
use crate::domain::file::actions::ActionSet as WorkflowActionSet;
use crate::domain::file::site::{Id, JobId, Origin, Scope, Slot, StepIndex, WorkflowPath};

use std::collections::HashMap;

/// A site naming one step of one job.
fn workflow_step(workflow: &str, job: &str, step: u16) -> Id {
    Id {
        file: WorkflowPath::new(workflow),
        slot: Slot::WorkflowStep {
            job: JobId::from(job),
            step: StepIndex::from(step),
        },
    }
}

fn make_located(workflow: &str, action: &str, version: &str) -> LocatedAction {
    use crate::domain::action::uses_ref::ParsedRef;
    use crate::domain::file::actions::WorkflowAction;
    LocatedAction {
        action: WorkflowAction {
            id: ActionId::from(action),
            reference: ParsedRef::Ref(Version::from(version)),
        },
        site: workflow_step(workflow, "build", 0),
        origin: Origin::default(),
    }
}

#[test]
fn resolve_version_returns_none_when_no_overrides() {
    let overrides: Vec<ActionOverride> = vec![];
    let loc = workflow_step(".github/workflows/ci.yml", "build", 0);
    assert_eq!(resolve_version(&overrides, &loc), None);
}

#[test]
fn resolve_version_workflow_level() {
    let overrides = vec![ActionOverride {
        workflow: WorkflowPath::new(".github/workflows/ci.yml"),
        scope: Scope::File,
        version: Specifier::parse("^3"),
    }];
    let loc = workflow_step(".github/workflows/ci.yml", "build", 0);
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
            scope: Scope::File,
            version: Specifier::parse("^3"),
        },
        ActionOverride {
            workflow: WorkflowPath::new(".github/workflows/ci.yml"),
            scope: Scope::JobStep {
                job: JobId::from("build"),
                step: StepIndex::from(0_u16),
            },
            version: Specifier::parse("^2"),
        },
    ];
    let loc = workflow_step(".github/workflows/ci.yml", "build", 0);
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
            scope: Scope::File,
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
            scope: Scope::File,
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
            scope: Scope::File,
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
            scope: Scope::Job {
                job: JobId::from("deleted"),
            },
            version: Specifier::from_v1("v3"),
        }],
    );

    let mut live = make_located(".github/workflows/ci.yml", "actions/checkout", "v4");
    live.site = workflow_step(".github/workflows/ci.yml", "build", 0);
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
            scope: Scope::Job {
                job: JobId::from("build"),
            },
            version: Specifier::from_v1("v3"),
        }],
    );

    let mut live = make_located(".github/workflows/ci.yml", "actions/checkout", "v4");
    live.site = workflow_step(".github/workflows/ci.yml", "build", 0);
    prune_stale(&mut actions_overrides, &[live]);

    assert_eq!(
        actions_overrides
            .get(&ActionId::from("actions/checkout"))
            .map(Vec::len),
        Some(1)
    );
}
