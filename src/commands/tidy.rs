use log::{debug, info};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use thiserror::Error;

use crate::domain::{
    ActionId, ActionResolver, ActionSpec, CommitSha, LocatedAction, Lock, LockKey, Manifest,
    ResolutionResult, UpdateResult, Version, VersionCorrection, VersionRegistry, WorkflowActionSet,
    WorkflowScanner, WorkflowUpdater, select_best_tag,
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
#[allow(
    clippy::too_many_arguments,
    clippy::needless_pass_by_value,
    clippy::too_many_lines
)]
pub fn run<R, P, W>(
    mut manifest: Manifest,
    mut lock: Lock,
    manifest_path: &Path,
    registry: R,
    parser: &P,
    writer: &W,
) -> Result<(Manifest, Lock), TidyError>
where
    R: VersionRegistry + Clone,
    P: WorkflowScanner,
    W: WorkflowUpdater,
{
    // Single scan pass with location context
    let located = parser.scan_all_located()?;
    if located.is_empty() {
        return Ok((manifest, lock));
    }

    let action_set = WorkflowActionSet::from_located(&located);
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
        let resolver = ActionResolver::new(registry.clone());
        for action_id in missing {
            let version = select_dominant_version(action_id, &action_set);

            // Task 2.4: SHA correction for new actions only
            // Find a LocatedAction with the dominant version that has a SHA, then validate/correct the version
            let corrected_version = if version.is_sha() {
                // Try to find a located action with this version and SHA
                let located_with_version = located.iter().find(|loc| {
                    &loc.id == action_id && loc.version == version && loc.sha.is_some()
                });

                if let Some(located_action) = located_with_version {
                    if let Some(sha) = &located_action.sha {
                        let (corrected, was_corrected) =
                            resolver.correct_version(action_id, sha, &version);
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

    // Task 2.3: Upgrade SHA versions to tags via registry
    // After adding missing and removing unused, iterate existing manifest entries where version.is_sha()
    // and try to upgrade them to the best tag available in the registry
    {
        let resolver = ActionResolver::new(registry.clone());
        let specs: Vec<ActionSpec> = manifest.specs().iter().map(|s| (*s).clone()).collect();
        let mut upgraded_actions = Vec::new();

        for spec in &specs {
            if spec.version.is_sha() {
                match resolver
                    .registry()
                    .tags_for_sha(&spec.id, &CommitSha::from(spec.version.as_str()))
                {
                    Ok(tags) => {
                        if let Some(best_tag) = select_best_tag(&tags) {
                            manifest.set(spec.id.clone(), best_tag.clone());
                            upgraded_actions
                                .push(format!("{} SHA upgraded to {}", spec.id, best_tag));
                        }
                    }
                    Err(e) => {
                        if matches!(e, crate::domain::resolution::ResolutionError::TokenRequired) {
                            debug!(
                                "GITHUB_TOKEN not set. Cannot upgrade {} SHA {}, keeping SHA",
                                spec.id, spec.version
                            );
                        } else {
                            debug!("Could not upgrade {} SHA {}: {}", spec.id, spec.version, e);
                        }
                    }
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

    // Sync overrides: when multiple distinct versions of the same action appear across
    // workflows, record overrides for steps whose version differs from the manifest global.
    // Only applies to actions with multiple workflow versions — if the manifest has v4 but
    // every workflow step uses v3, the manifest wins and no override is needed.
    sync_overrides(&mut manifest, &located, &action_set);

    // Remove stale override entries (overrides pointing to removed workflows/jobs/steps)
    prune_stale_overrides(&mut manifest, &located);

    // Resolve SHAs and validate version comments (must handle all versions in manifest + overrides)
    let corrections = update_lock(&mut lock, &mut manifest, &registry)?;

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
fn update_lock<R: VersionRegistry + Clone>(
    lock: &mut Lock,
    manifest: &mut Manifest,
    registry: &R,
) -> Result<Vec<VersionCorrection>, TidyError> {
    let corrections = Vec::new();
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

    if !needs_resolving {
        return Ok(corrections);
    }

    let resolver = ActionResolver::new(registry.clone());

    // Resolve lock entries for all specs
    let updated_specs: Vec<ActionSpec> = manifest.specs().iter().map(|s| (*s).clone()).collect();
    for spec in &updated_specs {
        populate_lock_entry(lock, &resolver, spec, &mut unresolved);
    }

    // Resolve override versions (no SHA correction for overrides, just resolve)
    for (id, overrides) in manifest.all_overrides() {
        for exc in overrides {
            let exc_spec = ActionSpec::new(id.clone(), exc.version.clone());
            populate_lock_entry(lock, &resolver, &exc_spec, &mut unresolved);
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

/// Resolve a single spec into the lock if missing, then populate version/specifier fields.
fn populate_lock_entry<R: VersionRegistry + Clone>(
    lock: &mut Lock,
    resolver: &ActionResolver<R>,
    spec: &ActionSpec,
    unresolved: &mut Vec<String>,
) {
    let key = LockKey::from(spec);

    let needs_population = match lock.get(&key) {
        Some(entry) => !entry.is_complete(&spec.version),
        None => true,
    };

    if !needs_population {
        return;
    }

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

    if let Some(entry) = lock.get(&key) {
        if (entry.version.is_none()
            || entry
                .version
                .as_ref()
                .is_some_and(std::string::String::is_empty))
            && let Some(refined_version) = resolver.refine_version(&spec.id, &entry.sha)
        {
            lock.set_version(&key, Some(refined_version.to_string()));
        }

        let expected_specifier = spec.version.specifier();
        lock.set_specifier(&key, expected_specifier);
    }
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

    #[derive(Clone, Copy)]
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
        let github_dir = repo_root.join(".github");

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
        let manifest_path = github_dir.join("gx.toml");

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
        let updater = FileWorkflowUpdater::new(repo_root);

        let (updated_manifest, _) = run(
            manifest,
            lock,
            &manifest_path,
            NoopRegistry,
            &scanner,
            &updater,
        )
        .unwrap();

        // Manifest must still be v4 — workflow v3 SHA must NOT overwrite it
        assert_eq!(
            updated_manifest.get(&ActionId::from("actions/checkout")),
            Some(&Version::from("v4")),
            "Manifest v4 must not be overwritten by workflow SHA pointing to v3"
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
    }

    #[test]
    fn test_sha_to_tag_upgrade_via_registry() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let repo_root = temp_dir.path();
        let workflows_dir = repo_root.join(".github").join("workflows");
        fs::create_dir_all(&workflows_dir).unwrap();
        let github_dir = repo_root.join(".github");

        let sha = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

        // Workflow uses a raw SHA without comment
        let workflow = format!(
            "on: push\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: actions/checkout@{sha}\n"
        );
        fs::write(workflows_dir.join("ci.yml"), &workflow).unwrap();

        let manifest_path = github_dir.join("gx.toml");

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
        let updater = FileWorkflowUpdater::new(repo_root);

        let (updated_manifest, _) =
            run(manifest, lock, &manifest_path, registry, &scanner, &updater).unwrap();

        // Manifest should be upgraded from SHA to v4 (best tag)
        assert_eq!(
            updated_manifest.get(&ActionId::from("actions/checkout")),
            Some(&Version::from("v4")),
            "Manifest SHA should be upgraded to v4 via registry"
        );
    }

    #[test]
    fn test_sha_to_tag_upgrade_graceful_without_token() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let repo_root = temp_dir.path();
        let workflows_dir = repo_root.join(".github").join("workflows");
        fs::create_dir_all(&workflows_dir).unwrap();
        let github_dir = repo_root.join(".github");

        let sha = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

        let workflow = format!(
            "on: push\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: actions/checkout@{sha}\n"
        );
        fs::write(workflows_dir.join("ci.yml"), &workflow).unwrap();

        let manifest_path = github_dir.join("gx.toml");

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

        // NoopRegistry returns TokenRequired — simulates missing GITHUB_TOKEN
        let scanner = FileWorkflowScanner::new(repo_root);
        let updater = FileWorkflowUpdater::new(repo_root);

        let (updated_manifest, _) = run(
            manifest,
            lock,
            &manifest_path,
            NoopRegistry,
            &scanner,
            &updater,
        )
        .unwrap();

        // SHA stays unchanged when no token is available
        assert_eq!(
            updated_manifest.get(&ActionId::from("actions/checkout")),
            Some(&Version::from(sha)),
            "Without a token, SHA must stay unchanged"
        );
    }

    /// Task 3.4: Lock resolves from registry, not from workflow SHAs.
    /// Verifies that lock entries use the registry-resolved SHA, not a stale
    /// workflow SHA that might correspond to a different version.
    #[test]
    fn test_lock_resolves_from_registry_not_workflow_shas() {
        use crate::domain::{LockKey, RefType};

        #[derive(Clone)]
        struct RegistryShaRegistry(String);
        impl crate::domain::VersionRegistry for RegistryShaRegistry {
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
                Err(crate::domain::ResolutionError::TokenRequired)
            }
            fn all_tags(
                &self,
                _id: &ActionId,
            ) -> Result<Vec<Version>, crate::domain::ResolutionError> {
                Err(crate::domain::ResolutionError::TokenRequired)
            }
        }

        let temp_dir = tempfile::TempDir::new().unwrap();
        let repo_root = temp_dir.path();
        let workflows_dir = repo_root.join(".github").join("workflows");
        fs::create_dir_all(&workflows_dir).unwrap();
        let github_dir = repo_root.join(".github");

        // Workflow has a stale SHA that belongs to v3, with comment saying v4
        let workflow_sha = "cccccccccccccccccccccccccccccccccccccccc";
        let workflow = format!(
            "on: push\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: actions/checkout@{workflow_sha} # v4\n"
        );
        fs::write(workflows_dir.join("ci.yml"), &workflow).unwrap();

        let manifest_path = github_dir.join("gx.toml");

        // Manifest has v4 (correct version)
        let mut manifest = Manifest::default();
        manifest.set(ActionId::from("actions/checkout"), Version::from("v4"));

        // Lock is empty — will need to resolve from registry
        let lock = Lock::default();

        // Registry returns a DIFFERENT SHA for v4 (the correct one)
        let registry_sha = "dddddddddddddddddddddddddddddddddddddddd";

        let registry = RegistryShaRegistry(registry_sha.to_string());
        let scanner = FileWorkflowScanner::new(repo_root);
        let updater = FileWorkflowUpdater::new(repo_root);

        let (_, updated_lock) =
            run(manifest, lock, &manifest_path, registry, &scanner, &updater).unwrap();

        // Lock must have the registry SHA, not the workflow SHA
        let key = LockKey::new(ActionId::from("actions/checkout"), Version::from("v4"));
        let entry = updated_lock.get(&key).expect("Lock entry should exist");
        assert_eq!(
            entry.sha.as_str(),
            registry_sha,
            "Lock SHA must come from registry, not from workflow"
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
}
