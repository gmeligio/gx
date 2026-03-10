mod lock_sync;
mod manifest_sync;
mod patches;
pub mod report;

use crate::command::Command;
use crate::config::Config;
use crate::domain::{
    ActionId, ActionResolver, CommitSha, Lock, LockDiff, LockKey, Manifest, ManifestDiff, ShaIndex,
    VersionCorrection, VersionRegistry, WorkflowActionSet, WorkflowError, WorkflowPatch,
    WorkflowScanner, WorkflowUpdater,
};
use crate::infra::{
    FileWorkflowScanner, FileWorkflowUpdater, GithubError, GithubRegistry, LockFileError,
    ManifestError, apply_lock_diff, apply_manifest_diff, create_lock,
};
use report::TidyReport;
use std::collections::HashMap;
use std::path::Path;
use thiserror::Error;

/// The complete plan produced by a tidy operation.
#[derive(Debug, Default)]
pub struct TidyPlan {
    pub manifest: ManifestDiff,
    pub lock: LockDiff,
    pub workflows: Vec<WorkflowPatch>,
    pub corrections: Vec<VersionCorrection>,
}

impl TidyPlan {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.manifest.is_empty() && self.lock.is_empty() && self.workflows.is_empty()
    }
}

/// Errors that can occur during the tidy command
#[derive(Debug, Error)]
pub enum TidyError {
    /// One or more actions could not be resolved to a commit SHA.
    #[error("failed to resolve {count} action(s):\n  {specs}")]
    ResolutionFailed { count: usize, specs: String },

