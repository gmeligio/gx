use super::Error as TidyError;
use crate::domain::action::identity::ActionId;
use crate::domain::action::resolved::ResolvedAction;
use crate::domain::action::spec::Spec;
use crate::domain::diff::WorkflowPatch;
use crate::domain::file::actions::Located as LocatedAction;
use crate::domain::file::scan::Scanner as WorkflowScanner;
use crate::domain::lock::Lock;
use crate::domain::manifest::Manifest;
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
    let mut by_file: HashMap<crate::domain::file::site::WorkflowPath, Vec<&LocatedAction>> =
        HashMap::new();
    for action in located {
        by_file
            .entry(action.site.file.clone())
            .or_default()
            .push(action);
    }

    let workflows = scanner.find_workflow_paths()?;
    let mut patches = Vec::new();

    for workflow_path in &workflows {
        // Exact key, not a suffix match: when one managed file's path ends with
        // another's, a suffix match over an unordered map pairs a file with the wrong
        // file's pins, and which one wins varies between runs.
        let steps: &[&LocatedAction] = by_file
            .get(&scanner.repo_rel(workflow_path))
            .map_or(&[], Vec::as_slice);
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
        if let Some(version) = manifest.resolve_version(&action.action.id, &action.site) {
            let key = Spec::new(action.action.id.clone(), version.clone());
            if let Some(entry) = lock.get(&key) {
                map.insert(
                    action.action.id.clone(),
                    ResolvedAction {
                        id: action.action.id.clone(),
                        sha: entry.commit.sha.clone(),
                        // The lock entry knows its own kind: a bare commit pin
                        // carries no `# comment` annotation.
                        version: entry.reference.annotation().cloned(),
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
    reason = "tests use unwrap, indexing, and other patterns freely"
)]
mod tests {
    use super::{Lock, Manifest, build_pins};
    use crate::domain::action::identity::{ActionId, CommitDate, CommitSha, Repository};
    use crate::domain::action::resolved::{Commit, ResolvedRef};
    use crate::domain::action::spec::Spec;
    use crate::domain::action::specifier::Specifier;
    use crate::domain::action::uses_ref::RefType;
    use crate::domain::file::site::Id as WorkflowLocation;
    use crate::domain::file::site::{JobId, StepIndex, WorkflowPath};
    use crate::domain::file::site::{Origin, Slot};

    /// Task 4.2: SHA-only manifest version produces `@SHA` without trailing
    /// `# SHA` comment in workflow output.
    #[test]
    fn sha_only_version_no_trailing_comment() {
        let sha = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

        // Manifest has SHA as version
        let mut manifest = Manifest::default();
        manifest.set(ActionId::from("actions/checkout"), Specifier::from_v1(sha));

        // Lock has a bare commit pin for this SHA (no version label).
        let spec = Spec::new(ActionId::from("actions/checkout"), Specifier::from_v1(sha));
        let mut lock = Lock::default();
        lock.set(
            &spec,
            ResolvedRef::Commit,
            Commit {
                sha: CommitSha::from(sha),
                repository: Repository::from("actions/checkout"),
                ref_type: Some(RefType::Commit),
                date: CommitDate::from("2026-01-01T00:00:00Z"),
            },
        );

        // A located action referencing this action by bare SHA (`@<sha>`).
        let located = crate::domain::file::actions::Located {
            action: crate::domain::file::actions::WorkflowAction {
                id: ActionId::from("actions/checkout"),
                reference: crate::domain::action::uses_ref::ParsedRef::Sha(CommitSha::from(sha)),
            },
            site: WorkflowLocation {
                file: WorkflowPath::new(".github/workflows/ci.yml"),
                slot: Slot::WorkflowStep {
                    job: JobId::from("build"),
                    step: StepIndex::from(0_u16),
                },
            },
            origin: Origin::default(),
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
