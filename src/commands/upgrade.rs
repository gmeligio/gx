use anyhow::Result;
use log::{info, warn};
use std::path::Path;

use crate::domain::{
    ActionId, ActionResolver, ActionSpec, Lock, LockKey, Manifest, ResolutionResult, UpdateResult,
    UpgradeCandidate, Version, VersionRegistry, WorkflowScanner, WorkflowUpdater,
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
/// Errors if workflows are out of sync with the manifest — run `gx tidy` first.
///
/// # Errors
///
/// Returns an error if drift is detected, if workflows cannot be read, or if files cannot be saved.
#[allow(clippy::too_many_arguments, clippy::needless_pass_by_value)]
pub fn run<M, L, R, P, W>(
    _repo_root: &Path,
    mut manifest: Manifest,
    manifest_store: M,
    mut lock: Lock,
    lock_store: L,
    registry: R,
    scanner: &P,
    writer: &W,
    mode: &UpgradeMode,
) -> Result<()>
where
    M: ManifestStore,
    L: LockStore,
    R: VersionRegistry,
    P: WorkflowScanner,
    W: WorkflowUpdater,
{
    // Detect drift before doing any upgrade work
    let action_set = scanner.scan_all()?;
    let filter = match mode {
        UpgradeMode::Targeted(id, _) => Some(id),
        _ => None,
    };
    let drift = manifest.detect_drift(&action_set, filter);
    if !drift.is_empty() {
        let lines: Vec<String> = drift.iter().map(|d| format!("  - {d}")).collect();
        anyhow::bail!(
            "Workflows are out of sync with gx.toml:\n{}\nRun `gx tidy` first.",
            lines.join("\n")
        );
    }

    let service = ActionResolver::new(registry);

    let Some((upgrades, repins)) = determine_upgrades(&manifest, &service, mode)? else {
        return Ok(());
    };

    info!("Upgrading actions:");
    for upgrade in &upgrades {
        info!("+ {upgrade}");
        manifest.set(upgrade.id.clone(), upgrade.upgraded.clone());
    }

    for upgrade in &upgrades {
        let spec = ActionSpec::new(upgrade.id.clone(), upgrade.upgraded.clone());
        resolve_and_store(&service, &spec, &mut lock, "Could not resolve");
    }

    for spec in &repins {
        resolve_and_store(&service, spec, &mut lock, "Could not re-pin");
    }

    manifest_store.save(&manifest)?;
    let keys_to_retain: Vec<LockKey> = manifest.specs().iter().map(|s| LockKey::from(*s)).collect();
    lock.retain(&keys_to_retain);
    lock_store.save(&lock)?;

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

fn determine_upgrades<R: VersionRegistry>(
    manifest: &Manifest,
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
                    let tag_exists = tags.iter().any(|t| t.as_str() == version.as_str());
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

fn resolve_and_store<R: VersionRegistry>(
    service: &ActionResolver<R>,
    spec: &ActionSpec,
    lock: &mut Lock,
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
        let results = vec![UpdateResult {
            file: PathBuf::from("ci.yml"),
            changes: vec!["actions/checkout v4 -> v5".to_string()],
        }];
        print_update_results(&results);
    }

    #[test]
    fn test_run_errors_on_drift_with_version_mismatch() {
        use crate::domain::{
            InterpretedRef, WorkflowActionSet, WorkflowError, WorkflowScanner, WorkflowUpdater,
        };
        use crate::infrastructure::{MemoryLock, MemoryManifest};
        use std::collections::HashMap;
        use tempfile::TempDir;

        struct DummyRegistry;
        impl crate::domain::VersionRegistry for DummyRegistry {
            fn lookup_sha(
                &self,
                _: &ActionId,
                _: &Version,
            ) -> Result<crate::domain::CommitSha, crate::domain::ResolutionError> {
                Err(crate::domain::ResolutionError::TokenRequired)
            }
            fn tags_for_sha(
                &self,
                _: &ActionId,
                _: &crate::domain::CommitSha,
            ) -> Result<Vec<Version>, crate::domain::ResolutionError> {
                Err(crate::domain::ResolutionError::TokenRequired)
            }
            fn all_tags(
                &self,
                _: &ActionId,
            ) -> Result<Vec<Version>, crate::domain::ResolutionError> {
                Err(crate::domain::ResolutionError::TokenRequired)
            }
        }

        struct DummyUpdater;
        impl WorkflowUpdater for DummyUpdater {
            fn update_all(
                &self,
                _: &HashMap<ActionId, String>,
            ) -> Result<Vec<UpdateResult>, WorkflowError> {
                Ok(vec![])
            }
        }

        // Scanner returns checkout@v4
        struct CheckoutV4Scanner;
        impl WorkflowScanner for CheckoutV4Scanner {
            fn scan_all(&self) -> Result<WorkflowActionSet, WorkflowError> {
                let mut set = WorkflowActionSet::new();
                set.add(&InterpretedRef {
                    id: ActionId::from("actions/checkout"),
                    version: Version::from("v4"),
                    sha: None,
                });
                Ok(set)
            }
        }

        let temp_dir = TempDir::new().unwrap();

        // Manifest says v3, workflow says v4 → drift
        let mut manifest = Manifest::default();
        manifest.set(ActionId::from("actions/checkout"), Version::from("v3"));

        let lock = Lock::default();
        let lock_store = MemoryLock;

        let result = run(
            temp_dir.path(),
            manifest,
            MemoryManifest::default(),
            lock,
            lock_store,
            DummyRegistry,
            &CheckoutV4Scanner,
            &DummyUpdater,
            &UpgradeMode::Safe,
        );

        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("gx tidy"),
            "error should mention gx tidy, got: {msg}"
        );
        assert!(
            msg.contains("actions/checkout"),
            "error should name the drifted action, got: {msg}"
        );
    }

    #[test]
    fn test_run_targeted_ignores_drift_on_other_actions() {
        use crate::domain::{
            InterpretedRef, WorkflowActionSet, WorkflowError, WorkflowScanner, WorkflowUpdater,
        };
        use crate::infrastructure::{MemoryLock, MemoryManifest};
        use std::collections::HashMap;
        use tempfile::TempDir;

        struct DummyRegistry;
        impl crate::domain::VersionRegistry for DummyRegistry {
            fn lookup_sha(
                &self,
                _: &ActionId,
                _: &Version,
            ) -> Result<crate::domain::CommitSha, crate::domain::ResolutionError> {
                Err(crate::domain::ResolutionError::TokenRequired)
            }
            fn tags_for_sha(
                &self,
                _: &ActionId,
                _: &crate::domain::CommitSha,
            ) -> Result<Vec<Version>, crate::domain::ResolutionError> {
                Err(crate::domain::ResolutionError::TokenRequired)
            }
            fn all_tags(
                &self,
                _: &ActionId,
            ) -> Result<Vec<Version>, crate::domain::ResolutionError> {
                // Return v5 as a valid tag for checkout so targeted upgrade can verify it
                Ok(vec![Version::from("v5")])
            }
        }

        struct DummyUpdater;
        impl WorkflowUpdater for DummyUpdater {
            fn update_all(
                &self,
                _: &HashMap<ActionId, String>,
            ) -> Result<Vec<UpdateResult>, WorkflowError> {
                Ok(vec![])
            }
        }

        // Scanner returns checkout@v4 (matches manifest) and setup-node@v99 (drifted, but not targeted)
        struct TwoActionScanner;
        impl WorkflowScanner for TwoActionScanner {
            fn scan_all(&self) -> Result<WorkflowActionSet, WorkflowError> {
                let mut set = WorkflowActionSet::new();
                set.add(&InterpretedRef {
                    id: ActionId::from("actions/checkout"),
                    version: Version::from("v4"),
                    sha: None,
                });
                set.add(&InterpretedRef {
                    id: ActionId::from("actions/setup-node"),
                    version: Version::from("v99"), // drifted, but not the target
                    sha: None,
                });
                Ok(set)
            }
        }

        let temp_dir = TempDir::new().unwrap();

        // Manifest: checkout@v4 (matches), setup-node@v3 (drifted but not targeted)
        let mut manifest = Manifest::default();
        manifest.set(ActionId::from("actions/checkout"), Version::from("v4"));
        manifest.set(ActionId::from("actions/setup-node"), Version::from("v3"));

        let lock = Lock::default();
        let lock_store = MemoryLock;

        // Targeted upgrade on checkout — should NOT error even though setup-node is drifted
        let mode = UpgradeMode::Targeted(ActionId::from("actions/checkout"), Version::from("v5"));
        let result = run(
            temp_dir.path(),
            manifest,
            MemoryManifest::default(),
            lock,
            lock_store,
            DummyRegistry,
            &TwoActionScanner,
            &DummyUpdater,
            &mode,
        );

        // Should not error on drift (only checked checkout, which has no drift)
        if let Err(e) = &result {
            assert!(
                !e.to_string().contains("gx tidy"),
                "should not error on drift for other actions: {e}"
            );
        }
    }
}
