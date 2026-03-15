use super::Error as TidyError;
use crate::domain::action::identity::ActionId;
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
    let mut by_location: HashMap<String, Vec<&LocatedAction>> = HashMap::new();
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
        let file_map = build_file_update_map(manifest, lock, steps);
        if !file_map.is_empty() {
            let pins: Vec<(ActionId, String)> = file_map.into_iter().collect();
            patches.push(WorkflowPatch {
                path: workflow_path.clone(),
                pins,
            });
        }
    }

    Ok(patches)
}

/// Build the per-file update map: resolves each step's version via override hierarchy.
fn build_file_update_map(
    manifest: &Manifest,
    lock: &Lock,
    steps: &[&LocatedAction],
) -> HashMap<ActionId, String> {
    let mut map: HashMap<ActionId, String> = HashMap::new();
    for action in steps {
        if let Some(version) = manifest.resolve_version(&action.action.id, &action.location) {
            let key = Spec::new(action.action.id.clone(), version.clone());
            if let Some((res, commit)) = lock.get(&key) {
                // Omit comment when resolved version is a raw SHA
                let workflow_ref = if version.is_sha() {
                    commit.sha.to_string()
                } else {
                    format!("{} # {}", commit.sha, res.comment)
                };
                map.insert(action.action.id.clone(), workflow_ref);
            }
        }
    }
    map
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    clippy::get_unwrap,
    reason = "tests use unwrap, indexing, and other patterns freely"
)]
mod tests {
    use super::{Lock, Manifest, build_file_update_map};
    use crate::domain::action::identity::{ActionId, CommitSha, Version};
    use crate::domain::action::resolved::Commit;
    use crate::domain::action::spec::Spec;
    use crate::domain::action::specifier::Specifier;
    use crate::domain::action::uses_ref::RefType;
    use crate::domain::workflow_actions::{Location as WorkflowLocation, StepIndex};

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
                repository: "actions/checkout".to_owned(),
                ref_type: Some(RefType::Tag),
                date: "2026-01-01T00:00:00Z".to_owned(),
            },
            String::new(),
        );

        // A located action referencing this action
        let located = crate::domain::workflow_actions::Located {
            action: crate::domain::action::uses_ref::InterpretedRef {
                id: ActionId::from("actions/checkout"),
                version: Version::from(sha),
                sha: Some(CommitSha::from(sha)),
            },
            location: WorkflowLocation {
                workflow: ".github/workflows/ci.yml".to_owned(),
                job: Some("build".to_owned()),
                step: Some(StepIndex::from(0_u16)),
            },
        };

        let map = build_file_update_map(&manifest, &lock, &[&located]);

        let workflow_ref = map.get(&ActionId::from("actions/checkout")).unwrap();
        // Must be just the SHA, no "# SHA" comment
        assert_eq!(
            workflow_ref, sha,
            "SHA-only version must produce @SHA without trailing # comment"
        );
        assert!(
            !workflow_ref.contains('#'),
            "SHA-only version must not have a # comment"
        );
    }
}
