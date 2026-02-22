use anyhow::Result;
use log::{debug, info};
use std::collections::HashSet;
use std::path::Path;

use crate::domain::{
    ActionId, ActionResolver, ActionSpec, LockKey, ResolutionResult, UpdateResult, Version,
    VersionCorrection, VersionRegistry, WorkflowActionSet, WorkflowScanner, WorkflowUpdater,
};
use crate::infrastructure::{LockStore, ManifestStore};

/// Run the tidy command to synchronize workflow actions with the manifest. Adds missing actions and removes unused ones from the manifest.
///
/// # Errors
///
/// Returns an error if workflows cannot be read or files cannot be saved.
///
/// # Panics
///
/// Panics if an action in the intersection of workflow and manifest actions is not found
/// in the manifest (this should never happen due to the intersection logic).
pub fn run<
    M: ManifestStore,
    L: LockStore,
    R: VersionRegistry,
    P: WorkflowScanner,
    W: WorkflowUpdater,
>(
    _repo_root: &Path,
    mut manifest: M,
    mut lock: L,
    registry: R,
    parser: &P,
    writer: &W,
) -> Result<()> {
    let action_set = parser.scan_all()?;
    if action_set.is_empty() {
        return Ok(());
    }

    let workflow_actions: HashSet<ActionId> = action_set.action_ids().into_iter().collect();

    let manifest_actions: HashSet<ActionId> =
        manifest.specs().iter().map(|s| s.id.clone()).collect();

    // Find differences
    let missing: Vec<_> = workflow_actions.difference(&manifest_actions).collect();
    let unused: Vec<_> = manifest_actions.difference(&workflow_actions).collect();

    // Remove unused actions from manifest
    if !unused.is_empty() {
        info!("Removing unused actions from manifest:");
        for action in &unused {
            info!("- {action}");
            manifest.remove(action);
        }
    }

    // Add missing actions to manifest (using highest version if multiple exist)
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

    // Update existing actions only if manifest has SHA but workflow has tag
    // (This happens when upgrading from SHA to semantic version via comment)
    let existing: Vec<_> = workflow_actions.intersection(&manifest_actions).collect();
    if !existing.is_empty() {
        let mut updated_actions = Vec::new();

        for action_id in &existing {
            let versions = action_set.versions_for(action_id);

            if versions.len() == 1 {
                let workflow_version = &versions[0];
                let manifest_version = manifest
                    .get(action_id)
                    .expect("action_id is from intersection with manifest_actions, so it must be present")
                    .clone();

                // Use domain policy to check if manifest should be updated
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

    // Update lock file with resolved commit SHAs and validate version comments
    let corrections = update_lock_file(&mut lock, &mut manifest, &action_set, registry)?;

    // Save manifest if dirty, including corrections
    manifest.save()?;

    // Remove unused entries from lock file
    let keys_to_retain: Vec<LockKey> = manifest.specs().iter().map(|s| LockKey::from(*s)).collect();
    lock.retain(&keys_to_retain);

    // Save lock file only if dirty
    lock.save()?;

    // Apply manifest versions to workflows using SHAs from lock file
    if manifest.is_empty() {
        info!("No actions found in {}", manifest.path()?.display());
        return Ok(());
    }

    // Build update map with SHAs from lock file and version comments from manifest
    let update_map = lock.build_update_map(&keys_to_retain);

    let results = writer.update_all(&update_map)?;
    print_update_results(&results);

    // Print summary of version corrections
    if !corrections.is_empty() {
        info!("Version corrections:");
        for c in &corrections {
            info!("{c}");
        }
    }

    Ok(())
}

/// Select the best version from a list of versions.
/// Prefers the highest semantic version if available.
fn select_version(versions: &[Version]) -> Version {
    Version::highest(versions).unwrap_or_else(|| versions[0].clone())
}

fn update_lock_file<M: ManifestStore, L: LockStore, R: VersionRegistry>(
    lock: &mut L,
    manifest: &mut M,
    action_set: &WorkflowActionSet,
    registry: R,
) -> Result<Vec<VersionCorrection>> {
    let mut corrections = Vec::new();
    let mut unresolved = Vec::new();

    let specs: Vec<ActionSpec> = manifest.specs().iter().map(|s| (*s).clone()).collect();

    // Check if there are any actions that need resolving
    let needs_resolving = specs.iter().any(|spec| !lock.has(&LockKey::from(spec)));

    // Also check if any actions have SHAs that need validation
    let has_workflow_shas = specs
        .iter()
        .any(|spec| action_set.sha_for(&spec.id).is_some());

    if !needs_resolving && !has_workflow_shas {
        return Ok(corrections);
    }

    let resolution_service = ActionResolver::new(registry);

    // Process each action in manifest
    for spec in &specs {
        // Check if workflow has a SHA for this action
        if let Some(workflow_sha) = action_set.sha_for(&spec.id) {
            // Validate that version comment matches the SHA and determine correct version
            let result = resolution_service.validate_and_correct(spec, workflow_sha);

            match result {
                ResolutionResult::Resolved(resolved) => {
                    lock.set(&resolved);
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

                    // Update manifest with correct version
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
                // Resolve via Github API when there is no workflow SHA
                debug!("Resolving {spec}");
                let result = resolution_service.resolve(spec);

                match result {
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
    use std::path::PathBuf;

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

    #[test]
    fn test_print_results_with_empty_results() {
        let results: Vec<UpdateResult> = vec![];
        print_update_results(&results);
    }

    #[test]
    fn test_print_results_with_updates() {
        let results = vec![UpdateResult {
            file: PathBuf::from("test.yml"),
            changes: vec!["actions/checkout@v4".to_string()],
        }];
        print_update_results(&results);
    }
}