    /// Workflow files could not be scanned or updated.
    #[error(transparent)]
    Workflow(#[from] WorkflowError),
}

/// Compute a `TidyPlan` describing all changes without modifying the original manifest or lock.
///
/// Internally, this clones the manifest/lock and runs the same mutation logic, then diffs
/// the before/after state to produce the plan.
///
/// # Errors
///
/// Returns [`TidyError::Workflow`] if workflows cannot be scanned.
/// Returns [`TidyError::ResolutionFailed`] if actions cannot be resolved.
pub fn plan<R, P>(
    manifest: &Manifest,
    lock: &Lock,
    registry: &R,
    scanner: &P,
    mut on_progress: impl FnMut(&str),
) -> Result<TidyPlan, TidyError>
where
    R: VersionRegistry,
    P: WorkflowScanner,
{
    let mut located = Vec::new();
    let mut action_set = WorkflowActionSet::new();
    for result in scanner.scan() {
        let action = result?;
        action_set.add_located(&action);
        located.push(action);
    }
    if located.is_empty() {
        return Ok(TidyPlan::default());
    }

    // Work on clones to compute the planned state
    let mut planned_manifest = manifest.clone();
    let mut planned_lock = lock.clone();

    let resolver = ActionResolver::new(registry);
    let mut sha_index = ShaIndex::new();

    // Phase 1: Sync manifest
    let sync_events = manifest_sync::sync_manifest_actions(
        &mut planned_manifest,
        &located,
        &action_set,
        &resolver,
        &mut sha_index,
    );
    for event in &sync_events {
        on_progress(&event.to_string());
    }
    let upgrade_events = manifest_sync::upgrade_sha_versions_to_tags(
        &mut planned_manifest,
        &resolver,
        &mut sha_index,
    );
    for event in &upgrade_events {
        on_progress(&event.to_string());
    }

    // Phase 2: Sync overrides
    planned_manifest.sync_overrides(&located, &action_set);
    planned_manifest.prune_stale_overrides(&located);

    // Build SHA map: workflow SHA for each (action, manifest_version) pair
    let workflow_shas: HashMap<LockKey, CommitSha> = located
        .iter()
        .filter_map(|loc| {
            let sha = loc.sha.as_ref()?;
            let manifest_version = planned_manifest.get(&loc.id)?;
            let key = LockKey::new(loc.id.clone(), manifest_version.clone());
            Some((key, sha.clone()))
        })
        .collect();

    // Phase 3: Resolve lock
    let (corrections, lock_events) = lock_sync::update_lock(
        &mut planned_lock,
        &mut planned_manifest,
        &resolver,
        &workflow_shas,
        &mut sha_index,
    )?;
    for event in &lock_events {
        on_progress(&event.to_string());
    }
    let keys_to_retain = planned_manifest.lock_keys();
    planned_lock.retain(&keys_to_retain);

    // Phase 4: Compute workflow patches (instead of writing files)
    let workflow_patches =
        patches::compute_workflow_patches(&located, &planned_manifest, &planned_lock, scanner)?;

    // Diff original vs planned to produce the plan
    let manifest_diff = manifest.diff(&planned_manifest);
    let lock_diff = lock.diff(&planned_lock);

    Ok(TidyPlan {
        manifest: manifest_diff,
        lock: lock_diff,
        workflows: workflow_patches,
        corrections,
    })
}

/// Apply workflow patches: write pin changes to workflow files and log results.
///
/// # Errors
///
/// Returns [`TidyError::Workflow`] if any workflow file cannot be updated.
pub fn apply_workflow_patches<W: WorkflowUpdater>(
    writer: &W,
    patches: &[WorkflowPatch],
    corrections: &[VersionCorrection],
) -> Result<usize, TidyError> {
    let mut results = Vec::new();
    for patch in patches {
        let map: HashMap<ActionId, String> = patch.pins.iter().cloned().collect();
        let result = writer.update_file(&patch.path, &map)?;
        if !result.changes.is_empty() {
            results.push(result);
        }
    }
    let _ = corrections;
    Ok(results.len())
}

/// Errors that can occur during the tidy command's run phase (I/O + domain)
#[derive(Debug, thiserror::Error)]
pub enum TidyRunError {
    #[error(transparent)]
    Github(#[from] GithubError),
    #[error(transparent)]
    Manifest(#[from] ManifestError),
    #[error(transparent)]
    Lock(#[from] LockFileError),
    #[error(transparent)]
    Tidy(#[from] TidyError),
}

/// The tidy command struct.
pub struct Tidy;

impl Command for Tidy {
    type Report = TidyReport;
    type Error = TidyRunError;

    fn run(
        &self,
        repo_root: &Path,
        config: Config,
        on_progress: &mut dyn FnMut(&str),
    ) -> Result<TidyReport, TidyRunError> {
        let has_manifest = config.manifest_path.exists();
        if config.manifest_migrated {
            on_progress("migrated gx.toml → semver specifiers");
        }
        if config.lock_migrated {
            on_progress("migrated gx.lock → v1.4");
        }
        if config.settings.github_token.is_none() {
            on_progress(
                "Warning: No GITHUB_TOKEN set — using unauthenticated GitHub API (60 requests/hour limit).",
            );
        }
        let registry = GithubRegistry::new(config.settings.github_token)?;
        let scanner = FileWorkflowScanner::new(repo_root);
        let updater = FileWorkflowUpdater::new(repo_root);

        let original_manifest = config.manifest.clone();

        let tidy_plan = plan(
            &config.manifest,
            &config.lock,
            &registry,
            &scanner,
            on_progress,
        )?;

        if tidy_plan.is_empty() {
            return Ok(TidyReport::default());
        }

        if has_manifest {
            apply_manifest_diff(&config.manifest_path, &tidy_plan.manifest)?;
            if config.lock_path.exists() {
                apply_lock_diff(&config.lock_path, &tidy_plan.lock)?;
            } else {
                create_lock(&config.lock_path, &tidy_plan.lock)?;
            }
        }

        let workflows_updated =
            apply_workflow_patches(&updater, &tidy_plan.workflows, &tidy_plan.corrections)?;

        let report = TidyReport {
            removed: tidy_plan
                .manifest
                .removed
                .iter()
                .map(std::string::ToString::to_string)
                .collect(),
            added: tidy_plan
                .manifest
                .added
                .iter()
                .map(|(id, v)| (id.to_string(), v.to_string()))
                .collect(),
            upgraded: tidy_plan
                .manifest
                .updated
                .iter()
                .map(|(id, new_v)| {
                    let old_v = original_manifest.get(id).map_or_else(
                        || {
                            // Fallback: use new version as "from" if original not found
                            let _ = LockKey::new(id.clone(), new_v.clone());
                            "?".to_string()
                        },
                        std::string::ToString::to_string,
                    );
                    (id.to_string(), old_v, new_v.to_string())
                })
                .collect(),
            corrections: tidy_plan.corrections.len(),
            workflows_updated,
        };

        Ok(report)
    }
}

#[cfg(test)]
mod tests;
