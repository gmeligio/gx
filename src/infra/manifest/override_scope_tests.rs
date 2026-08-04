//! Parsing of `[actions.overrides]` scopes.
//!
//! Split from `tests.rs` to stay within the 550-line budget enforced by
//! `tests/code_health.rs`. These cover the mapping between the three optional TOML keys
//! (`workflow`, `job`, `step`) and the `Scope` sum type they parse into.

use super::{Store, parse};
use crate::domain::action::identity::ActionId;
use crate::domain::file::site::{JobId, Scope, Slot, StepIndex};
use std::io::Write as _;
use tempfile::NamedTempFile;

/// Parses rather than erroring because the manifest is read before any scan, so parse time
/// cannot know whether the named file has jobs. Reporting an override that selects nothing
/// belongs after the scan, with `prune_stale`.
#[test]
fn step_without_job_on_a_workflow_parses_and_selects_nothing() {
    let content = concat!(
        "[actions]\n",
        "\"actions/checkout\" = \"^4\"\n",
        "\n",
        "[actions.overrides]\n",
        "\"actions/checkout\" = [\n",
        "  { workflow = \".github/workflows/ci.yml\", step = 0, version = \"^3\" },\n",
        "]\n",
    );
    let mut file = NamedTempFile::new().unwrap();
    file.write_all(content.as_bytes()).unwrap();

    let loaded = parse(file.path()).unwrap();
    let overrides = loaded
        .value
        .overrides_for(&ActionId::from("actions/checkout"));

    assert_eq!(
        overrides
            .iter()
            .map(|o| o.scope.clone())
            .collect::<Vec<_>>(),
        vec![Scope::CompositeStep {
            step: StepIndex::from(0_u16)
        }],
        "the override parses to a composite-step scope whatever the path"
    );

    assert!(
        !overrides[0].scope.selects(&Slot::WorkflowStep {
            job: JobId::from("build"),
            step: StepIndex::from(0_u16),
        }),
        "a composite-step override must not select a workflow step"
    );
}

/// The same shape *is* valid on a composite action file, which has no jobs to name.
#[test]
fn step_without_job_on_a_composite_action_is_accepted() {
    let content = concat!(
        "[actions]\n",
        "\"actions/checkout\" = \"^4\"\n",
        "\n",
        "[actions.overrides]\n",
        "\"actions/checkout\" = [\n",
        "  { workflow = \".github/actions/setup/action.yml\", step = 0, version = \"^3\" },\n",
        "]\n",
    );
    let mut file = NamedTempFile::new().unwrap();
    file.write_all(content.as_bytes()).unwrap();

    let loaded = parse(file.path()).unwrap();
    let overrides = loaded
        .value
        .overrides_for(&ActionId::from("actions/checkout"));

    assert_eq!(overrides.len(), 1);
    assert_eq!(
        overrides[0].scope,
        Scope::CompositeStep {
            step: StepIndex::from(0_u16)
        }
    );
}

/// Every scope a user can write survives a read/write cycle unchanged. `Scope` is a sum
/// type in memory but still three optional TOML keys on disk; this pins the mapping in
/// both directions.
#[test]
fn every_override_scope_round_trips() {
    let content = concat!(
        "[actions]\n",
        "\"actions/checkout\" = \"^4\"\n",
        "\n",
        "[actions.overrides]\n",
        "\"actions/checkout\" = [\n",
        "  { workflow = \".github/workflows/ci.yml\", version = \"^1\" },\n",
        "  { workflow = \".github/workflows/ci.yml\", job = \"build\", version = \"^2\" },\n",
        "  { workflow = \".github/workflows/ci.yml\", job = \"test\", step = 2, version = \"^3\" },\n",
        "  { workflow = \".github/actions/setup/action.yml\", step = 0, version = \"^5\" },\n",
        "]\n",
    );
    let mut file = NamedTempFile::new().unwrap();
    file.write_all(content.as_bytes()).unwrap();

    let loaded = parse(file.path()).unwrap();
    let overrides = loaded
        .value
        .overrides_for(&ActionId::from("actions/checkout"));

    assert_eq!(
        overrides
            .iter()
            .map(|o| o.scope.clone())
            .collect::<Vec<_>>(),
        vec![
            Scope::File,
            Scope::Job {
                job: JobId::from("build")
            },
            Scope::JobStep {
                job: JobId::from("test"),
                step: StepIndex::from(2_u16),
            },
            Scope::CompositeStep {
                step: StepIndex::from(0_u16)
            },
        ]
    );

    // Write it back out and read it again: the on-disk keys must survive the sum type.
    let out = NamedTempFile::new().unwrap();
    Store::new(out.path()).save(&loaded.value).unwrap();
    let reloaded = parse(out.path()).unwrap();

    assert_eq!(
        reloaded
            .value
            .overrides_for(&ActionId::from("actions/checkout")),
        overrides,
        "an override's scope must survive a write/read cycle"
    );
}
