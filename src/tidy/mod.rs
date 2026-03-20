/// Lock file synchronization: resolving and updating lock entries.
mod lock_sync;
/// Manifest synchronization: adding, removing, and upgrading action specs.
mod manifest_sync;
/// Workflow patch computation for updating pinned SHAs in workflow files.
mod patches;
pub mod report;

use crate::command::Command;
use crate::config::Config;
use crate::domain::action::identity::CommitSha;
use crate::domain::action::spec::Spec;
use crate::domain::action::tag_selection::ShaIndex;
use crate::domain::lock::Lock;
use crate::domain::manifest::Manifest;
use crate::domain::plan::{LockDiff, ManifestDiff, WorkflowPatch};
use crate::domain::resolution::{ActionResolver, VersionRegistry};
use crate::domain::workflow::{Error as WorkflowError, Scanner as WorkflowScanner};
use crate::domain::workflow_actions::ActionSet as WorkflowActionSet;
use crate::infra::github::{Error as GithubError, Registry as GithubRegistry};
use crate::infra::lock::{Error as LockFileError, Store as LockStore};
use crate::infra::manifest::Error as ManifestError;
use crate::infra::manifest::patch::apply_manifest_diff;
use crate::infra::workflow_scan::FileScanner as FileWorkflowScanner;
use crate::infra::workflow_update::WorkflowWriter;
use report::Report;
use std::collections::HashMap;
use std::path::Path;
use thiserror::Error;

/// The complete plan produced by a tidy operation.
#[derive(Debug, Default)]
pub struct Plan {
    pub manifest: ManifestDiff,
    /// The final lock state — written by `Store::save()`.
    pub lock: Lock,
    /// The diff between the original and planned lock — for reporting only.
    pub lock_changes: LockDiff,
    pub workflows: Vec<WorkflowPatch>,
}

impl Plan {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.manifest.is_empty() && self.lock_changes.is_empty() && self.workflows.is_empty()
    }
}

/// Errors that can occur during the tidy command.
#[derive(Debug, Error)]
pub enum Error {
    /// One or more actions could not be resolved to a commit SHA.
    #[error("failed to resolve {count} action(s):\n  {specs}")]
    ResolutionFailed { count: usize, specs: String },

    /// Workflow files could not be scanned or updated.
    #[error(transparent)]
    Workflow(#[from] WorkflowError),
}

/// Compute a `Plan` describing all changes without modifying the original manifest or lock.
///
/// Internally, this clones the manifest/lock and runs the same mutation logic, then diffs
/// the before/after state to produce the plan.
///
/// # Errors
///
/// Returns [`Error::Workflow`] if workflows cannot be scanned.
/// Returns [`Error::ResolutionFailed`] if actions cannot be resolved.
pub fn plan<R, P, F>(
    manifest: &Manifest,
    lock: &Lock,
    registry: &R,
    scanner: &P,
    mut on_progress: F,
) -> Result<Plan, Error>
where
    F: FnMut(&str),
    R: VersionRegistry,
    P: WorkflowScanner,
{
    let mut located = Vec::new();
    let mut action_set = WorkflowActionSet::new();
    for result in scanner.scan() {
        let action = result?;
        action_set.add(&action.action);
        located.push(action);
    }
    if located.is_empty() {
        return Ok(Plan::default());
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
    let workflow_shas: HashMap<Spec, CommitSha> = located
        .iter()
        .filter_map(|loc| {
            let sha = loc.action.sha.as_ref()?;
            let manifest_version = planned_manifest.get(&loc.action.id)?;
            let key = Spec::new(loc.action.id.clone(), manifest_version.clone());
            Some((key, sha.clone()))
        })
        .collect();

    // Phase 3: Resolve lock
    let lock_events = lock_sync::update_lock(
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

    Ok(Plan {
        manifest: manifest_diff,
        lock: planned_lock,
        lock_changes: lock_diff,
        workflows: workflow_patches,
    })
}

/// Apply workflow patches: write pin changes to workflow files and log results.
///
/// # Errors
///
/// Returns [`Error::Workflow`] if any workflow file cannot be updated.
pub fn apply_workflow_patches(
    writer: &WorkflowWriter,
    patches: &[WorkflowPatch],
) -> Result<usize, Error> {
    let results = writer.apply_patches(patches)?;
    Ok(results.len())
}

/// Errors that can occur during the tidy command's run phase (I/O + domain).
#[derive(Debug, thiserror::Error)]
pub enum RunError {
    #[error(transparent)]
    Github(#[from] GithubError),
    #[error(transparent)]
    Manifest(#[from] ManifestError),
    #[error(transparent)]
    Lock(#[from] LockFileError),
    #[error(transparent)]
    Tidy(#[from] Error),
}

/// The tidy command struct.
pub struct Tidy;

impl Command for Tidy {
    type Report = Report;
    type Error = RunError;

    fn run(
        &self,
        repo_root: &Path,
        config: Config,
        on_progress: &mut dyn FnMut(&str),
    ) -> Result<Report, RunError> {
        let has_manifest = config.manifest_path.exists();
        if config.manifest_migrated {
            on_progress("migrated gx.toml → semver specifiers");
        }
        if config.settings.github_token.is_none() {
            on_progress(
                "Warning: No GITHUB_TOKEN set — using unauthenticated GitHub API (60 requests/hour limit).",
            );
        }
        let registry = GithubRegistry::new(config.settings.github_token)?;
        let scanner = FileWorkflowScanner::new(repo_root);
        let updater = WorkflowWriter::new(repo_root);

        let original_manifest = config.manifest.clone();

        let tidy_plan = plan(
            &config.manifest,
            &config.lock,
            &registry,
            &scanner,
            on_progress,
        )?;

        if tidy_plan.is_empty() {
            return Ok(Report::default());
        }

        if has_manifest {
            apply_manifest_diff(&config.manifest_path, &tidy_plan.manifest)?;
            let lock_store = LockStore::new(&config.lock_path);
            lock_store.save(&tidy_plan.lock)?;
        }

        let workflows_updated = apply_workflow_patches(&updater, &tidy_plan.workflows)?;

        let report = Report {
            removed: tidy_plan.manifest.removed,
            added: tidy_plan.manifest.added,
            upgraded: tidy_plan
                .manifest
                .updated
                .into_iter()
                .map(|(id, new_v)| {
                    let old_v = original_manifest
                        .get(&id)
                        .map_or_else(|| "?".to_owned(), std::string::ToString::to_string);
                    (id, old_v, new_v)
                })
                .collect(),
            workflows_updated,
        };

        Ok(report)
    }
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    reason = "tests use unwrap, indexing, and other patterns freely"
)]
mod tests;
