use log::{info, warn};
use thiserror::Error;

use crate::domain::UpgradePlan as DomainUpgradePlan;
use crate::domain::{
    ActionId, ActionResolver, ActionSpec, Lock, LockDiff, LockKey, Manifest, ManifestDiff,
    ResolutionError, UpdateResult, UpgradeAction, UpgradeCandidate, Version, VersionRegistry,
    WorkflowError, WorkflowUpdater, find_upgrade_candidate,
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

    /// Workflow files could not be updated.
    #[error(transparent)]
    Workflow(#[from] WorkflowError),
}

/// Compute an `UpgradePlan` describing all changes without modifying the original manifest or lock.
///
/// # Errors
///
/// Returns [`UpgradeError::ActionNotInManifest`] if the target action is not in the manifest.
/// Returns [`UpgradeError::TagNotFound`] if the pinned version tag does not exist.
/// Returns [`UpgradeError::TagFetchFailed`] if tags cannot be fetched from the registry.
#[allow(clippy::needless_pass_by_value)]
pub fn plan<R>(
    manifest: &Manifest,
    lock: &Lock,
    registry: R,
    request: &UpgradeRequest,
) -> Result<DomainUpgradePlan, UpgradeError>
where
    R: VersionRegistry,
{
    let service = ActionResolver::new(registry);

    let Some((upgrades, repins)) = determine_upgrades(manifest, lock, &service, request)? else {
        return Ok(DomainUpgradePlan {
            manifest: ManifestDiff::default(),
            lock: LockDiff::default(),
            workflows: vec![],
            upgrades: vec![],
        });
    };

    // Work on clones to compute the planned state
    let mut planned_manifest = manifest.clone();
    let mut planned_lock = lock.clone();

    for upgrade in &upgrades {
        if let UpgradeAction::CrossRange {
            new_manifest_version,
            ..
        } = &upgrade.action
        {
            planned_manifest.set(upgrade.id.clone(), new_manifest_version.clone());
        }
    }

    for upgrade in &upgrades {
        let version_to_resolve = match &upgrade.action {
            UpgradeAction::InRange { .. } => upgrade.current.clone(),
            UpgradeAction::CrossRange {
                new_manifest_version,
                ..
            } => new_manifest_version.clone(),
        };
        let spec = ActionSpec::new(upgrade.id.clone(), version_to_resolve);
        resolve_and_store(&service, &spec, &mut planned_lock, "Could not resolve");
    }

    for spec in &repins {
        resolve_and_store(&service, spec, &mut planned_lock, "Could not re-pin");
    }

    let keys_to_retain: Vec<LockKey> = planned_manifest.specs().map(LockKey::from).collect();
    planned_lock.retain(&keys_to_retain);

    // Diff original vs planned
    let manifest_diff = diff_manifests(manifest, &planned_manifest);
    let lock_diff = diff_locks(lock, &planned_lock);

    Ok(DomainUpgradePlan {
        manifest: manifest_diff,
        lock: lock_diff,
        workflows: vec![], // Workflow patches computed during apply phase
        upgrades,
    })
}

/// Diff two manifest states to produce a `ManifestDiff`.
fn diff_manifests(before: &Manifest, after: &Manifest) -> ManifestDiff {
    use std::collections::HashSet;

    let before_ids: HashSet<ActionId> = before.specs().map(|s| s.id.clone()).collect();
    let after_ids: HashSet<ActionId> = after.specs().map(|s| s.id.clone()).collect();

    // Detect version changes (present in both but version differs)
    let mut added: Vec<(ActionId, Version)> = Vec::new();
    let mut removed: Vec<ActionId> = Vec::new();

    // New actions
    for id in after_ids.difference(&before_ids) {
        if let Some(v) = after.get(id) {
            added.push((id.clone(), v.clone()));
        }
    }

    // Removed actions
    for id in before_ids.difference(&after_ids) {
        removed.push(id.clone());
    }

    // Version changes (same action, different version)
    let mut updated: Vec<(ActionId, Version)> = Vec::new();
    for id in before_ids.intersection(&after_ids) {
        let before_v = before.get(id);
        let after_v = after.get(id);
        if before_v != after_v
            && let Some(v) = after_v
        {
            updated.push((id.clone(), v.clone()));
        }
    }

    ManifestDiff {
        added,
        removed,
        updated,
        overrides_added: vec![],
        overrides_removed: vec![],
    }
}

/// Diff two lock states to produce a `LockDiff`.
///
/// Entries with the same key but different SHAs are treated as replacements
/// (removed + added) since the entire entry needs to be rewritten.
fn diff_locks(before: &Lock, after: &Lock) -> LockDiff {
    use std::collections::HashSet;

    let before_keys: HashSet<LockKey> = before.entries().map(|(k, _)| k.clone()).collect();
    let after_keys: HashSet<LockKey> = after.entries().map(|(k, _)| k.clone()).collect();

    let mut added: Vec<(LockKey, crate::domain::LockEntry)> = after_keys
        .difference(&before_keys)
        .filter_map(|k| after.get(k).map(|e| (k.clone(), e.clone())))
        .collect();

    let mut removed: Vec<LockKey> = before_keys.difference(&after_keys).cloned().collect();

    // Detect changed entries (same key, different SHA) and treat as replace
    for key in before_keys.intersection(&after_keys) {
        if let (Some(b), Some(a)) = (before.get(key), after.get(key))
            && b.sha != a.sha
        {
            removed.push(key.clone());
            added.push((key.clone(), a.clone()));
        }
    }

    LockDiff {
        added,
        removed,
        updated: vec![],
    }
}

type DetermineResult = Option<(Vec<UpgradeCandidate>, Vec<ActionSpec>)>;

/// # Errors
///
/// Returns [`UpgradeError::ActionNotInManifest`] if the target action is not in the manifest.
/// Returns [`UpgradeError::TagNotFound`] if the pinned version tag does not exist.
/// Returns [`UpgradeError::TagFetchFailed`] if tags cannot be fetched from the registry.
fn determine_upgrades<R: VersionRegistry>(
    manifest: &Manifest,
    lock: &Lock,
    service: &ActionResolver<R>,
    request: &UpgradeRequest,
) -> Result<DetermineResult, UpgradeError> {
    match &request.mode {
        UpgradeMode::Safe | UpgradeMode::Latest => {
            let mut specs: Vec<&ActionSpec> = manifest.specs().collect();

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
                        // Get lock version as floor (if entry exists)
                        let lock_key = LockKey::from(*spec);
                        let lock_version_str =
                            lock.get(&lock_key).and_then(|entry| entry.version.as_ref());
                        let lock_version = lock_version_str.map(|v| Version::from(v.as_str()));

                        let allow_major = matches!(request.mode, UpgradeMode::Latest);
                        let action = find_upgrade_candidate(
                            &spec.version,
                            lock_version.as_ref(),
                            &tags,
                            allow_major,
                        );

                        if let Some(upgrade_action) = action {
                            upgrades.push(UpgradeCandidate {
                                id: spec.id.clone(),
                                current: spec.version.clone(),
                                action: upgrade_action,
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
                    action: UpgradeAction::InRange {
                        candidate: version.clone(),
                    },
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
        Ok(resolved) => {
            lock.set(&resolved);
        }
        Err(e) => {
            warn!("{unresolved_msg} {spec}: {e}");
        }
    }
}

/// Apply upgrade plan's workflow updates: update all workflow files with new lock entries.
///
/// # Errors
///
/// Returns [`UpgradeError::Workflow`] if workflow files cannot be updated.
pub fn apply_upgrade_workflows<W: WorkflowUpdater>(
    writer: &W,
    lock_diff: &LockDiff,
    upgrades: &[UpgradeCandidate],
) -> Result<(), UpgradeError> {
    use crate::domain::LockEntry;

    let update_map: std::collections::HashMap<ActionId, String> = lock_diff
        .added
        .iter()
        .map(|(key, entry): &(LockKey, LockEntry)| {
            let ref_str = if key.version.is_sha() {
                entry.sha.to_string()
            } else {
                format!("{} # {}", entry.sha, key.version)
            };
            (key.id.clone(), ref_str)
        })
        .collect();

    if update_map.is_empty() {
        return Ok(());
    }

    let results = writer.update_all(&update_map)?;
    print_update_results(&results);

    if !upgrades.is_empty() {
        info!("Upgrading actions:");
        for u in upgrades {
            info!("+ {u}");
        }
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

    // ========== Step 9: upgrade::plan() tests ==========

    use crate::domain::{CommitSha, RefType, ResolvedAction, ResolvedRef};

    struct MockPlanRegistry {
        tags: std::collections::HashMap<String, Vec<String>>,
    }

    impl MockPlanRegistry {
        fn new() -> Self {
            Self {
                tags: std::collections::HashMap::new(),
            }
        }

        fn with_tags(mut self, action: &str, tags: Vec<&str>) -> Self {
            self.tags.insert(
                action.to_string(),
                tags.into_iter().map(String::from).collect(),
            );
            self
        }
    }

    impl VersionRegistry for MockPlanRegistry {
        fn lookup_sha(
            &self,
            id: &ActionId,
            version: &Version,
        ) -> Result<ResolvedRef, crate::domain::ResolutionError> {
            let sha = format!("{}{}", id.as_str(), version.as_str()).replace('/', "");
            let padded = format!("{:0<40}", &sha[..sha.len().min(40)]);
            Ok(ResolvedRef::new(
                CommitSha::from(padded),
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
            Ok(vec![])
        }

        fn all_tags(&self, id: &ActionId) -> Result<Vec<Version>, crate::domain::ResolutionError> {
            Ok(self
                .tags
                .get(id.as_str())
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .map(Version::from)
                .collect())
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

    #[test]
    fn test_plan_no_upgradable_actions_returns_empty() {
        let mut manifest = Manifest::default();
        manifest.set(ActionId::from("actions/checkout"), Version::from("v4"));

        let mut lock = Lock::default();
        lock.set(&ResolvedAction::new(
            ActionId::from("actions/checkout"),
            Version::from("v4"),
            CommitSha::from("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
            "actions/checkout".to_string(),
            RefType::Tag,
            "2026-01-01T00:00:00Z".to_string(),
        ));

        // Registry returns no tags → nothing to upgrade
        let registry = MockPlanRegistry::new();
        let request = UpgradeRequest::new(UpgradeMode::Safe, UpgradeScope::All).unwrap();

        let result = plan(&manifest, &lock, registry, &request).unwrap();
        assert!(
            result.is_empty(),
            "Plan with no upgradable actions must be empty"
        );
    }

    #[test]
    fn test_plan_one_upgradable_action_produces_diffs() {
        let mut manifest = Manifest::default();
        manifest.set(ActionId::from("actions/checkout"), Version::from("v4"));

        let mut lock = Lock::default();
        lock.set(&ResolvedAction::new(
            ActionId::from("actions/checkout"),
            Version::from("v4"),
            CommitSha::from("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
            "actions/checkout".to_string(),
            RefType::Tag,
            "2026-01-01T00:00:00Z".to_string(),
        ));
        lock.set_version(
            &LockKey::new(ActionId::from("actions/checkout"), Version::from("v4")),
            Some("v4.1.0".to_string()),
        );

        // Registry has v4.2.0 available (in-range upgrade from v4)
        let registry =
            MockPlanRegistry::new().with_tags("actions/checkout", vec!["v4", "v4.1.0", "v4.2.0"]);

        let request = UpgradeRequest::new(UpgradeMode::Safe, UpgradeScope::All).unwrap();

        let result = plan(&manifest, &lock, registry, &request).unwrap();

        // Should have upgrade candidate
        assert!(
            !result.upgrades.is_empty(),
            "Plan must include upgrade candidates, got none"
        );

        // Lock should have a new entry for the upgraded version
        assert!(
            !result.lock.added.is_empty(),
            "Plan must include lock additions for resolved upgrade, got: {:?}",
            result.lock
        );
    }

    #[test]
    fn test_plan_latest_mode_produces_major_version_bump() {
        let mut manifest = Manifest::default();
        manifest.set(ActionId::from("actions/checkout"), Version::from("v3"));

        let mut lock = Lock::default();
        lock.set(&ResolvedAction::new(
            ActionId::from("actions/checkout"),
            Version::from("v3"),
            CommitSha::from("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
            "actions/checkout".to_string(),
            RefType::Tag,
            "2026-01-01T00:00:00Z".to_string(),
        ));
        lock.set_version(
            &LockKey::new(ActionId::from("actions/checkout"), Version::from("v3")),
            Some("v3.0.0".to_string()),
        );

        // Registry has v4 available (cross-range)
        let registry = MockPlanRegistry::new()
            .with_tags("actions/checkout", vec!["v3", "v3.0.0", "v4", "v4.0.0"]);

        let request = UpgradeRequest::new(UpgradeMode::Latest, UpgradeScope::All).unwrap();

        let result = plan(&manifest, &lock, registry, &request).unwrap();

        // Should have upgrade candidates
        assert!(
            !result.upgrades.is_empty(),
            "Latest mode plan must include upgrade candidates"
        );

        // Manifest should show the version change (v3 → v4)
        let has_manifest_change =
            result.manifest.updated.iter().any(|(id, v)| {
                id == &ActionId::from("actions/checkout") && v == &Version::from("v4")
            });
        assert!(
            has_manifest_change,
            "Latest mode plan must include manifest version bump to v4, got: {:?}",
            result.manifest.updated
        );
    }
}
