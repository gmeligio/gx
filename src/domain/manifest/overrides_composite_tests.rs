//! Override resolution and pruning for composite-action step locations, which carry
//! a step index but no job. Kept beside the workflow-location tests in `overrides.rs`.

use super::*;
use crate::domain::action::identity::{ActionId, Version};
use crate::domain::workflow_actions::{
    JobId, Located as LocatedAction, Location as WorkflowLocation, StepIndex, WorkflowPath,
};
use std::collections::HashMap;

const ACTION_FILE: &str = ".github/actions/setup/action.yml";

fn composite_loc(step: u16) -> WorkflowLocation {
    WorkflowLocation {
        workflow: WorkflowPath::new(ACTION_FILE),
        job: None,
        step: Some(StepIndex::from(step)),
        line: None,
    }
}

fn located_at(loc: WorkflowLocation) -> LocatedAction {
    use crate::domain::action::uses_ref::ParsedRef;
    use crate::domain::workflow_actions::WorkflowAction;
    LocatedAction {
        action: WorkflowAction {
            id: ActionId::from("actions/checkout"),
            reference: ParsedRef::Ref(Version::from("v4")),
        },
        location: loc,
    }
}

#[test]
fn composite_step_override_applies() {
    let overrides = vec![ActionOverride {
        workflow: WorkflowPath::new(ACTION_FILE),
        job: None,
        step: Some(StepIndex::from(0_u16)),
        version: Specifier::parse("^3"),
    }];

    assert_eq!(
        resolve_version(&overrides, &composite_loc(0)),
        Some(&Specifier::parse("^3"))
    );
}

#[test]
fn composite_step_override_wins_over_file_level() {
    let overrides = vec![
        ActionOverride {
            workflow: WorkflowPath::new(ACTION_FILE),
            job: None,
            step: None,
            version: Specifier::parse("^2"),
        },
        ActionOverride {
            workflow: WorkflowPath::new(ACTION_FILE),
            job: None,
            step: Some(StepIndex::from(1_u16)),
            version: Specifier::parse("^3"),
        },
    ];

    assert_eq!(
        resolve_version(&overrides, &composite_loc(1)),
        Some(&Specifier::parse("^3"))
    );
    // A step with no matching step-override falls back to the file-level one.
    assert_eq!(
        resolve_version(&overrides, &composite_loc(0)),
        Some(&Specifier::parse("^2"))
    );
}

#[test]
fn job_bearing_override_is_unaffected_by_the_file_step_tier() {
    let overrides = vec![ActionOverride {
        workflow: WorkflowPath::new(".github/workflows/ci.yml"),
        job: Some(JobId::from("build")),
        step: Some(StepIndex::from(0_u16)),
        version: Specifier::parse("^3"),
    }];

    let workflow_loc = WorkflowLocation {
        workflow: WorkflowPath::new(".github/workflows/ci.yml"),
        job: Some(JobId::from("build")),
        step: Some(StepIndex::from(0_u16)),
        line: None,
    };
    assert_eq!(
        resolve_version(&overrides, &workflow_loc),
        Some(&Specifier::parse("^3"))
    );
}

#[test]
fn composite_step_override_survives_while_its_step_exists() {
    let mut map: HashMap<ActionId, Vec<ActionOverride>> = HashMap::new();
    map.insert(
        ActionId::from("actions/checkout"),
        vec![ActionOverride {
            workflow: WorkflowPath::new(ACTION_FILE),
            job: None,
            step: Some(StepIndex::from(0_u16)),
            version: Specifier::parse("^3"),
        }],
    );

    prune_stale(&mut map, &[located_at(composite_loc(0))]);

    assert_eq!(
        map.get(&ActionId::from("actions/checkout")).map(Vec::len),
        Some(1)
    );
}

#[test]
fn composite_step_override_is_pruned_when_its_step_is_gone() {
    let mut map: HashMap<ActionId, Vec<ActionOverride>> = HashMap::new();
    map.insert(
        ActionId::from("actions/checkout"),
        vec![ActionOverride {
            workflow: WorkflowPath::new(ACTION_FILE),
            job: None,
            step: Some(StepIndex::from(5_u16)),
            version: Specifier::parse("^3"),
        }],
    );

    // The file still exists and still holds step 0 — but not step 5.
    prune_stale(&mut map, &[located_at(composite_loc(0))]);

    // prune_stale drops the key entirely once its last override is pruned.
    assert!(
        !map.contains_key(&ActionId::from("actions/checkout")),
        "an override naming a step that no longer exists must be pruned"
    );
}
