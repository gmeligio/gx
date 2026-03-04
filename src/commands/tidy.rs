use log::{debug, info};
use std::collections::{HashMap, HashSet};
use thiserror::Error;

use crate::domain::{
    ActionId, ActionOverride, ActionResolver, ActionSpec, CommitSha, LocatedAction, Lock, LockDiff,
    LockKey, Manifest, ManifestDiff, ShaIndex, TidyPlan, UpdateResult, Version, VersionCorrection,
    VersionRegistry, WorkflowActionSet, WorkflowError, WorkflowPatch, WorkflowScanner,
    WorkflowUpdater, select_most_specific_tag,
};

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
#[allow(clippy::needless_pass_by_value)]
pub fn plan<R, P>(
    manifest: &Manifest,
    lock: &Lock,
    registry: R,
    scanner: &P,
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
    sync_manifest_actions(
        &mut planned_manifest,
        &located,
        &action_set,
        &resolver,
        &mut sha_index,
    );
    upgrade_sha_versions_to_tags(&mut planned_manifest, &resolver, &mut sha_index);

    // Phase 2: Sync overrides
    sync_overrides(&mut planned_manifest, &located, &action_set);
    prune_stale_overrides(&mut planned_manifest, &located);

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
    let corrections = update_lock(
        &mut planned_lock,
        &mut planned_manifest,
        &resolver,
        &workflow_shas,
        &mut sha_index,
    )?;
    let keys_to_retain: Vec<LockKey> = build_keys_to_retain(&planned_manifest);
    planned_lock.retain(&keys_to_retain);

    // Phase 4: Compute workflow patches (instead of writing files)
    let workflow_patches =
        compute_workflow_patches(&located, &planned_manifest, &planned_lock, scanner)?;

    // Diff original vs planned to produce the plan
    let manifest_diff = diff_manifests(manifest, &planned_manifest);
    let lock_diff = diff_locks(lock, &planned_lock);

    Ok(TidyPlan {
        manifest: manifest_diff,
        lock: lock_diff,
        workflows: workflow_patches,
        corrections,
    })
}

/// Compute workflow patches (pin maps) without writing files.
fn compute_workflow_patches<P: WorkflowScanner>(
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

/// Diff two manifest states to produce a `ManifestDiff`.
fn diff_manifests(before: &Manifest, after: &Manifest) -> ManifestDiff {
    let before_ids: HashSet<ActionId> = before.specs().map(|s| s.id.clone()).collect();
    let after_ids: HashSet<ActionId> = after.specs().map(|s| s.id.clone()).collect();

    let added: Vec<(ActionId, Version)> = after_ids
        .difference(&before_ids)
        .filter_map(|id| after.get(id).map(|v| (id.clone(), v.clone())))
        .collect();

    let removed: Vec<ActionId> = before_ids.difference(&after_ids).cloned().collect();

    // Detect version changes on existing actions (e.g. SHA upgraded to tag)
    let updated: Vec<(ActionId, Version)> = before_ids
        .intersection(&after_ids)
        .filter_map(|id| {
            let bv = before.get(id)?;
            let av = after.get(id)?;
            (bv != av).then(|| (id.clone(), av.clone()))
        })
        .collect();

    // Diff overrides
    let before_overrides = before.all_overrides();
    let after_overrides = after.all_overrides();

    let mut overrides_added = Vec::new();
    let mut overrides_removed = Vec::new();

    // Find new overrides
    for (id, after_list) in after_overrides {
        let before_list = before_overrides.get(id).cloned().unwrap_or_default();
        for ovr in after_list {
            if !before_list.contains(ovr) {
                overrides_added.push((id.clone(), ovr.clone()));
            }
        }
    }

    // Find removed overrides
    for (id, before_list) in before_overrides {
        let after_list = after_overrides.get(id).cloned().unwrap_or_default();
        let removed_for_id: Vec<ActionOverride> = before_list
            .iter()
            .filter(|ovr| !after_list.contains(ovr))
            .cloned()
            .collect();
        if !removed_for_id.is_empty() {
            overrides_removed.push((id.clone(), removed_for_id));
        }
    }

    ManifestDiff {
        added,
        removed,
        updated,
        overrides_added,
        overrides_removed,
    }
}

/// Diff two lock states to produce a `LockDiff`.
fn diff_locks(before: &Lock, after: &Lock) -> LockDiff {
    let before_keys: HashSet<LockKey> = before.entries().map(|(k, _)| k.clone()).collect();
    let after_keys: HashSet<LockKey> = after.entries().map(|(k, _)| k.clone()).collect();

    let added = after_keys
        .difference(&before_keys)
        .filter_map(|k| after.get(k).map(|e| (k.clone(), e.clone())))
        .collect();

    let removed = before_keys.difference(&after_keys).cloned().collect();

    // For now, we don't compute fine-grained LockEntryPatch updates.
    // Added entries cover new entries, and the apply phase can handle full replacement.
    LockDiff {
        added,
        removed,
        updated: vec![],
    }
}

/// Remove unused actions from manifest and add missing ones.
fn sync_manifest_actions<R: VersionRegistry>(
    manifest: &mut Manifest,
    located: &[LocatedAction],
    action_set: &WorkflowActionSet,
    resolver: &ActionResolver<R>,
    sha_index: &mut ShaIndex,
) {
    let workflow_actions: HashSet<ActionId> = action_set.action_ids().cloned().collect();
    let manifest_actions: HashSet<ActionId> = manifest.specs().map(|s| s.id.clone()).collect();

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
        for action_id in missing {
            let version = select_dominant_version(action_id, action_set);

            let corrected_version = if version.is_sha() {
                let located_with_version = located.iter().find(|loc| {
                    &loc.id == action_id && loc.version == version && loc.sha.is_some()
                });

                if let Some(located_action) = located_with_version {
                    if let Some(sha) = &located_action.sha {
                        let (corrected, was_corrected) =
                            resolver.correct_version(action_id, sha, &version, sha_index);
                        if was_corrected {
                            info!(
                                "Corrected {action_id} version to {corrected} (SHA {sha} points to {corrected})",
                            );
                        }
                        corrected
                    } else {
                        version.clone()
                    }
                } else {
                    version.clone()
                }
            } else {
                version.clone()
            };

            manifest.set((*action_id).clone(), corrected_version.clone());
            let spec = ActionSpec::new((*action_id).clone(), corrected_version.clone());
            info!("+ {spec}");
        }
    }
}

