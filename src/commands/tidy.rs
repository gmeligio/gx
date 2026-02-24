use anyhow::Result;
use log::{debug, info};
use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::domain::{
    ActionId, ActionResolver, ActionSpec, LocatedAction, Lock, LockKey, Manifest, ResolutionResult,
    UpdateResult, Version, VersionCorrection, VersionRegistry, WorkflowActionSet, WorkflowScanner,
    WorkflowScannerLocated, WorkflowUpdater,
};
use crate::infrastructure::{LockStore, ManifestStore};

/// Run the tidy command: synchronize manifest and lock with workflows, then update workflow files.
///
/// # Errors
///
/// Returns an error if workflows cannot be read, resolution fails, or files cannot be saved.
///
/// # Panics
///
/// Panics if an action in the intersection of workflow and manifest is not found in the manifest.
#[allow(clippy::too_many_arguments, clippy::needless_pass_by_value)]
pub fn run<M, L, R, P, W>(
    _repo_root: &Path,
    mut manifest: Manifest,
    manifest_store: M,
    mut lock: Lock,
    lock_store: L,
    registry: R,
    parser: &P,
    writer: &W,
) -> Result<()>
where
    M: ManifestStore,
    L: LockStore,
    R: VersionRegistry,
    P: WorkflowScanner + WorkflowScannerLocated,
    W: WorkflowUpdater,
{
    let action_set = parser.scan_all()?;
    if action_set.is_empty() {
        return Ok(());
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

    // Remove stale exception entries (exceptions pointing to removed workflows/jobs/steps)
    prune_stale_exceptions(&mut manifest, &located);

    // Resolve SHAs and validate version comments (must handle all versions in manifest + exceptions)
    let corrections = update_lock(&mut lock, &mut manifest, &action_set, registry)?;

    // Persist manifest
    manifest_store.save(&manifest)?;

    // Prune and persist lock: retain all version variants that appear in manifest or exceptions
    let keys_to_retain: Vec<LockKey> = build_keys_to_retain(&manifest);
    lock.retain(&keys_to_retain);
    lock_store.save(&lock)?;

    if manifest.is_empty() {
        info!("No actions found in {}", manifest_store.path().display());
        return Ok(());
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
        let steps = by_location
            .iter()
            .find(|(loc, _)| abs_str.ends_with(loc.as_str()))
            .map(|(_, steps)| steps.as_slice())
            .unwrap_or(&[]);
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

    Ok(())
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

/// Collect all LockKeys needed: one per (action, version) pair across globals and exceptions.
fn build_keys_to_retain(manifest: &Manifest) -> Vec<LockKey> {
    let mut keys: Vec<LockKey> = manifest.specs().iter().map(|s| LockKey::from(*s)).collect();
    for (id, exceptions) in manifest.all_exceptions() {
        for exc in exceptions {
            let key = LockKey::new(id.clone(), exc.version.clone());
            if !keys.contains(&key) {
                keys.push(key);
            }
        }
    }
    keys
}

/// Build the per-file update map: resolves each step's version via exception hierarchy.
fn build_file_update_map(
    manifest: &Manifest,
    lock: &Lock,
    steps: &[&LocatedAction],
) -> HashMap<ActionId, String> {
    let mut map: HashMap<ActionId, String> = HashMap::new();
    for action in steps {
        if let Some(version) = manifest.resolve_version(&action.id, &action.location) {
            let key = LockKey::new(action.id.clone(), version.clone());
            if let Some(sha) = lock.get(&key) {
                let workflow_ref = format!("{} # {}", sha, version);
                map.insert(action.id.clone(), workflow_ref);
            }
        }
    }
    map
}

/// Remove exception entries whose referenced workflow/job/step no longer exists in the scanned set.
fn prune_stale_exceptions(manifest: &mut Manifest, located: &[LocatedAction]) {
    let live_workflows: HashSet<&str> =
        located.iter().map(|a| a.location.workflow.as_str()).collect();

    let action_ids: Vec<ActionId> = manifest.all_exceptions().keys().cloned().collect();

    for id in action_ids {
        let exceptions = manifest.exceptions_for(&id).to_vec();
        let pruned: Vec<crate::domain::ActionException> = exceptions
            .into_iter()
            .filter(|exc| {
                if !live_workflows.contains(exc.workflow.as_str()) {
                    info!("Removing stale exception for {id} in {}", exc.workflow);
                    return false;
                }
                if let Some(job) = &exc.job {
                    let job_exists = located.iter().any(|a| {
                        a.location.workflow == exc.workflow
                            && a.location.job.as_deref() == Some(job.as_str())
                    });
                    if !job_exists {
                        info!(
                            "Removing stale exception for {id} in {}/{}",
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
                            "Removing stale exception for {id} in {}:{}/{}",
                            exc.workflow, job, step
                        );
                        return false;
                    }
                }
                true
            })
            .collect();

        manifest.replace_exceptions(id, pruned);
    }
}

fn update_lock<R: VersionRegistry>(
    lock: &mut Lock,
    manifest: &mut Manifest,
    action_set: &WorkflowActionSet,
    registry: R,
) -> Result<Vec<VersionCorrection>> {
    let mut corrections = Vec::new();
    let mut unresolved = Vec::new();

    // Collect all specs: global + exception versions
    let mut all_specs: Vec<ActionSpec> = manifest.specs().iter().map(|s| (*s).clone()).collect();
    for (id, exceptions) in manifest.all_exceptions() {
        for exc in exceptions {
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
            match resolver.validate_and_correct(spec, workflow_sha) {
                ResolutionResult::Resolved(action) => {
                    lock.set(&action);
                }
                ResolutionResult::Corrected {
                    original,
                    corrected,
                } => {
                    corrections.push(VersionCorrection {
                        action: original.id.clone(),
                        old_version: original.version.clone(),
                        new_version: corrected.version.clone(),
                        sha: corrected.sha.clone(),
                    });
                    manifest.set(corrected.id.clone(), corrected.version.clone());
                    lock.set(&corrected);
                }
                ResolutionResult::Unresolved { spec: s, reason } => {
                    debug!("Could not resolve {s}: {reason}");
                    unresolved.push(format!("{s}: {reason}"));
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

    // Then resolve exception versions (no SHA correction for exceptions, just resolve)
    for (id, exceptions) in manifest.all_exceptions() {
        for exc in exceptions {
            let exc_spec = ActionSpec::new(id.clone(), exc.version.clone());
            let key = LockKey::from(&exc_spec);
            if !lock.has(&key) {
                debug!("Resolving exception {exc_spec}");
                match resolver.resolve(&exc_spec) {
                    ResolutionResult::Resolved(action) => {
                        lock.set(&action);
                    }
                    ResolutionResult::Unresolved { spec: s, reason } => {
                        debug!("Could not resolve exception {s}: {reason}");
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
        anyhow::bail!(
            "failed to resolve {} action(s):\n  {}",
            unresolved.len(),
            unresolved.join("\n  ")
        );
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
}
