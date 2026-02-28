use log::{info, warn};
use std::path::Path;
use thiserror::Error;

use crate::domain::{
    ActionId, ActionResolver, ActionSpec, Lock, LockKey, Manifest, ResolutionError,
    ResolutionResult, UpdateResult, UpgradeCandidate, Version, VersionRegistry, WorkflowUpdater,
};
use crate::infrastructure::{
    LockFileError, LockStore, ManifestError, ManifestStore, WorkflowError,
};

/// Which actions to upgrade: all or a single action.
#[non_exhaustive]
#[derive(Debug, Clone)]
pub enum UpgradeScope {
    /// Upgrade all actions in the manifest.
    All,
    /// Upgrade a single action by ID.
    Single(ActionId),
}

/// How the upgrade command should find new versions.
#[non_exhaustive]
#[derive(Debug)]
pub enum UpgradeMode {
    /// Default: upgrade within the current major version.
    Safe,
    /// Upgrade to the absolute latest version, including major versions.
    Latest,
    /// Upgrade to a specific version (only valid with Single scope).
    Pinned(Version),
}

/// A request to upgrade actions with a specific mode and scope.
#[derive(Debug)]
pub struct UpgradeRequest {
    pub mode: UpgradeMode,
    pub scope: UpgradeScope,
}

impl UpgradeRequest {
    /// Create a new upgrade request, validating that Pinned mode requires Single scope.
    ///
    /// # Errors
    ///
    /// Returns [`UpgradeError::PinnedRequiresSingleScope`] if `mode` is `Pinned` and `scope` is `All`.
    pub fn new(mode: UpgradeMode, scope: UpgradeScope) -> Result<Self, UpgradeError> {
        if matches!((&mode, &scope), (UpgradeMode::Pinned(_), UpgradeScope::All)) {
            return Err(UpgradeError::PinnedRequiresSingleScope);
        }
        Ok(Self { mode, scope })
    }
}

/// Errors that can occur during the upgrade command
#[derive(Debug, Error)]
pub enum UpgradeError {
    /// Pinned mode was used without specifying a single action target.
    #[error("pinned mode requires a single action target (e.g., actions/checkout@v5)")]
    PinnedRequiresSingleScope,

    /// The specified action was not found in the manifest.
    #[error("{0} not found in manifest")]
    ActionNotInManifest(ActionId),

    /// The specified version tag does not exist in the registry for the action.
    #[error("{version} not found in registry for {id}")]
    TagNotFound { id: ActionId, version: Version },

    /// Could not fetch tags from the registry for the action.
    #[error("could not fetch tags for {id}")]
    TagFetchFailed {
        id: ActionId,
        #[source]
        source: ResolutionError,
    },

    /// The manifest store failed to save.
    #[error(transparent)]
    Manifest(#[from] ManifestError),

    /// The lock store failed to save.
    #[error(transparent)]
    Lock(#[from] LockFileError),

    /// Workflow files could not be updated.
    #[error(transparent)]
    Workflow(#[from] WorkflowError),
}

/// Run the upgrade command to find and apply available upgrades for actions.
///
/// The manifest is the source of truth — upgrade proceeds from it unconditionally.
///
/// # Errors
///
/// Returns [`UpgradeError::Manifest`] if the manifest cannot be saved.
/// Returns [`UpgradeError::Lock`] if the lock file cannot be saved.
/// Returns [`UpgradeError::Workflow`] if workflow files cannot be updated.
/// Propagates errors from [`determine_upgrades`].
#[allow(clippy::too_many_arguments, clippy::needless_pass_by_value)]
pub fn run<M, L, R, W>(
    _repo_root: &Path,
    mut manifest: Manifest,
    manifest_store: M,
    mut lock: Lock,
    lock_store: L,
    registry: R,
    writer: &W,
    request: &UpgradeRequest,
) -> Result<(), UpgradeError>
where
    M: ManifestStore,
    L: LockStore,
    R: VersionRegistry,
    W: WorkflowUpdater,
{
    let service = ActionResolver::new(registry);

    let Some((upgrades, repins)) = determine_upgrades(&manifest, &service, request)? else {
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

type UpgradePlan = Option<(Vec<UpgradeCandidate>, Vec<ActionSpec>)>;

/// # Errors
///
/// Returns [`UpgradeError::ActionNotInManifest`] if the target action is not in the manifest.
/// Returns [`UpgradeError::TagNotFound`] if the pinned version tag does not exist.
/// Returns [`UpgradeError::TagFetchFailed`] if tags cannot be fetched from the registry.
fn determine_upgrades<R: VersionRegistry>(
    manifest: &Manifest,
    service: &ActionResolver<R>,
    request: &UpgradeRequest,
) -> Result<UpgradePlan, UpgradeError> {
    match &request.mode {
        UpgradeMode::Safe | UpgradeMode::Latest => {
            let mut specs = manifest.specs();

            // Filter to a single action if scope requires it
            if let UpgradeScope::Single(target_id) = &request.scope {
                specs.retain(|s| &s.id == target_id);
                if specs.is_empty() {
                    return Err(UpgradeError::ActionNotInManifest(target_id.clone()));
                }
            }

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
                        let new_version = match &request.mode {
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
        UpgradeMode::Pinned(version) => {
            let id = match &request.scope {
                UpgradeScope::Single(id) => id,
                UpgradeScope::All => {
                    unreachable!("Pinned + All should be rejected in UpgradeRequest::new")
                }
            };

            let current = manifest
                .get(id)
                .ok_or_else(|| UpgradeError::ActionNotInManifest(id.clone()))?;

            match service.registry().all_tags(id) {
                Ok(tags) => {
                    let tag_exists = tags.iter().any(|t| t.as_str() == version.as_str());
                    if !tag_exists {
                        return Err(UpgradeError::TagNotFound {
                            id: id.clone(),
                            version: version.clone(),
                        });
                    }
                }
                Err(e) => {
                    return Err(UpgradeError::TagFetchFailed {
                        id: id.clone(),
                        source: e,
                    });
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
    fn new_should_reject_pinned_with_all_scope() {
        let err = UpgradeRequest::new(UpgradeMode::Pinned(Version::from("v5")), UpgradeScope::All)
            .unwrap_err();
        assert_eq!(
            err.to_string(),
            "pinned mode requires a single action target (e.g., actions/checkout@v5)"
        );
    }

    #[test]
    fn new_should_accept_pinned_with_single_scope() {
        let result = UpgradeRequest::new(
            UpgradeMode::Pinned(Version::from("v5")),
            UpgradeScope::Single(ActionId::from("actions/checkout")),
        );
        assert!(result.is_ok());
    }
}