/// Upgrade SHA versions in manifest to tags via `ShaIndex`.
fn upgrade_sha_versions_to_tags<R: VersionRegistry>(
    manifest: &mut Manifest,
    resolver: &ActionResolver<R>,
    sha_index: &mut ShaIndex,
) {
    // Collect only SHA specs (avoid cloning the full Vec when most specs are tags)
    let sha_specs: Vec<(ActionId, CommitSha)> = manifest
        .specs()
        .filter(|s| s.version.is_sha())
        .map(|s| (s.id.clone(), CommitSha::from(s.version.as_str())))
        .collect();
    let mut upgraded_actions = Vec::new();

    for (id, sha) in &sha_specs {
        match sha_index.get_or_describe(resolver.registry(), id, sha) {
            Ok(desc) => {
                if let Some(best_tag) = select_most_specific_tag(&desc.tags) {
                    manifest.set(id.clone(), best_tag.clone());
                    upgraded_actions.push(format!("{id} SHA upgraded to {best_tag}"));
                }
            }
            Err(e) => {
                debug!("Could not upgrade {id} SHA {sha}: {e}");
            }
        }
    }

    if !upgraded_actions.is_empty() {
        info!("Upgrading SHA versions to tags:");
        for upgrade in &upgraded_actions {
            info!("~ {upgrade}");
        }
    }
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
) -> Result<(), TidyError> {
    let mut results = Vec::new();
    for patch in patches {
        let map: HashMap<ActionId, String> = patch.pins.iter().cloned().collect();
        let result = writer.update_file(&patch.path, &map)?;
        if !result.changes.is_empty() {
            results.push(result);
        }
    }
    print_update_results(&results);
    print_corrections(corrections);
    Ok(())
}

/// Print version corrections.
fn print_corrections(corrections: &[VersionCorrection]) {
    if !corrections.is_empty() {
        info!("Version corrections:");
        for c in corrections {
            info!("{c}");
        }
    }
}

fn select_version(versions: &[Version]) -> Version {
    Version::highest(versions).unwrap_or_else(|| versions[0].clone())
}

fn select_dominant_version(action_id: &ActionId, action_set: &WorkflowActionSet) -> Version {
    action_set.dominant_version(action_id).unwrap_or_else(|| {
        let versions: Vec<Version> = action_set.versions_for(action_id).cloned().collect();
        select_version(&versions)
    })
}

