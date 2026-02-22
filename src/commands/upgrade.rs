use anyhow::Result;
use log::{info, warn};
use std::path::Path;

use crate::domain::{
    ActionId, ActionResolver, ActionSpec, LockKey, ResolutionResult, UpdateResult,
    UpgradeCandidate, Version, VersionRegistry, WorkflowUpdater,
};
use crate::infrastructure::{LockStore, ManifestStore};

/// How the upgrade command should find new versions.
pub enum UpgradeMode {
    /// Default: upgrade all actions within their current major version.
    Safe,
    /// Upgrade all actions to the absolute latest version, including major versions.
    Latest,
    /// Upgrade a single action to a specific version.
    Targeted(ActionId, Version),
}

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
    mode: &UpgradeMode,
) -> Result<()> {
    let service = ActionResolver::new(registry);

    let Some((upgrades, repins)) = determine_upgrades(&manifest, &service, mode)? else {
        return Ok(());
    };

    // Apply upgrades to manifest
    info!("Upgrading actions:");
    for upgrade in &upgrades {
        info!("+ {upgrade}");
        manifest.set(upgrade.id.clone(), upgrade.upgraded.clone());
    }

    // Resolve new versions to SHAs
    for upgrade in &upgrades {
        let spec = ActionSpec::new(upgrade.id.clone(), upgrade.upgraded.clone());
        resolve_and_store(&service, &spec, &mut lock, "Could not resolve");
    }

    // Re-pin non-semver refs to current SHA
    for spec in &repins {
        resolve_and_store(&service, spec, &mut lock, "Could not re-pin");
    }

    // Save manifest and lock
    manifest.save()?;
    let keys_to_retain: Vec<LockKey> = manifest.specs().iter().map(|s| LockKey::from(*s)).collect();
    lock.retain(&keys_to_retain);
    lock.save()?;

    // Update workflows only for upgraded actions and re-pinned refs
    let mut update_keys: Vec<LockKey> = upgrades
        .iter()
        .map(|u| LockKey::new(u.id.clone(), u.upgraded.clone()))
        .collect();
    for spec in &repins {
        update_keys.push(LockKey::from(spec));
    }
    let update_map = lock.build_update_map(&update_keys);
    let results = writer.update_all(&update_map)?;
    print_update_results(&results);

    Ok(())
}

fn determine_upgrades<M: ManifestStore, R: VersionRegistry>(
    manifest: &M,
    service: &ActionResolver<R>,
    mode: &UpgradeMode,
) -> Result<Option<(Vec<UpgradeCandidate>, Vec<ActionSpec>)>> {
    match mode {
        UpgradeMode::Safe | UpgradeMode::Latest => {
            let specs = manifest.specs();
            if specs.is_empty() {
                return Ok(None);
            }

            info!("Checking for upgrades...");
            let mut upgrades = Vec::new();
            let mut repins: Vec<ActionSpec> = Vec::new();

            for spec in &specs {
                if spec.version.precision().is_none() {
                    if spec.version.is_sha() {
                        info!("Skipping {spec} (bare SHA)");
                    } else {
                        info!("Re-pinning {spec} (non-semver ref)");
                        repins.push((*spec).clone());
                    }
                    continue;
                }

                match service.registry().all_tags(&spec.id) {
                    Ok(tags) => {
                        let new_version = match mode {
                            UpgradeMode::Latest => spec.version.find_latest_upgrade(&tags),
                            _ => spec.version.find_upgrade(&tags),
                        };
                        if let Some(upgraded) = new_version {
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

            if upgrades.is_empty() && repins.is_empty() {
                info!("All actions are up to date.");
                return Ok(None);
            }

            Ok(Some((upgrades, repins)))
        }
        UpgradeMode::Targeted(id, version) => {
            let current = manifest
                .get(id)
                .ok_or_else(|| anyhow::anyhow!("{id} not found in manifest"))?;

            match service.registry().all_tags(id) {
                Ok(tags) => {
                    let tag_exists = tags.iter().any(|t| {
                        // Compare by parsing both to semver to handle v5 matching v5.0.0
                        t.as_str() == version.as_str()
                    });
                    if !tag_exists {
                        anyhow::bail!("{version} not found in registry for {id}");
                    }
                }
                Err(e) => {
                    anyhow::bail!("Could not fetch tags for {id}: {e}");
                }
            }

            Ok(Some((
                vec![UpgradeCandidate {
                    id: id.clone(),
                    current: current.clone(),
                    upgraded: version.clone(),
                }],
                vec![],
            )))
        }
    }
}

fn resolve_and_store<R: VersionRegistry, L: LockStore>(
    service: &ActionResolver<R>,
    spec: &ActionSpec,
    lock: &mut L,
    unresolved_msg: &str,
) {
    match service.resolve(spec) {
        ResolutionResult::Resolved(resolved) => {
            lock.set(&resolved);
        }
        ResolutionResult::Corrected { corrected, .. } => {
            lock.set(&corrected);
        }
        ResolutionResult::Unresolved { spec: s, reason } => {
            warn!("{unresolved_msg} {s}: {reason}");
        }
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
        let result = run(
            root,
            manifest,
            lock,
            DummyRegistry,
            &DummyUpdater,
            &UpgradeMode::Safe,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_targeted_action_not_in_manifest() {
        use crate::domain::{ActionId, Version, VersionRegistry, WorkflowError, WorkflowUpdater};
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

        let mode = UpgradeMode::Targeted(ActionId::from("actions/checkout"), Version::from("v5"));
        let result = run(root, manifest, lock, DummyRegistry, &DummyUpdater, &mode);
        assert!(result.is_err());
        assert!(
            result.unwrap_err().to_string().contains("not found"),
            "Should error when action is not in manifest"
        );
    }
}
