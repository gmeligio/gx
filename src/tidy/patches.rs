use super::Error as TidyError;
use crate::domain::action::identity::ActionId;
use crate::domain::action::resolved::ResolvedAction;
use crate::domain::action::spec::Spec;
use crate::domain::lock::Lock;
use crate::domain::manifest::Manifest;
use crate::domain::plan::WorkflowPatch;
use crate::domain::workflow::Scanner as WorkflowScanner;
use crate::domain::workflow_actions::Located as LocatedAction;
use std::collections::HashMap;

/// Compute workflow patches (pin maps) without writing files.
///
/// # Errors
///
/// Returns [`TidyError::Workflow`] if workflow paths cannot be listed.
pub(super) fn compute_workflow_patches<P: WorkflowScanner>(
    located: &[LocatedAction],
    manifest: &Manifest,
    lock: &Lock,
    scanner: &P,
) -> Result<Vec<WorkflowPatch>, TidyError> {
    let mut by_location: HashMap<
        crate::domain::workflow_actions::WorkflowPath,
        Vec<&LocatedAction>,
    > = HashMap::new();
    for action in located {
        by_location
            .entry(action.location.workflow.clone())
            .or_default()
            .push(action);
    }

    let workflows = scanner.find_workflow_paths()?;
    let mut patches = Vec::new();

    for workflow_path in &workflows {
        let abs_str = workflow_path.to_string_lossy().replace('\\', "/");
        let steps: &[&LocatedAction] = by_location
            .iter()
            .find(|(loc, _)| abs_str.ends_with(loc.as_str()))
            .map_or(&[], |(_, steps)| steps.as_slice());
        let pins = build_pins(manifest, lock, steps);
        if !pins.is_empty() {
            patches.push(WorkflowPatch {
                path: workflow_path.clone(),
                pins,
            });
        }
    }

    Ok(patches)
}

/// Build the per-file pins: resolves each step's version via override hierarchy.
fn build_pins(manifest: &Manifest, lock: &Lock, steps: &[&LocatedAction]) -> Vec<ResolvedAction> {
    let mut map = HashMap::<ActionId, ResolvedAction>::new();
    for action in steps {
        if let Some(version) = manifest.resolve_version(&action.action.id, &action.location) {
            let key = Spec::new(action.action.id.clone(), version.clone());
            if let Some((res, commit)) = lock.get(&key) {
                map.insert(
                    action.action.id.clone(),
                    ResolvedAction {
                        id: action.action.id.clone(),
                        sha: commit.sha.clone(),
                        version: if version.is_sha() {
                            None
                        } else {
                            Some(res.version.clone())
                        },
                    },
                );
            }
        }
    }
    map.into_values().collect()
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    clippy::get_unwrap,
    reason = "tests use unwrap, indexing, and other patterns freely"
)]
mod tests {
    use super::{Lock, Manifest, build_pins};
    use crate::domain::action::identity::{ActionId, CommitDate, CommitSha, Repository, Version};
    use crate::domain::action::resolved::Commit;
    use crate::domain::action::spec::Spec;
    use crate::domain::action::specifier::Specifier;
    use crate::domain::action::uses_ref::RefType;
    use crate::domain::workflow_actions::{
        JobId, Location as WorkflowLocation, StepIndex, WorkflowPath,
    };

    /// Task 4.2: SHA-only manifest version produces `@SHA` without trailing
    /// `# SHA` comment in workflow output.
    #[test]
    fn sha_only_version_no_trailing_comment() {
        let sha = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

        // Manifest has SHA as version
        let mut manifest = Manifest::default();
        manifest.set(ActionId::from("actions/checkout"), Specifier::from_v1(sha));

        // Lock has an entry for this SHA version
        let spec = Spec::new(ActionId::from("actions/checkout"), Specifier::from_v1(sha));
        let mut lock = Lock::default();
        lock.set(
            &spec,
            Version::from(sha),
            Commit {
                sha: CommitSha::from(sha),
                repository: Repository::from("actions/checkout"),
                ref_type: Some(RefType::Tag),
                date: CommitDate::from("2026-01-01T00:00:00Z"),
            },
        );

        // A located action referencing this action
        let located = crate::domain::workflow_actions::Located {
            action: crate::domain::action::uses_ref::InterpretedRef {
                id: ActionId::from("actions/checkout"),
                version: Version::from(sha),
                sha: Some(CommitSha::from(sha)),
            },
            location: WorkflowLocation {
                workflow: WorkflowPath::new(".github/workflows/ci.yml"),
                job: Some(JobId::from("build")),
                step: Some(StepIndex::from(0_u16)),
            },
        };

        let pins = build_pins(&manifest, &lock, &[&located]);

        let pin = pins
            .iter()
            .find(|p| p.id == ActionId::from("actions/checkout"))
            .unwrap();
        // Must be just the SHA, no version annotation
        assert_eq!(
            pin.sha.as_str(),
            sha,
            "SHA-only version must produce @SHA without trailing # comment"
        );
        assert!(
            pin.version.is_none(),
            "SHA-only version must not have a version annotation"
        );
    }
}
