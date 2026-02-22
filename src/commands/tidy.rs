use anyhow::Result;
use log::{debug, info};
use std::collections::HashSet;
use std::path::Path;

use crate::domain::{
    ActionId, ActionResolver, ActionSpec, Lock, LockKey, Manifest, ResolutionResult, UpdateResult,
    Version, VersionCorrection, VersionRegistry, WorkflowActionSet, WorkflowScanner, WorkflowUpdater,
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
    P: WorkflowScanner,
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
            let versions = action_set.versions_for(action_id);
            let version = select_version(&versions);
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

    // Resolve SHAs and validate version comments
    let corrections = update_lock(&mut lock, &mut manifest, &action_set, registry)?;

    // Persist manifest
    manifest_store.save(&manifest)?;

    // Prune and persist lock
    let keys_to_retain: Vec<LockKey> = manifest.specs().iter().map(|s| LockKey::from(*s)).collect();
    lock.retain(&keys_to_retain);
    lock_store.save(&lock)?;

    if manifest.is_empty() {
        info!("No actions found in {}", manifest_store.path().display());
        return Ok(());
    }

    // Apply versions to workflows
    let update_map = lock.build_update_map(&keys_to_retain);
    let results = writer.update_all(&update_map)?;
    print_update_results(&results);

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

fn update_lock<R: VersionRegistry>(
    lock: &mut Lock,
    manifest: &mut Manifest,
    action_set: &WorkflowActionSet,
    registry: R,
) -> Result<Vec<VersionCorrection>> {
    let mut corrections = Vec::new();
    let mut unresolved = Vec::new();

    let specs: Vec<ActionSpec> = manifest.specs().iter().map(|s| (*s).clone()).collect();

    let needs_resolving = specs.iter().any(|spec| !lock.has(&LockKey::from(spec)));
    let has_workflow_shas = specs.iter().any(|spec| action_set.sha_for(&spec.id).is_some());

    if !needs_resolving && !has_workflow_shas {
        return Ok(corrections);
    }

    let resolver = ActionResolver::new(registry);

    for spec in &specs {
        if let Some(workflow_sha) = action_set.sha_for(&spec.id) {
            match resolver.validate_and_correct(spec, workflow_sha) {
                ResolutionResult::Resolved(resolved) => {
                    lock.set(&resolved);
                }
                ResolutionResult::Corrected { original, corrected } => {
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
                    ResolutionResult::Resolved(resolved) => {
                        lock.set(&resolved);
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
        let versions = vec![Version::from("v3"), Version::from("v4"), Version::from("v2")];
        assert_eq!(select_version(&versions), Version::from("v4"));
    }
}