/// Collect all `LockKeys` needed: one per (action, version) pair across globals and overrides.
fn build_keys_to_retain(manifest: &Manifest) -> Vec<LockKey> {
    let seen: std::collections::HashSet<LockKey> = manifest
        .specs()
        .map(LockKey::from)
        .chain(manifest.all_overrides().iter().flat_map(|(id, overrides)| {
            overrides
                .iter()
                .map(move |exc| LockKey::new(id.clone(), exc.version.clone()))
        }))
        .collect();
    seen.into_iter().collect()
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
                // Task 4.1: Omit comment when resolved version is a raw SHA
                let workflow_ref = if version.is_sha() {
                    entry.sha.to_string()
                } else {
                    format!("{} # {}", entry.sha, version)
                };
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
        let version_count = action_set.versions_for(&action.id).count();
        if version_count <= 1 {
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

    // Compute all pruned override lists in one pass, then apply
    let updates: Vec<(ActionId, Vec<crate::domain::ActionOverride>)> = manifest
        .all_overrides()
        .iter()
        .map(|(id, overrides)| {
            let pruned: Vec<crate::domain::ActionOverride> = overrides
                .iter()
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
                .cloned()
                .collect();
            (id.clone(), pruned)
        })
        .collect();

    for (id, pruned) in updates {
        manifest.replace_overrides(id, pruned);
    }
}

/// # Errors
///
/// Returns [`TidyError::ResolutionFailed`] if any actions could not be resolved with a strict error.
/// Recoverable errors (rate limit, auth required) are warned and skipped.
fn update_lock<R: VersionRegistry>(
    lock: &mut Lock,
    manifest: &mut Manifest,
    resolver: &ActionResolver<R>,
    workflow_shas: &HashMap<LockKey, CommitSha>,
    sha_index: &mut ShaIndex,
) -> Result<Vec<VersionCorrection>, TidyError> {
    let corrections = Vec::new();
    let mut unresolved = Vec::new();
    let mut recoverable_count: usize = 0;

    // Build all specs in one pass: global + override versions
    let all_specs: Vec<ActionSpec> = manifest
        .specs()
        .cloned()
        .chain(manifest.all_overrides().iter().flat_map(|(id, overrides)| {
            overrides
                .iter()
                .map(move |exc| ActionSpec::new(id.clone(), exc.version.clone()))
        }))
        .collect();

    let needs_resolving = all_specs.iter().any(|spec| !lock.has(&LockKey::from(spec)));

    if !needs_resolving {
        return Ok(corrections);
    }

    for spec in &all_specs {
        if let Err(e) = populate_lock_entry(lock, resolver, spec, workflow_shas, sha_index) {
            if e.is_recoverable() {
                log::warn!("Skipping {spec}: {e}");
                recoverable_count += 1;
            } else {
                unresolved.push(format!("{spec}: {e}"));
            }
        }
    }

    if recoverable_count > 0 {
        log::warn!(
            "{recoverable_count} action(s) skipped due to recoverable errors — run `gx tidy` again to retry."
        );
    }

    if !unresolved.is_empty() {
        return Err(TidyError::ResolutionFailed {
            count: unresolved.len(),
            specs: unresolved.join("\n  "),
        });
    }

    Ok(corrections)
}

/// Resolve a single spec into the lock if missing, then populate version/specifier fields.
///
/// Returns `Ok(())` on success or when no population was needed.
/// Returns `Err(ResolutionError)` if resolution fails.
fn populate_lock_entry<R: VersionRegistry>(
    lock: &mut Lock,
    resolver: &ActionResolver<R>,
    spec: &ActionSpec,
    workflow_shas: &HashMap<LockKey, CommitSha>,
    sha_index: &mut ShaIndex,
) -> Result<(), crate::domain::ResolutionError> {
    let key = LockKey::from(spec);

    let needs_population = match lock.get(&key) {
        Some(entry) => !entry.is_complete(&spec.version),
        None => true,
    };

    if !needs_population {
        return Ok(());
    }

    if !lock.has(&key) {
        debug!("Resolving {spec}");
        let result = if let Some(sha) = workflow_shas.get(&key) {
            resolver
                .resolve_from_sha(&spec.id, sha, sha_index)
                .or_else(|_| resolver.resolve(spec))
        } else {
            resolver.resolve(spec)
        };

        match result {
            Ok(action) => {
                let entry_version = action.version.to_string();
                // Always store at the spec key (manifest version) so lookups are consistent.
                // For SHA-first, action.version is the most specific tag (e.g. v3.6.1) while
                // spec.version is the manifest version (e.g. v4); they differ, so we need the
                // proxy to ensure the correct lock key.
                let proxy = crate::domain::ResolvedAction::new(
                    action.id,
                    spec.version.clone(),
                    action.sha,
                    action.repository,
                    action.ref_type,
                    action.date,
                );
                lock.set(&proxy);
                lock.set_version(&key, Some(entry_version));
            }
            Err(e) => {
                debug!("Could not resolve {spec}: {e}");
                return Err(e);
            }
        }
    }

    if lock.get(&key).is_some() {
        let expected_specifier = spec.version.specifier();
        lock.set_specifier(&key, expected_specifier);
    }

    Ok(())
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
        FileWorkflowScanner, FileWorkflowUpdater, parse_lock, parse_manifest,
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

    #[derive(Clone, Copy)]
    struct NoopRegistry;
    impl crate::domain::VersionRegistry for NoopRegistry {
        fn lookup_sha(
            &self,
            _id: &ActionId,
            _version: &Version,
        ) -> Result<crate::domain::ResolvedRef, crate::domain::ResolutionError> {
            Err(crate::domain::ResolutionError::AuthRequired)
        }
        fn tags_for_sha(
            &self,
            _id: &ActionId,
            _sha: &CommitSha,
        ) -> Result<Vec<Version>, crate::domain::ResolutionError> {
            Err(crate::domain::ResolutionError::AuthRequired)
        }
        fn all_tags(&self, _id: &ActionId) -> Result<Vec<Version>, crate::domain::ResolutionError> {
            Err(crate::domain::ResolutionError::AuthRequired)
        }
        fn describe_sha(
            &self,
            _id: &ActionId,
            _sha: &CommitSha,
        ) -> Result<crate::domain::ShaDescription, crate::domain::ResolutionError> {
            Err(crate::domain::ResolutionError::AuthRequired)
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
        let seed_diff = crate::domain::LockDiff {
            added: vec![
                (
                    LockKey::new(ActionId::from("actions/checkout"), Version::from("v6.0.1")),
                    crate::domain::LockEntry {
                        sha: CommitSha::from("8e8c483db84b4bee98b60c0593521ed34d9990e8"),
                        version: Some("v6.0.1".to_string()),
                        specifier: Some(String::new()),
                        repository: "actions/checkout".to_string(),
                        ref_type: crate::domain::RefType::Tag,
                        date: "2026-01-01T00:00:00Z".to_string(),
                    },
                ),
                (
                    LockKey::new(ActionId::from("actions/checkout"), Version::from("v5")),
                    crate::domain::LockEntry {
                        sha: CommitSha::from("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
                        version: Some("v5".to_string()),
                        specifier: Some(String::new()),
                        repository: "actions/checkout".to_string(),
                        ref_type: crate::domain::RefType::Tag,
                        date: "2026-01-01T00:00:00Z".to_string(),
                    },
                ),
            ],
            ..Default::default()
        };
        crate::infrastructure::create_lock(&lock_path, &seed_diff).unwrap();

        // Load manifest and lock via free functions
        let manifest = parse_manifest(&manifest_path).unwrap(); // empty on first run
        let lock = parse_lock(&lock_path).unwrap();
        let scanner = FileWorkflowScanner::new(repo_root);
        let updater = FileWorkflowUpdater::new(repo_root);

        let tidy_plan = plan(&manifest, &lock, NoopRegistry, &scanner).unwrap();

        // Apply the plan — manifest doesn't exist yet so use create, lock exists so use apply
        crate::infrastructure::create_manifest(&manifest_path, &tidy_plan.manifest).unwrap();
        crate::infrastructure::apply_lock_diff(&lock_path, &tidy_plan.lock).unwrap();
        apply_workflow_patches(&updater, &tidy_plan.workflows, &tidy_plan.corrections).unwrap();

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

    #[test]
    fn test_lock_completeness_missing_specifier_derived() {
        use crate::domain::{LockEntry, LockKey, RefType, Version};
        use std::collections::HashMap;

        let key = LockKey::new(ActionId::from("actions/checkout"), Version::from("v4"));

        // Create a lock entry with version but missing specifier
        let entry = LockEntry::with_version_and_specifier(
            CommitSha::from("abc123def456789012345678901234567890abcd"),
            Some("v4".to_string()),
            None, // Missing specifier
            "actions/checkout".to_string(),
            RefType::Tag,
            "2026-01-01T00:00:00Z".to_string(),
        );
        let mut lock = Lock::new(HashMap::from([(key.clone(), entry)]));

        // Verify it's not complete
        let entry = lock.get(&key).unwrap();
        assert!(!entry.is_complete(&Version::from("v4")));

        // After tidy, specifier should be populated (DERIVE operation)
        let manifest_version = Version::from("v4");
        let expected_specifier = manifest_version.specifier();
        lock.set_specifier(&key, expected_specifier);

        // Now it should be complete
        let entry = lock.get(&key).unwrap();
        assert!(entry.is_complete(&Version::from("v4")));
        assert_eq!(entry.specifier, Some("^4".to_string()));
    }

    #[test]
    fn test_lock_completeness_missing_version_refined() {
        use crate::domain::{LockEntry, LockKey, RefType, Version};
        use std::collections::HashMap;

        let key = LockKey::new(ActionId::from("actions/checkout"), Version::from("v4"));

        // Create a lock entry with specifier but missing version
        let entry = LockEntry::with_version_and_specifier(
            CommitSha::from("abc123def456789012345678901234567890abcd"),
            None, // Missing version
            Some("^4".to_string()),
            "actions/checkout".to_string(),
            RefType::Tag,
            "2026-01-01T00:00:00Z".to_string(),
        );
        let mut lock = Lock::new(HashMap::from([(key.clone(), entry)]));

        // Verify it's not complete
        let entry = lock.get(&key).unwrap();
        assert!(!entry.is_complete(&Version::from("v4")));

        // After tidy, version should be populated via REFINE
        // (We'd call resolver.refine_version() in the real code)
        lock.set_version(&key, Some("v4".to_string()));

        // Now it should be complete
        let entry = lock.get(&key).unwrap();
        assert!(entry.is_complete(&Version::from("v4")));
        assert_eq!(entry.version, Some("v4".to_string()));
    }

    #[test]
    fn test_lock_completeness_complete_entry_unchanged() {
        use crate::domain::{LockEntry, LockKey, RefType, Version};
        use std::collections::HashMap;

        let key = LockKey::new(ActionId::from("actions/checkout"), Version::from("v4"));

        // Create a complete lock entry
        let entry = LockEntry::with_version_and_specifier(
            CommitSha::from("abc123def456789012345678901234567890abcd"),
            Some("v4".to_string()),
            Some("^4".to_string()),
            "actions/checkout".to_string(),
            RefType::Tag,
            "2026-01-01T00:00:00Z".to_string(),
        );
        let lock = Lock::new(HashMap::from([(key.clone(), entry)]));

        // Verify it's complete
        let entry = lock.get(&key).unwrap();
        assert!(entry.is_complete(&Version::from("v4")));
        // No operations should be performed (skipped)
    }

    #[test]
    fn test_lock_completeness_manifest_version_precision_mismatch() {
        use crate::domain::{LockEntry, LockKey, RefType, Version};
        use std::collections::HashMap;

        let key = LockKey::new(ActionId::from("actions/checkout"), Version::from("v6.1"));

        // Create entry with old specifier from when version was v6
        let entry = LockEntry::with_version_and_specifier(
            CommitSha::from("abc123def456789012345678901234567890abcd"),
            Some("v6.0.2".to_string()),
            Some("^6".to_string()), // Was correct for v6, wrong for v6.1
            "actions/checkout".to_string(),
            RefType::Tag,
            "2026-01-01T00:00:00Z".to_string(),
        );
        let mut lock = Lock::new(HashMap::from([(key.clone(), entry)]));

        // Verify it's not complete because specifier doesn't match v6.1
        let entry = lock.get(&key).unwrap();
        assert!(!entry.is_complete(&Version::from("v6.1")));

        // Fix by updating specifier
        lock.set_specifier(&key, Version::from("v6.1").specifier());

        // Now it should be complete
        let entry = lock.get(&key).unwrap();
        assert!(entry.is_complete(&Version::from("v6.1")));
        assert_eq!(entry.specifier, Some("^6.1".to_string()));
    }

    /// Task 2.5: Manifest authority — manifest v4 must survive even when workflows
    /// have a stale SHA pointing at v3.  The manifest is the source of truth for
    /// existing actions; tidy must never downgrade it from workflow state.
    #[test]
    fn test_manifest_authority_not_overwritten_by_workflow_sha() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let repo_root = temp_dir.path();
        let workflows_dir = repo_root.join(".github").join("workflows");
        fs::create_dir_all(&workflows_dir).unwrap();

        // Workflow pins to a SHA that actually belongs to v3
        let workflow = "on: push
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa # v3
";
        fs::write(workflows_dir.join("ci.yml"), workflow).unwrap();

        // Manifest already tracks actions/checkout at v4 (user's intent)
        let mut manifest = Manifest::default();
        manifest.set(ActionId::from("actions/checkout"), Version::from("v4"));

        // Pre-seed lock so tidy doesn't need to resolve
        let mut lock = Lock::default();
        lock.set(&ResolvedAction::new(
            ActionId::from("actions/checkout"),
            Version::from("v4"),
            CommitSha::from("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"),
            "actions/checkout".to_string(),
            crate::domain::RefType::Tag,
            "2026-01-01T00:00:00Z".to_string(),
        ));

        let scanner = FileWorkflowScanner::new(repo_root);

        let tidy_plan = plan(&manifest, &lock, NoopRegistry, &scanner).unwrap();

        // Manifest diff must NOT change checkout's version — v4 is preserved
        assert!(
            !tidy_plan
                .manifest
                .updated
                .iter()
                .any(|(id, _)| id == &ActionId::from("actions/checkout")),
            "Manifest v4 must not be overwritten by workflow SHA pointing to v3"
        );
        assert!(
            !tidy_plan
                .manifest
                .removed
                .contains(&ActionId::from("actions/checkout")),
            "Manifest should not remove actions/checkout"
        );
    }

    /// Task 2.6: SHA-to-tag upgrade — when the manifest has a raw SHA, tidy
    /// should upgrade it to a tag via the registry.  When no token is available,
    /// the SHA stays unchanged (graceful degradation).

    #[derive(Clone)]
    struct TagUpgradeRegistry {
        tags: Vec<Version>,
    }
    impl crate::domain::VersionRegistry for TagUpgradeRegistry {
        fn lookup_sha(
            &self,
            id: &ActionId,
            version: &Version,
        ) -> Result<crate::domain::ResolvedRef, crate::domain::ResolutionError> {
            Ok(crate::domain::ResolvedRef::new(
                CommitSha::from(version.as_str()),
                id.base_repo(),
                crate::domain::RefType::Tag,
                "2026-01-01T00:00:00Z".to_string(),
            ))
        }
        fn tags_for_sha(
            &self,
            _id: &ActionId,
            _sha: &CommitSha,
        ) -> Result<Vec<Version>, crate::domain::ResolutionError> {
            Ok(self.tags.clone())
        }
        fn all_tags(&self, _id: &ActionId) -> Result<Vec<Version>, crate::domain::ResolutionError> {
            Ok(self.tags.clone())
        }
        fn describe_sha(
            &self,
            id: &ActionId,
            _sha: &CommitSha,
        ) -> Result<crate::domain::ShaDescription, crate::domain::ResolutionError> {
            Ok(crate::domain::ShaDescription {
                tags: self.tags.clone(),
                repository: id.base_repo(),
                date: "2026-01-01T00:00:00Z".to_string(),
            })
        }
    }

    #[test]
    fn test_sha_to_tag_upgrade_via_registry() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let repo_root = temp_dir.path();
        let workflows_dir = repo_root.join(".github").join("workflows");
        fs::create_dir_all(&workflows_dir).unwrap();

        let sha = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

        // Workflow uses a raw SHA without comment
        let workflow = format!(
            "on: push\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: actions/checkout@{sha}\n"
        );
        fs::write(workflows_dir.join("ci.yml"), &workflow).unwrap();

        // Manifest has the raw SHA as the version
        let mut manifest = Manifest::default();
        manifest.set(ActionId::from("actions/checkout"), Version::from(sha));

        // Pre-seed lock
        let mut lock = Lock::default();
        lock.set(&ResolvedAction::new(
            ActionId::from("actions/checkout"),
            Version::from(sha),
            CommitSha::from(sha),
            "actions/checkout".to_string(),
            crate::domain::RefType::Tag,
            "2026-01-01T00:00:00Z".to_string(),
        ));

        // Registry returns v4 for this SHA
        let registry = TagUpgradeRegistry {
            tags: vec![Version::from("v4"), Version::from("v4.0.0")],
        };

        let scanner = FileWorkflowScanner::new(repo_root);

        let tidy_plan = plan(&manifest, &lock, registry, &scanner).unwrap();

        // Manifest should show the SHA upgraded to the most specific tag (v4.0.0)
        let has_upgrade = tidy_plan.manifest.updated.iter().any(|(id, v)| {
            id == &ActionId::from("actions/checkout") && v == &Version::from("v4.0.0")
        });
        assert!(
            has_upgrade,
            "Manifest SHA should be upgraded to v4.0.0 (most specific) via registry, got: {:?}",
            tidy_plan.manifest.updated
        );
    }

    #[test]
    fn test_sha_to_tag_upgrade_graceful_without_token() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let repo_root = temp_dir.path();
        let workflows_dir = repo_root.join(".github").join("workflows");
        fs::create_dir_all(&workflows_dir).unwrap();

        let sha = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

        let workflow = format!(
            "on: push\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: actions/checkout@{sha}\n"
        );
        fs::write(workflows_dir.join("ci.yml"), &workflow).unwrap();

        let mut manifest = Manifest::default();
        manifest.set(ActionId::from("actions/checkout"), Version::from(sha));

        // Pre-seed lock
        let mut lock = Lock::default();
        lock.set(&ResolvedAction::new(
            ActionId::from("actions/checkout"),
            Version::from(sha),
            CommitSha::from(sha),
            "actions/checkout".to_string(),
            crate::domain::RefType::Tag,
            "2026-01-01T00:00:00Z".to_string(),
        ));

        // NoopRegistry returns AuthRequired — simulates missing GITHUB_TOKEN
        let scanner = FileWorkflowScanner::new(repo_root);

        let tidy_plan = plan(&manifest, &lock, NoopRegistry, &scanner).unwrap();

        // SHA stays unchanged when no token is available — no version updates in plan
        assert!(
            !tidy_plan
                .manifest
                .updated
                .iter()
                .any(|(id, _)| id == &ActionId::from("actions/checkout")),
            "Without a token, SHA must stay unchanged"
        );
    }

    /// SHA-first: when a workflow provides a SHA, the lock must use that SHA directly.
    /// The registry is only consulted for metadata (repo, date), not to override the SHA.
    #[test]
    fn test_lock_resolves_from_workflow_sha_first() {
        use crate::domain::{LockKey, RefType};

        #[derive(Clone)]
        struct MetadataOnlyRegistry;
        impl crate::domain::VersionRegistry for MetadataOnlyRegistry {
            fn lookup_sha(
                &self,
                id: &ActionId,
                _version: &Version,
            ) -> Result<crate::domain::ResolvedRef, crate::domain::ResolutionError> {
                Ok(crate::domain::ResolvedRef::new(
                    // Registry SHA is irrelevant — resolve_from_sha uses the input SHA
                    CommitSha::from("dddddddddddddddddddddddddddddddddddddddd"),
                    id.base_repo(),
                    RefType::Tag,
                    "2026-01-01T00:00:00Z".to_string(),
                ))
            }
            fn tags_for_sha(
                &self,
                _id: &ActionId,
                _sha: &CommitSha,
            ) -> Result<Vec<Version>, crate::domain::ResolutionError> {
                Err(crate::domain::ResolutionError::AuthRequired)
            }
            fn all_tags(
                &self,
                _id: &ActionId,
            ) -> Result<Vec<Version>, crate::domain::ResolutionError> {
                Err(crate::domain::ResolutionError::AuthRequired)
            }
            fn describe_sha(
                &self,
                id: &ActionId,
                _sha: &CommitSha,
            ) -> Result<crate::domain::ShaDescription, crate::domain::ResolutionError> {
                Ok(crate::domain::ShaDescription {
                    tags: vec![],
                    repository: id.base_repo(),
                    date: "2026-01-01T00:00:00Z".to_string(),
                })
            }
        }

        let temp_dir = tempfile::TempDir::new().unwrap();
        let repo_root = temp_dir.path();
        let workflows_dir = repo_root.join(".github").join("workflows");
        fs::create_dir_all(&workflows_dir).unwrap();

        // Workflow pins to a specific SHA with a floating tag comment
        let workflow_sha = "cccccccccccccccccccccccccccccccccccccccc";
        let workflow = format!(
            "on: push\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: actions/checkout@{workflow_sha} # v4\n"
        );
        fs::write(workflows_dir.join("ci.yml"), &workflow).unwrap();

        // Manifest has v4
        let mut manifest = Manifest::default();
        manifest.set(ActionId::from("actions/checkout"), Version::from("v4"));

        // Lock is empty — will be populated via SHA-first path
        let lock = Lock::default();

        let scanner = FileWorkflowScanner::new(repo_root);
        let tidy_plan = plan(&manifest, &lock, MetadataOnlyRegistry, &scanner).unwrap();

        // Lock diff must add an entry using the workflow SHA (SHA-first)
        let key = LockKey::new(ActionId::from("actions/checkout"), Version::from("v4"));
        let added_entry = tidy_plan
            .lock
            .added
            .iter()
            .find(|(k, _)| *k == key)
            .map(|(_, e)| e);
        assert!(
            added_entry.is_some(),
            "Lock diff should add entry for actions/checkout@v4"
        );
        assert_eq!(
            added_entry.unwrap().sha.as_str(),
            workflow_sha,
            "Lock SHA must come from workflow (SHA-first), not from registry"
        );
    }

    /// Task 4.2: SHA-only manifest version produces `@SHA` without trailing
    /// `# SHA` comment in workflow output.
    #[test]
    fn test_sha_only_version_no_trailing_comment() {
        use crate::domain::{LockEntry, LockKey, RefType, WorkflowLocation};
        use std::collections::HashMap;

        let sha = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

        // Manifest has SHA as version
        let mut manifest = Manifest::default();
        manifest.set(ActionId::from("actions/checkout"), Version::from(sha));

        // Lock has an entry for this SHA version
        let key = LockKey::new(ActionId::from("actions/checkout"), Version::from(sha));
        let entry = LockEntry::with_version_and_specifier(
            CommitSha::from(sha),
            None,
            None,
            "actions/checkout".to_string(),
            RefType::Tag,
            "2026-01-01T00:00:00Z".to_string(),
        );
        let lock = Lock::new(HashMap::from([(key, entry)]));

        // A located action referencing this action
        let located = LocatedAction {
            id: ActionId::from("actions/checkout"),
            version: Version::from(sha),
            sha: Some(CommitSha::from(sha)),
            location: WorkflowLocation {
                workflow: ".github/workflows/ci.yml".to_string(),
                job: Some("build".to_string()),
                step: Some(0),
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

    // ========== Step 8: tidy::plan() tests ==========

    #[test]
    fn test_plan_empty_workflows_returns_empty_plan() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let repo_root = temp_dir.path();
        // Create .github/workflows dir but no workflow files
        let workflows_dir = repo_root.join(".github").join("workflows");
        fs::create_dir_all(&workflows_dir).unwrap();

        let manifest = Manifest::default();
        let lock = Lock::default();
        let scanner = FileWorkflowScanner::new(repo_root);

        let result = plan(&manifest, &lock, NoopRegistry, &scanner).unwrap();
        assert!(result.is_empty(), "Plan for empty workflows must be empty");
    }

    #[test]
    fn test_plan_one_new_action_produces_added_entries() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let repo_root = temp_dir.path();
        let workflows_dir = repo_root.join(".github").join("workflows");
        fs::create_dir_all(&workflows_dir).unwrap();

        let sha = "abc123def456789012345678901234567890abcd";
        let workflow = format!(
            "on: push\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: actions/checkout@{sha} # v4\n"
        );
        fs::write(workflows_dir.join("ci.yml"), &workflow).unwrap();

        // Pre-seed lock so plan doesn't need to resolve via registry
        let mut lock = Lock::default();
        lock.set(&ResolvedAction::new(
            ActionId::from("actions/checkout"),
            Version::from("v4"),
            CommitSha::from(sha),
            "actions/checkout".to_string(),
            crate::domain::RefType::Tag,
            "2026-01-01T00:00:00Z".to_string(),
        ));

        let manifest = Manifest::default(); // empty — action is "new"
        let scanner = FileWorkflowScanner::new(repo_root);

        let result = plan(&manifest, &lock, NoopRegistry, &scanner).unwrap();

        // Manifest should have added action
        assert!(
            result.manifest.added.iter().any(|(id, v)| {
                id == &ActionId::from("actions/checkout") && v == &Version::from("v4")
            }),
            "Plan must include actions/checkout@v4 in manifest.added, got: {:?}",
            result.manifest.added
        );
        assert!(result.manifest.removed.is_empty());
    }

    #[test]
    fn test_plan_removed_action_produces_removed_entries() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let repo_root = temp_dir.path();
        let workflows_dir = repo_root.join(".github").join("workflows");
        fs::create_dir_all(&workflows_dir).unwrap();

        // Workflow only has setup-node, not checkout
        let workflow = "on: push\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: actions/setup-node@aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa # v3\n";
        fs::write(workflows_dir.join("ci.yml"), workflow).unwrap();

        // Manifest has both checkout and setup-node
        let mut manifest = Manifest::default();
        manifest.set(ActionId::from("actions/checkout"), Version::from("v4"));
        manifest.set(ActionId::from("actions/setup-node"), Version::from("v3"));

        // Pre-seed lock for both
        let mut lock = Lock::default();
        lock.set(&ResolvedAction::new(
            ActionId::from("actions/checkout"),
            Version::from("v4"),
            CommitSha::from("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"),
            "actions/checkout".to_string(),
            crate::domain::RefType::Tag,
            "2026-01-01T00:00:00Z".to_string(),
        ));
        lock.set(&ResolvedAction::new(
            ActionId::from("actions/setup-node"),
            Version::from("v3"),
            CommitSha::from("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
            "actions/setup-node".to_string(),
            crate::domain::RefType::Tag,
            "2026-01-01T00:00:00Z".to_string(),
        ));

        let scanner = FileWorkflowScanner::new(repo_root);

        let result = plan(&manifest, &lock, NoopRegistry, &scanner).unwrap();

        // checkout should be removed from manifest
        assert!(
            result
                .manifest
                .removed
                .contains(&ActionId::from("actions/checkout")),
            "Plan must include actions/checkout in manifest.removed, got: {:?}",
            result.manifest.removed
        );
        // setup-node should NOT be removed
        assert!(
            !result
                .manifest
                .removed
                .contains(&ActionId::from("actions/setup-node")),
        );
        // Lock should also have checkout removed
        assert!(
            result
                .lock
                .removed
                .iter()
                .any(|k| k.id == ActionId::from("actions/checkout")),
            "Plan must include actions/checkout in lock.removed, got: {:?}",
            result.lock.removed
        );
    }

    #[test]
    fn test_plan_multiple_versions_produces_override_diff() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let repo_root = temp_dir.path();
        let workflows_dir = repo_root.join(".github").join("workflows");
        fs::create_dir_all(&workflows_dir).unwrap();

        // ci.yml: 3 workflows with checkout@SHA # v6.0.1
        let sha_workflow = "on: push\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: actions/checkout@8e8c483db84b4bee98b60c0593521ed34d9990e8 # v6.0.1\n";
        fs::write(workflows_dir.join("ci.yml"), sha_workflow).unwrap();
        fs::write(workflows_dir.join("build.yml"), sha_workflow).unwrap();

        // windows.yml: checkout@v5 (minority version)
        let win_workflow = "on: push\njobs:\n  test:\n    runs-on: windows-latest\n    steps:\n      - uses: actions/checkout@v5\n";
        fs::write(workflows_dir.join("windows.yml"), win_workflow).unwrap();

        let manifest = Manifest::default(); // empty — will be populated by plan

        // Pre-seed lock for both versions
        let mut lock = Lock::default();
        lock.set(&ResolvedAction::new(
            ActionId::from("actions/checkout"),
            Version::from("v6.0.1"),
            CommitSha::from("8e8c483db84b4bee98b60c0593521ed34d9990e8"),
            "actions/checkout".to_string(),
            crate::domain::RefType::Tag,
            "2026-01-01T00:00:00Z".to_string(),
        ));
        lock.set(&ResolvedAction::new(
            ActionId::from("actions/checkout"),
            Version::from("v5"),
            CommitSha::from("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
            "actions/checkout".to_string(),
            crate::domain::RefType::Tag,
            "2026-01-01T00:00:00Z".to_string(),
        ));

        let scanner = FileWorkflowScanner::new(repo_root);

        let result = plan(&manifest, &lock, NoopRegistry, &scanner).unwrap();

        // Should have override(s) for the minority version
        assert!(
            !result.manifest.overrides_added.is_empty(),
            "Plan must include override entries for minority version, got: {:?}",
            result.manifest.overrides_added
        );

        // At least one override should be for v5 in windows.yml
        let has_windows_override = result.manifest.overrides_added.iter().any(|(id, ovr)| {
            id == &ActionId::from("actions/checkout")
                && ovr.workflow.ends_with("windows.yml")
                && ovr.version == Version::from("v5")
        });
        assert!(
            has_windows_override,
            "Plan must include v5 override for windows.yml"
        );
    }

    #[test]
    fn test_plan_stale_override_produces_override_removal() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let repo_root = temp_dir.path();
        let workflows_dir = repo_root.join(".github").join("workflows");
        fs::create_dir_all(&workflows_dir).unwrap();

        // Only ci.yml with checkout@SHA # v4
        let workflow = "on: push\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: actions/checkout@bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb # v4\n";
        fs::write(workflows_dir.join("ci.yml"), workflow).unwrap();

        // Manifest has checkout@v4 + stale override for deploy.yml (which no longer exists)
        let mut manifest = Manifest::default();
        manifest.set(ActionId::from("actions/checkout"), Version::from("v4"));
        manifest.add_override(
            ActionId::from("actions/checkout"),
            ActionOverride {
                workflow: ".github/workflows/deploy.yml".to_string(),
                job: None,
                step: None,
                version: Version::from("v3"),
            },
        );

        // Pre-seed lock
        let mut lock = Lock::default();
        lock.set(&ResolvedAction::new(
            ActionId::from("actions/checkout"),
            Version::from("v4"),
            CommitSha::from("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"),
            "actions/checkout".to_string(),
            crate::domain::RefType::Tag,
            "2026-01-01T00:00:00Z".to_string(),
        ));

        let scanner = FileWorkflowScanner::new(repo_root);

        let result = plan(&manifest, &lock, NoopRegistry, &scanner).unwrap();

        // Should have override removal for the stale deploy.yml override
        assert!(
            !result.manifest.overrides_removed.is_empty(),
            "Plan must include removed override for stale deploy.yml, got: {:?}",
            result.manifest.overrides_removed
        );
        let has_deploy_removal = result.manifest.overrides_removed.iter().any(|(id, ovrs)| {
            id == &ActionId::from("actions/checkout")
                && ovrs.iter().any(|o| o.workflow.ends_with("deploy.yml"))
        });
        assert!(
            has_deploy_removal,
            "Plan must include removal of deploy.yml override"
        );
    }

    #[test]
    fn test_plan_everything_in_sync_returns_empty_plan() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let repo_root = temp_dir.path();
        let workflows_dir = repo_root.join(".github").join("workflows");
        fs::create_dir_all(&workflows_dir).unwrap();

        let sha = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
        let workflow = format!(
            "on: push\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: actions/checkout@{sha} # v4\n"
        );
        fs::write(workflows_dir.join("ci.yml"), &workflow).unwrap();

        // Manifest already has checkout@v4
        let mut manifest = Manifest::default();
        manifest.set(ActionId::from("actions/checkout"), Version::from("v4"));

        // Lock already has the entry fully populated
        let mut lock = Lock::default();
        lock.set(&ResolvedAction::new(
            ActionId::from("actions/checkout"),
            Version::from("v4"),
            CommitSha::from(sha),
            "actions/checkout".to_string(),
            crate::domain::RefType::Tag,
            "2026-01-01T00:00:00Z".to_string(),
        ));

        let scanner = FileWorkflowScanner::new(repo_root);

        let result = plan(&manifest, &lock, NoopRegistry, &scanner).unwrap();

        // Everything is in sync — plan should have no manifest/lock changes
        assert!(
            result.manifest.added.is_empty(),
            "No manifest additions expected, got: {:?}",
            result.manifest.added
        );
        assert!(
            result.manifest.removed.is_empty(),
            "No manifest removals expected, got: {:?}",
            result.manifest.removed
        );
        assert!(
            result.lock.added.is_empty(),
            "No lock additions expected, got: {:?}",
            result.lock.added
        );
        assert!(
            result.lock.removed.is_empty(),
            "No lock removals expected, got: {:?}",
            result.lock.removed
        );
    }

    // ========== SHA-first lock resolution tests (tasks 5.1 and 5.2) ==========

    /// 5.1: Workflow has SHA-pinned action with floating tag comment (e.g., `@sha # v3`).
    /// Lock entry must use the workflow SHA and the most specific version from tags.
    #[test]
    fn test_sha_first_lock_uses_workflow_sha_and_most_specific_version() {
        use crate::domain::{LockKey, RefType};

        #[derive(Clone)]
        struct TaggedShaRegistry;
        impl crate::domain::VersionRegistry for TaggedShaRegistry {
            fn lookup_sha(
                &self,
                id: &ActionId,
                _version: &Version,
            ) -> Result<crate::domain::ResolvedRef, crate::domain::ResolutionError> {
                Ok(crate::domain::ResolvedRef::new(
                    CommitSha::from("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
                    id.base_repo(),
                    RefType::Commit,
                    "2026-01-01T00:00:00Z".to_string(),
                ))
            }
            fn tags_for_sha(
                &self,
                _id: &ActionId,
                _sha: &CommitSha,
            ) -> Result<Vec<Version>, crate::domain::ResolutionError> {
                Ok(vec![
                    Version::from("v3"),
                    Version::from("v3.6"),
                    Version::from("v3.6.1"),
                ])
            }
            fn all_tags(
                &self,
                _id: &ActionId,
            ) -> Result<Vec<Version>, crate::domain::ResolutionError> {
                Ok(vec![
                    Version::from("v3"),
                    Version::from("v3.6"),
                    Version::from("v3.6.1"),
                ])
            }
            fn describe_sha(
                &self,
                id: &ActionId,
                _sha: &CommitSha,
            ) -> Result<crate::domain::ShaDescription, crate::domain::ResolutionError> {
                Ok(crate::domain::ShaDescription {
                    tags: vec![
                        Version::from("v3"),
                        Version::from("v3.6"),
                        Version::from("v3.6.1"),
                    ],
                    repository: id.base_repo(),
                    date: "2026-01-01T00:00:00Z".to_string(),
                })
            }
        }

        let temp_dir = tempfile::TempDir::new().unwrap();
        let repo_root = temp_dir.path();
        let workflows_dir = repo_root.join(".github").join("workflows");
        fs::create_dir_all(&workflows_dir).unwrap();

        // Workflow pins to SHA with floating tag comment
        let workflow_sha = "6d1e696000000000000000000000000000000000";
        let workflow = format!(
            "on: push\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: jdx/mise-action@{workflow_sha} # v3\n"
        );
        fs::write(workflows_dir.join("ci.yml"), &workflow).unwrap();

        // Manifest has v3 (the floating tag from the comment)
        let mut manifest = Manifest::default();
        manifest.set(ActionId::from("jdx/mise-action"), Version::from("v3"));

        // Lock is empty — will be populated via SHA-first path
        let lock = Lock::default();

        let scanner = FileWorkflowScanner::new(repo_root);
        let tidy_plan = plan(&manifest, &lock, TaggedShaRegistry, &scanner).unwrap();

        let key = LockKey::new(ActionId::from("jdx/mise-action"), Version::from("v3"));
        let added_entry = tidy_plan
            .lock
            .added
            .iter()
            .find(|(k, _)| *k == key)
            .map(|(_, e)| e);

        assert!(
            added_entry.is_some(),
            "Lock diff should add entry for jdx/mise-action@v3"
        );
        let entry = added_entry.unwrap();
        assert_eq!(
            entry.sha.as_str(),
            workflow_sha,
            "Lock SHA must be the workflow SHA (SHA-first)"
        );
        assert_eq!(
            entry.version.as_deref(),
            Some("v3.6.1"),
            "Lock version must be the most specific tag"
        );
    }

    /// 5.2: Workflow has a bare version ref (no SHA). Lock must fall back to registry resolution.
    #[test]
    fn test_version_ref_falls_back_to_registry_resolution() {
        use crate::domain::{LockKey, RefType};

        #[derive(Clone)]
        struct SimpleRegistry(String);
        impl crate::domain::VersionRegistry for SimpleRegistry {
            fn lookup_sha(
                &self,
                id: &ActionId,
                _version: &Version,
            ) -> Result<crate::domain::ResolvedRef, crate::domain::ResolutionError> {
                Ok(crate::domain::ResolvedRef::new(
                    CommitSha::from(self.0.as_str()),
                    id.base_repo(),
                    RefType::Tag,
                    "2026-01-01T00:00:00Z".to_string(),
                ))
            }
            fn tags_for_sha(
                &self,
                _id: &ActionId,
                _sha: &CommitSha,
            ) -> Result<Vec<Version>, crate::domain::ResolutionError> {
                Err(crate::domain::ResolutionError::AuthRequired)
            }
            fn all_tags(
                &self,
                _id: &ActionId,
            ) -> Result<Vec<Version>, crate::domain::ResolutionError> {
                Err(crate::domain::ResolutionError::AuthRequired)
            }
            fn describe_sha(
                &self,
                _id: &ActionId,
                _sha: &CommitSha,
            ) -> Result<crate::domain::ShaDescription, crate::domain::ResolutionError> {
                Err(crate::domain::ResolutionError::AuthRequired)
            }
        }

        let registry_sha = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

        let temp_dir = tempfile::TempDir::new().unwrap();
        let repo_root = temp_dir.path();
        let workflows_dir = repo_root.join(".github").join("workflows");
        fs::create_dir_all(&workflows_dir).unwrap();

        // Workflow uses a bare version tag (no SHA pinning)
        let workflow = "on: push\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: actions/checkout@v4\n";
        fs::write(workflows_dir.join("ci.yml"), workflow).unwrap();

        // Manifest has v4
        let mut manifest = Manifest::default();
        manifest.set(ActionId::from("actions/checkout"), Version::from("v4"));

        // Lock is empty — will be resolved via registry fallback
        let lock = Lock::default();

        let scanner = FileWorkflowScanner::new(repo_root);
        let tidy_plan = plan(
            &manifest,
            &lock,
            SimpleRegistry(registry_sha.to_string()),
            &scanner,
        )
        .unwrap();

        let key = LockKey::new(ActionId::from("actions/checkout"), Version::from("v4"));
        let added_entry = tidy_plan
            .lock
            .added
            .iter()
            .find(|(k, _)| *k == key)
            .map(|(_, e)| e);

        assert!(
            added_entry.is_some(),
            "Lock diff should add entry for actions/checkout@v4"
        );
        assert_eq!(
            added_entry.unwrap().sha.as_str(),
            registry_sha,
            "Lock SHA must come from registry when no workflow SHA is available"
        );
    }

    /// Recoverable errors (AuthRequired) are warned and skipped; strict errors still fail.
    #[test]
    fn test_update_lock_recoverable_errors_are_skipped() {
        use crate::domain::{LockKey, RefType};

        // Registry: checkout fails with AuthRequired (recoverable), setup-node fails with strict
        #[derive(Clone)]
        struct MixedRegistry;
        impl crate::domain::VersionRegistry for MixedRegistry {
            fn lookup_sha(
                &self,
                id: &ActionId,
                _version: &Version,
            ) -> Result<crate::domain::ResolvedRef, crate::domain::ResolutionError> {
                if id.as_str() == "actions/checkout" {
                    Err(crate::domain::ResolutionError::AuthRequired)
                } else {
                    Ok(crate::domain::ResolvedRef::new(
                        CommitSha::from("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
                        id.base_repo(),
                        RefType::Tag,
                        "2026-01-01T00:00:00Z".to_string(),
                    ))
                }
            }
            fn tags_for_sha(
                &self,
                _id: &ActionId,
                _sha: &CommitSha,
            ) -> Result<Vec<Version>, crate::domain::ResolutionError> {
                Err(crate::domain::ResolutionError::AuthRequired)
            }
            fn all_tags(
                &self,
                _id: &ActionId,
            ) -> Result<Vec<Version>, crate::domain::ResolutionError> {
                Err(crate::domain::ResolutionError::AuthRequired)
            }
            fn describe_sha(
                &self,
                _id: &ActionId,
                _sha: &CommitSha,
            ) -> Result<crate::domain::ShaDescription, crate::domain::ResolutionError> {
                Err(crate::domain::ResolutionError::AuthRequired)
            }
        }

        let temp_dir = tempfile::TempDir::new().unwrap();
        let repo_root = temp_dir.path();
        let workflows_dir = repo_root.join(".github").join("workflows");
        fs::create_dir_all(&workflows_dir).unwrap();

        let workflow = "on: push
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
";
        fs::write(workflows_dir.join("ci.yml"), workflow).unwrap();

        // Both actions need resolving (empty lock)
        let lock = Lock::default();
        let manifest = Manifest::default();
        let scanner = FileWorkflowScanner::new(repo_root);

        // With MixedRegistry, checkout fails with AuthRequired (recoverable),
        // setup-node resolves successfully. Plan should succeed (recoverable errors are skipped).
        let result = plan(&manifest, &lock, MixedRegistry, &scanner);
        assert!(
            result.is_ok(),
            "Plan should succeed when only recoverable errors occur"
        );

        // setup-node should be in the lock diff (it resolved successfully)
        let tidy_plan = result.unwrap();
        let setup_node_key =
            LockKey::new(ActionId::from("actions/setup-node"), Version::from("v4"));
        assert!(
            tidy_plan
                .lock
                .added
                .iter()
                .any(|(k, _)| *k == setup_node_key),
            "setup-node should be resolved and added to lock"
        );
    }
}
