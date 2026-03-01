use log::{debug, info};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use thiserror::Error;

use crate::domain::{
    ActionId, ActionResolver, ActionSpec, LocatedAction, Lock, LockKey, Manifest, ResolutionResult,
    UpdateResult, Version, VersionCorrection, VersionRegistry, WorkflowActionSet, WorkflowScanner,
    WorkflowScannerLocated, WorkflowUpdater,
};
use crate::infrastructure::{LockFileError, ManifestError, WorkflowError};

/// Errors that can occur during the tidy command
#[derive(Debug, Error)]
pub enum TidyError {
    /// One or more actions could not be resolved to a commit SHA.
    #[error("failed to resolve {count} action(s):\n  {specs}")]
    ResolutionFailed { count: usize, specs: String },

    /// The manifest store failed to save.
    #[error(transparent)]
    Manifest(#[from] ManifestError),

    /// The lock store failed to save.
    #[error(transparent)]
    Lock(#[from] LockFileError),

    /// Workflow files could not be scanned or updated.
    #[error(transparent)]
    Workflow(#[from] WorkflowError),
}

/// Run the tidy command: synchronize manifest and lock with workflows, then update workflow files.
///
/// # Errors
///
/// Returns [`TidyError::Workflow`] if workflows cannot be scanned or updated.
/// Returns [`TidyError::ResolutionFailed`] if actions cannot be resolved.
///
/// # Panics
///
/// Panics if an action in the intersection of workflow and manifest is not found in the manifest.
#[allow(clippy::too_many_arguments, clippy::needless_pass_by_value)]
pub fn run<R, P, W>(
    mut manifest: Manifest,
    mut lock: Lock,
    manifest_path: &Path,
    registry: R,
    parser: &P,
    writer: &W,
) -> Result<(Manifest, Lock), TidyError>
where
    R: VersionRegistry,
    P: WorkflowScanner + WorkflowScannerLocated,
    W: WorkflowUpdater,
{
    let action_set = parser.scan_all()?;
    if action_set.is_empty() {
        return Ok((manifest, lock));
    }

    let workflow_actions: HashSet<ActionId> = action_set.action_ids().into_iter().collect();
    let manifest_actions: HashSet<ActionId> =
        manifest.specs().iter().map(|s| s.id.clone()).collect();

    // Remove unused actions from manifest
    let unused: Vec<_> = manifest_actions.difference(&workflow_actions).collect();
    if !unused.is_empty() {
        info!("Removing unused actions from manifest:");
        for action in &unused {
            info!("- {action}");
            manifest.remove(action);
        }
    }

    // Add missing actions to manifest
    let missing: Vec<_> = workflow_actions.difference(&manifest_actions).collect();
    if !missing.is_empty() {
        info!("Adding missing actions to manifest:");
        for action_id in &missing {
            let version = select_dominant_version(action_id, &action_set);
            manifest.set((*action_id).clone(), version.clone());
            let spec = ActionSpec::new((*action_id).clone(), version.clone());
            info!("+ {spec}");
        }
    }

    // Update existing actions if manifest has SHA but workflow has tag
    let existing: Vec<_> = workflow_actions.intersection(&manifest_actions).collect();
    if !existing.is_empty() {
        let mut updated_actions = Vec::new();
        for action_id in &existing {
            let versions = action_set.versions_for(action_id);
            if versions.len() == 1 {
                let workflow_version = &versions[0];
                let manifest_version = manifest
                    .get(action_id)
                    .expect("action_id is from intersection with manifest_actions")
                    .clone();
                if manifest_version.should_be_replaced_by(workflow_version) {
                    manifest.set((*action_id).clone(), workflow_version.clone());
                    let spec = ActionSpec::new((*action_id).clone(), workflow_version.clone());
                    updated_actions.push(format!("{spec} (was {manifest_version})"));
                }
            }
        }
        if !updated_actions.is_empty() {
            info!("Updating action versions in manifest:");
            for update in &updated_actions {
                info!("~ {update}");
            }
        }
    }

    // Scan with location context for per-step processing (also used for stale cleanup)
    let located = parser.scan_all_located()?;

    // Sync overrides: when multiple distinct versions of the same action appear across
    // workflows, record overrides for steps whose version differs from the manifest global.
    // Only applies to actions with multiple workflow versions — if the manifest has v4 but
    // every workflow step uses v3, the manifest wins and no override is needed.
    sync_overrides(&mut manifest, &located, &action_set);

    // Remove stale override entries (overrides pointing to removed workflows/jobs/steps)
    prune_stale_overrides(&mut manifest, &located);

    // Resolve SHAs and validate version comments (must handle all versions in manifest + overrides)
    let corrections = update_lock(&mut lock, &mut manifest, &action_set, registry)?;

    // Prune lock: retain all version variants that appear in manifest or overrides
    let keys_to_retain: Vec<LockKey> = build_keys_to_retain(&manifest);
    lock.retain(&keys_to_retain);

    if manifest.is_empty() {
        info!("No actions found in {}", manifest_path.display());
        return Ok((manifest, lock));
    }

    // Group located steps by workflow location path
    let mut by_location: HashMap<String, Vec<&LocatedAction>> = HashMap::new();
    for action in &located {
        by_location
            .entry(action.location.workflow.clone())
            .or_default()
            .push(action);
    }

    // For each workflow file, find matching located steps and update
    let workflows = parser.find_workflow_paths()?;
    let mut all_results: Vec<UpdateResult> = Vec::new();

    for workflow_path in &workflows {
        // Match absolute path against relative location paths (suffix match)
        let abs_str = workflow_path.to_string_lossy().replace('\\', "/");
        let steps: &[&LocatedAction] = by_location
            .iter()
            .find(|(loc, _)| abs_str.ends_with(loc.as_str()))
            .map_or(&[], |(_, steps)| steps.as_slice());
        let file_map = build_file_update_map(&manifest, &lock, steps);
        if !file_map.is_empty() {
            let result = writer.update_file(workflow_path, &file_map)?;
            if !result.changes.is_empty() {
                all_results.push(result);
            }
        }
    }
    print_update_results(&all_results);

    if !corrections.is_empty() {
        info!("Version corrections:");
        for c in &corrections {
            info!("{c}");
        }
    }

    Ok((manifest, lock))
}

fn select_version(versions: &[Version]) -> Version {
    Version::highest(versions).unwrap_or_else(|| versions[0].clone())
}

fn select_dominant_version(action_id: &ActionId, action_set: &WorkflowActionSet) -> Version {
    action_set.dominant_version(action_id).unwrap_or_else(|| {
        let versions = action_set.versions_for(action_id);
        select_version(&versions)
    })
}

/// Collect all `LockKeys` needed: one per (action, version) pair across globals and overrides.
fn build_keys_to_retain(manifest: &Manifest) -> Vec<LockKey> {
    let mut keys: Vec<LockKey> = manifest.specs().iter().map(|s| LockKey::from(*s)).collect();
    for (id, overrides) in manifest.all_overrides() {
        for exc in overrides {
            let key = LockKey::new(id.clone(), exc.version.clone());
            if !keys.contains(&key) {
                keys.push(key);
            }
        }
    }
    keys
}

/// Build the per-file update map: resolves each step's version via override hierarchy.
fn build_file_update_map(
    manifest: &Manifest,
    lock: &Lock,
    steps: &[&LocatedAction],
) -> HashMap<ActionId, String> {
    let mut map: HashMap<ActionId, String> = HashMap::new();
    for action in steps {
        if let Some(version) = manifest.resolve_version(&action.id, &action.location) {
            let key = LockKey::new(action.id.clone(), version.clone());
            if let Some(entry) = lock.get(&key) {
                let workflow_ref = format!("{} # {}", entry.sha, version);
                map.insert(action.id.clone(), workflow_ref);
            }
        }
    }
    map
}

/// Ensure overrides exist for every located step whose version differs from the manifest global,
/// **only when** multiple distinct versions of that action appear across workflows.
///
/// When only one version appears in workflows (manifest is the authority), no override is created.
/// When multiple versions coexist (e.g. `v5` in windows.yml, `v6.0.1` everywhere else),
/// the minority versions are recorded as overrides so tidy does not overwrite them.
fn sync_overrides(
    manifest: &mut Manifest,
    located: &[LocatedAction],
    action_set: &WorkflowActionSet,
) {
    for action in located {
        // Only create overrides when multiple distinct versions exist in workflows
        let versions = action_set.versions_for(&action.id);
        if versions.len() <= 1 {
            continue;
        }

        let global_version = match manifest.get(&action.id) {
            Some(v) => v.clone(),
            None => continue,
        };

        // Skip steps already matching the global version
        if action.version == global_version {
            continue;
        }

        // Check if an override already covers this exact location
        let already_covered = manifest.overrides_for(&action.id).iter().any(|o| {
            o.workflow == action.location.workflow
                && o.job == action.location.job
                && o.step == action.location.step
        });

        if !already_covered {
            info!(
                "Recording override for {} in {} ({})",
                action.id, action.location.workflow, action.version,
            );
            manifest.add_override(
                action.id.clone(),
                crate::domain::ActionOverride {
                    workflow: action.location.workflow.clone(),
                    job: action.location.job.clone(),
                    step: action.location.step,
                    version: action.version.clone(),
                },
            );
        }
    }
}

/// Remove override entries whose referenced workflow/job/step no longer exists in the scanned set.
fn prune_stale_overrides(manifest: &mut Manifest, located: &[LocatedAction]) {
    let live_workflows: HashSet<&str> = located
        .iter()
        .map(|a| a.location.workflow.as_str())
        .collect();

    let action_ids: Vec<ActionId> = manifest.all_overrides().keys().cloned().collect();

    for id in action_ids {
        let overrides = manifest.overrides_for(&id).to_vec();
        let pruned: Vec<crate::domain::ActionOverride> = overrides
            .into_iter()
            .filter(|exc| {
                if !live_workflows.contains(exc.workflow.as_str()) {
                    info!("Removing stale override for {id} in {}", exc.workflow);
                    return false;
                }
                if let Some(job) = &exc.job {
                    let job_exists = located.iter().any(|a| {
                        a.location.workflow == exc.workflow
                            && a.location.job.as_deref() == Some(job.as_str())
                    });
                    if !job_exists {
                        info!(
                            "Removing stale override for {id} in {}/{}",
                            exc.workflow, job
                        );
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
                        info!(
                            "Removing stale override for {id} in {}:{}/{}",
                            exc.workflow, job, step
                        );
                        return false;
                    }
                }
                true
            })
            .collect();

        manifest.replace_overrides(id, pruned);
    }
}

/// # Errors
///
/// Returns [`TidyError::ResolutionFailed`] if any actions could not be resolved.
fn update_lock<R: VersionRegistry>(
    lock: &mut Lock,
    manifest: &mut Manifest,
    action_set: &WorkflowActionSet,
    registry: R,
) -> Result<Vec<VersionCorrection>, TidyError> {
    let mut corrections = Vec::new();
    let mut unresolved = Vec::new();

    // Collect all specs: global + override versions
    let mut all_specs: Vec<ActionSpec> = manifest.specs().iter().map(|s| (*s).clone()).collect();
    for (id, overrides) in manifest.all_overrides() {
        for exc in overrides {
            let key = LockKey::new(id.clone(), exc.version.clone());
            if !lock.has(&key) {
                all_specs.push(ActionSpec::new(id.clone(), exc.version.clone()));
            }
        }
    }

    let needs_resolving = all_specs.iter().any(|spec| !lock.has(&LockKey::from(spec)));
    let has_workflow_shas = manifest
        .specs()
        .iter()
        .any(|spec| action_set.sha_for(&spec.id).is_some());

    if !needs_resolving && !has_workflow_shas {
        return Ok(corrections);
    }

    let resolver = ActionResolver::new(registry);

    // First handle globals with SHA validation/correction
    let specs: Vec<ActionSpec> = manifest.specs().iter().map(|s| (*s).clone()).collect();
    for spec in &specs {
        if let Some(workflow_sha) = action_set.sha_for(&spec.id) {
            // Two-step flow: correct version, then resolve with pinned SHA
            let (corrected_version, was_corrected) =
                resolver.correct_version(&spec.id, workflow_sha, &spec.version);

            if was_corrected {
                info!(
                    "Corrected {spec} version to {corrected_version} (SHA {workflow_sha} points to {corrected_version})",
                );
                corrections.push(VersionCorrection {
                    action: spec.id.clone(),
                    old_version: spec.version.clone(),
                    new_version: corrected_version.clone(),
                    sha: workflow_sha.clone(),
                });
                manifest.set(spec.id.clone(), corrected_version.clone());
            }

            let resolve_spec = ActionSpec::new(spec.id.clone(), corrected_version.clone());
            let resolve_key = LockKey::from(&resolve_spec);
            if !lock.has(&resolve_key) {
                match resolver.resolve(&resolve_spec) {
                    ResolutionResult::Resolved(action) => {
                        // Keep the workflow's pinned SHA (not the registry's current SHA)
                        let pinned_action = action.with_sha(workflow_sha.clone());
                        lock.set(&pinned_action);
                    }
                    ResolutionResult::Unresolved { spec: s, reason } => {
                        debug!("Could not resolve {s}: {reason}");
                        unresolved.push(format!("{s}: {reason}"));
                    }
                    ResolutionResult::Corrected { .. } => {
                        // This shouldn't happen in resolve(), but handle it
                        debug!("Unexpected Corrected result from resolve()");
                    }
                }
            }
        } else {
            let key = LockKey::from(spec);
            if !lock.has(&key) {
                debug!("Resolving {spec}");
                match resolver.resolve(spec) {
                    ResolutionResult::Resolved(action) => {
                        lock.set(&action);
                    }
                    ResolutionResult::Unresolved { spec: s, reason } => {
                        debug!("Could not resolve {s}: {reason}");
                        unresolved.push(format!("{s}: {reason}"));
                    }
                    ResolutionResult::Corrected { corrected, .. } => {
                        lock.set(&corrected);
                    }
                }
            }
        }
    }

    // Then resolve override versions (no SHA correction for overrides, just resolve)
    for (id, overrides) in manifest.all_overrides() {
        for exc in overrides {
            let exc_spec = ActionSpec::new(id.clone(), exc.version.clone());
            let key = LockKey::from(&exc_spec);
            if !lock.has(&key) {
                debug!("Resolving override {exc_spec}");
                match resolver.resolve(&exc_spec) {
                    ResolutionResult::Resolved(action) => {
                        lock.set(&action);
                    }
                    ResolutionResult::Unresolved { spec: s, reason } => {
                        debug!("Could not resolve override {s}: {reason}");
                        unresolved.push(format!("{s}: {reason}"));
                    }
                    ResolutionResult::Corrected { corrected, .. } => {
                        lock.set(&corrected);
                    }
                }
            }
        }
    }

    if !unresolved.is_empty() {
        return Err(TidyError::ResolutionFailed {
            count: unresolved.len(),
            specs: unresolved.join("\n  "),
        });
    }

    Ok(corrections)
}

fn print_update_results(results: &[UpdateResult]) {
    if results.is_empty() {
        info!("Workflows are already up to date.");
    } else {
        info!("Updated workflows:");
        for result in results {
            info!("{}", result.file.display());
            for change in &result.changes {
                info!("~ {change}");
            }
        }
        info!("{} workflow(s) updated.", results.len());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{ActionId, CommitSha, ResolvedAction};
    use crate::infrastructure::{
        FileLock, FileManifest, FileWorkflowScanner, FileWorkflowUpdater, parse_lock,
        parse_manifest,
    };
    use std::fs;

    #[test]
    fn tidy_error_resolution_failed_displays_specs() {
        let err = TidyError::ResolutionFailed {
            count: 2,
            specs: "actions/checkout: token required\n  actions/setup-node: timeout".to_string(),
        };
        assert_eq!(
            err.to_string(),
            "failed to resolve 2 action(s):\n  actions/checkout: token required\n  actions/setup-node: timeout"
        );
    }

    struct NoopRegistry;
    impl crate::domain::VersionRegistry for NoopRegistry {
        fn lookup_sha(
            &self,
            _id: &ActionId,
            _version: &Version,
        ) -> Result<crate::domain::ResolvedRef, crate::domain::ResolutionError> {
            Err(crate::domain::ResolutionError::TokenRequired)
        }
        fn tags_for_sha(
            &self,
            _id: &ActionId,
            _sha: &CommitSha,
        ) -> Result<Vec<Version>, crate::domain::ResolutionError> {
            Err(crate::domain::ResolutionError::TokenRequired)
        }
        fn all_tags(&self, _id: &ActionId) -> Result<Vec<Version>, crate::domain::ResolutionError> {
            Err(crate::domain::ResolutionError::TokenRequired)
        }
    }

    #[test]
    fn test_select_version_single() {
        let versions = vec![Version::from("v4")];
        assert_eq!(select_version(&versions), Version::from("v4"));
    }

    #[test]
    fn test_select_version_picks_highest() {
        let versions = vec![
            Version::from("v3"),
            Version::from("v4"),
            Version::from("v2"),
        ];
        assert_eq!(select_version(&versions), Version::from("v4"));
    }

    /// Bug #1 + #2: when workflows have a minority version (e.g. windows.yml uses
    /// `actions/checkout@v5` while all others use SHA-pinned `v6.0.1`), tidy must:
    ///   1. Record the minority version as an override in the manifest (Bug #1 / init)
    ///   2. Not overwrite windows.yml with the v6.0.1 SHA (Bug #2 / tidy)
    #[test]
    fn test_tidy_records_minority_version_as_override_and_does_not_overwrite_file() {
        // ---- Setup temp repo ----
        let temp_dir = tempfile::TempDir::new().unwrap();
        let repo_root = temp_dir.path();
        let workflows_dir = repo_root.join(".github").join("workflows");
        fs::create_dir_all(&workflows_dir).unwrap();
        let github_dir = repo_root.join(".github");

        // Most workflows: actions/checkout pinned to SHA with v6.0.1 comment
        let sha_workflow = "on: pull_request
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@8e8c483db84b4bee98b60c0593521ed34d9990e8 # v6.0.1
";
        fs::write(workflows_dir.join("ci.yml"), sha_workflow).unwrap();
        fs::write(workflows_dir.join("build.yml"), sha_workflow).unwrap();
        fs::write(workflows_dir.join("release.yml"), sha_workflow).unwrap();

        // windows.yml: plain tag @v5 (minority)
        let windows_workflow = "on: pull_request
jobs:
  test_windows:
    runs-on: windows-2025
    steps:
      - uses: actions/checkout@v5
";
        fs::write(workflows_dir.join("windows.yml"), windows_workflow).unwrap();

        // ---- Run tidy with empty manifest (simulates `gx init`) ----
        let manifest_path = github_dir.join("gx.toml");
        let lock_path = github_dir.join("gx.lock");

        // Pre-seed lock with both versions already resolved (simulates a pre-existing lock)
        let lock_store_seed = FileLock::new(&lock_path);
        let mut seed_lock = Lock::default();
        seed_lock.set(&ResolvedAction::new(
            ActionId::from("actions/checkout"),
            Version::from("v6.0.1"),
            CommitSha::from("8e8c483db84b4bee98b60c0593521ed34d9990e8"),
            "actions/checkout".to_string(),
            crate::domain::RefType::Tag,
            "2026-01-01T00:00:00Z".to_string(),
        ));
        seed_lock.set(&ResolvedAction::new(
            ActionId::from("actions/checkout"),
            Version::from("v5"),
            CommitSha::from("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
            "actions/checkout".to_string(),
            crate::domain::RefType::Tag,
            "2026-01-01T00:00:00Z".to_string(),
        ));
        lock_store_seed.save(&seed_lock).unwrap();

        // Load manifest and lock via free functions
        let manifest = parse_manifest(&manifest_path).unwrap(); // empty on first run
        let lock = parse_lock(&lock_path).unwrap();
        let scanner = FileWorkflowScanner::new(repo_root);
        let updater = FileWorkflowUpdater::new(repo_root);

        let (updated_manifest, updated_lock) = run(
            manifest,
            lock,
            &manifest_path,
            NoopRegistry,
            &scanner,
            &updater,
        )
        .unwrap();

        // Save the results (simulating what app.rs does when manifest exists)
        FileManifest::new(&manifest_path)
            .save(&updated_manifest)
            .unwrap();
        FileLock::new(&lock_path).save(&updated_lock).unwrap();

        // ---- Assert: manifest has global v6.0.1 + override for windows.yml v5 ----
        let saved_manifest = parse_manifest(&manifest_path).unwrap();

        assert_eq!(
            saved_manifest.get(&ActionId::from("actions/checkout")),
            Some(&Version::from("v6.0.1")),
            "Global version should be v6.0.1 (dominant)"
        );

        let overrides = saved_manifest.overrides_for(&ActionId::from("actions/checkout"));
        assert!(
            !overrides.is_empty(),
            "Bug #1: Expected an override for actions/checkout v5 in windows.yml, got none"
        );

        let windows_override = overrides
            .iter()
            .find(|o| o.workflow.ends_with("windows.yml"));
        assert!(
            windows_override.is_some(),
            "Override must be scoped to windows.yml"
        );
        assert_eq!(
            windows_override.unwrap().version,
            Version::from("v5"),
            "Override version must be v5"
        );

        // ---- Assert: windows.yml was NOT overwritten with the v6.0.1 SHA ----
        let windows_content = fs::read_to_string(workflows_dir.join("windows.yml")).unwrap();
        assert!(
            windows_content.contains("actions/checkout@"),
            "windows.yml should still reference actions/checkout"
        );
        assert!(
            !windows_content.contains("8e8c483db84b4bee98b60c0593521ed34d9990e8"),
            "Bug #2: windows.yml was overwritten with the v6.0.1 SHA — it must use the v5 ref, not v6.0.1.\nGot:\n{windows_content}"
        );
    }
}
