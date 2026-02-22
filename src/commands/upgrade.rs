use anyhow::Result;
use log::{debug, info, warn};
use std::path::Path;

use crate::domain::{
    ActionResolver, ActionSpec, LockKey, ResolutionResult, UpdateResult, UpgradeCandidate,
    VersionRegistry, WorkflowUpdater,
};
use crate::infrastructure::{LockStore, ManifestStore};

/// Run the upgrade command to find and apply available upgrades for actions.
///
/// # Errors
///
/// Returns an error if workflows cannot be read or files cannot be saved.
pub fn run<M: ManifestStore, L: LockStore, R: VersionRegistry, W: WorkflowUpdater>(
    _repo_root: &Path,
    mut manifest: M,
    mut lock: L,
    registry: R,
    writer: &W,
) -> Result<()> {
    let specs = manifest.specs();
    if specs.is_empty() {
        return Ok(());
    }

    let service = ActionResolver::new(registry);

    // Find available upgrades
    info!("Checking for upgrades...");
    let mut upgrades = Vec::new();

    for spec in &specs {
        if spec.version.precision().is_none() {
            debug!("Skipping {spec} (not a semver version)");
            continue;
        }

        match service.registry().all_tags(&spec.id) {
            Ok(tags) => {
                if let Some(upgraded) = spec.version.find_upgrade(&tags) {
                    upgrades.push(UpgradeCandidate {
                        id: spec.id.clone(),
                        current: spec.version.clone(),
                        upgraded,
                    });
                }
            }
            Err(e) => {
                warn!("Could not check upgrades for {spec}: {e}");
            }
        }
    }

    if upgrades.is_empty() {
        info!("All actions are up to date.");
        return Ok(());
    }

    // Apply upgrades
    info!("Upgrading actions:");
    for upgrade in &upgrades {
        info!("+ {upgrade}");
        manifest.set(upgrade.id.clone(), upgrade.upgraded.clone());
    }

    // Resolve new versions to SHAs
    for upgrade in &upgrades {
        let spec = ActionSpec::new(upgrade.id.clone(), upgrade.upgraded.clone());
        let result = service.resolve(&spec);
        match result {
            ResolutionResult::Resolved(resolved) => {
                lock.set(&resolved);
            }
            ResolutionResult::Corrected { corrected, .. } => {
                lock.set(&corrected);
            }
            ResolutionResult::Unresolved { spec: s, reason } => {
                warn!("Could not resolve {s}: {reason}");
            }
        }
    }

    // Save manifest and lock
    manifest.save()?;

    let keys_to_retain: Vec<LockKey> = manifest.specs().iter().map(|s| LockKey::from(*s)).collect();
    lock.retain(&keys_to_retain);
    lock.save()?;

    // Update workflows only for upgraded actions (not all manifest specs)
    let upgraded_keys: Vec<LockKey> = upgrades
        .iter()
        .map(|u| LockKey::new(u.id.clone(), u.upgraded.clone()))
        .collect();
    let update_map = lock.build_update_map(&upgraded_keys);
    let results = writer.update_all(&update_map)?;
    print_update_results(&results);

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
    use std::path::PathBuf;

    #[test]
    fn test_print_update_results_empty() {
        let results: Vec<UpdateResult> = vec![];
        print_update_results(&results);
    }

    #[test]
    fn test_print_update_results_with_changes() {
        let results = vec![
            UpdateResult {
                file: PathBuf::from("ci.yml"),
                changes: vec!["actions/checkout v4 -> v5".to_string()],
            },
            UpdateResult {
                file: PathBuf::from("deploy.yml"),
                changes: vec![
                    "actions/checkout v4 -> v5".to_string(),
                    "docker/build-push-action v5 -> v6".to_string(),
                ],
            },
        ];
        print_update_results(&results);
    }

    #[test]
    fn test_run_empty_manifest_returns_ok() {
        use crate::domain::{VersionRegistry, WorkflowError, WorkflowUpdater};
        use crate::infrastructure::{MemoryLock, MemoryManifest};
        use std::collections::HashMap;
        use tempfile::TempDir;

        struct DummyRegistry;
        impl VersionRegistry for DummyRegistry {
            fn lookup_sha(
                &self,
                _id: &crate::domain::ActionId,
                _version: &crate::domain::Version,
            ) -> std::result::Result<crate::domain::CommitSha, crate::domain::ResolutionError>
            {
                Err(crate::domain::ResolutionError::TokenRequired)
            }
            fn tags_for_sha(
                &self,
                _id: &crate::domain::ActionId,
                _sha: &crate::domain::CommitSha,
            ) -> std::result::Result<Vec<crate::domain::Version>, crate::domain::ResolutionError>
            {
                Err(crate::domain::ResolutionError::TokenRequired)
            }
            fn all_tags(
                &self,
                _id: &crate::domain::ActionId,
            ) -> std::result::Result<Vec<crate::domain::Version>, crate::domain::ResolutionError>
            {
                Err(crate::domain::ResolutionError::TokenRequired)
            }
        }

        struct DummyUpdater;
        impl WorkflowUpdater for DummyUpdater {
            fn update_all(
                &self,
                _actions: &HashMap<crate::domain::ActionId, String>,
            ) -> std::result::Result<Vec<UpdateResult>, WorkflowError> {
                Ok(vec![])
            }
        }

        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();
        std::fs::create_dir_all(root.join(".github").join("workflows")).unwrap();

        let manifest = MemoryManifest::default();
        let lock = MemoryLock::default();

        // Empty manifest should return Ok immediately without calling GitHub
        let result = run(root, manifest, lock, DummyRegistry, &DummyUpdater);
        assert!(result.is_ok());
    }
}
