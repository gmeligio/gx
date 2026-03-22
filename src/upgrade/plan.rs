use crate::domain::action::identity::{ActionId, Version};
use crate::domain::action::resolved::ResolvedAction;
use crate::domain::action::spec::Spec as ActionSpec;
use crate::domain::action::upgrade::{
    Action as UpgradeAction, Candidate as UpgradeCandidate, find_upgrade_candidate,
};
use crate::domain::diff::{LockDiff, ManifestDiff, WorkflowPatch};
use crate::domain::lock::Lock;
use crate::domain::manifest::Manifest;
use crate::domain::resolution::{ActionResolver, Error as ResolutionError, VersionRegistry};
use crate::domain::workflow::Error as WorkflowError;
use crate::infra::workflow_update::WorkflowWriter;
use thiserror::Error;

use super::cli::{Mode as UpgradeMode, Request as UpgradeRequest, Scope as UpgradeScope};

/// The complete plan produced by an upgrade operation.
#[derive(Debug)]
pub struct Plan {
    pub manifest: ManifestDiff,
    /// The final lock state — written by `Store::save()`.
    pub lock: Lock,
    /// The diff between the original and planned lock — for reporting only.
    pub lock_changes: LockDiff,
    pub workflows: Vec<WorkflowPatch>,
    pub upgrades: Vec<UpgradeCandidate>,
}

impl Plan {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.manifest.is_empty() && self.lock_changes.is_empty() && self.workflows.is_empty()
    }
}

/// Errors that can occur during the upgrade command.
#[derive(Debug, Error)]
pub enum UpgradeError {
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
        source: Box<ResolutionError>,
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
pub fn plan<R, F: FnMut(&str)>(
    manifest: &Manifest,
    lock: &Lock,
    registry: &R,
    request: &UpgradeRequest,
    mut on_progress: F,
) -> Result<Plan, UpgradeError>
where
    R: VersionRegistry,
{
    let service = ActionResolver::new(registry);

    let Some((upgrades, repins)) =
        determine_upgrades(manifest, lock, &service, request, &mut on_progress)?
    else {
        return Ok(Plan {
            manifest: ManifestDiff::default(),
            lock: lock.clone(),
            lock_changes: LockDiff::default(),
            workflows: vec![],
            upgrades: vec![],
        });
    };

    // Work on clones to compute the planned state
    let mut planned_manifest = manifest.clone();
    let mut planned_lock = lock.clone();

    for upgrade in &upgrades {
        if let UpgradeAction::CrossRange { new_specifier, .. } = &upgrade.action {
            planned_manifest.set(upgrade.id.clone(), new_specifier.clone());
        }
    }

    for upgrade in &upgrades {
        let version_to_resolve = match &upgrade.action {
            UpgradeAction::InRange { .. } => upgrade.current.clone(),
            UpgradeAction::CrossRange { new_specifier, .. } => new_specifier.clone(),
        };
        let spec = ActionSpec::new(upgrade.id.clone(), version_to_resolve);
        resolve_and_store(
            &service,
            &spec,
            &mut planned_lock,
            "Could not resolve",
            &mut on_progress,
        );
    }

    for spec in &repins {
        resolve_and_store(
            &service,
            spec,
            &mut planned_lock,
            "Could not re-pin",
            &mut on_progress,
        );
    }

    let keys_to_retain: Vec<ActionSpec> = planned_manifest.specs().cloned().collect();
    planned_lock.retain(&keys_to_retain);

    // Diff original vs planned
    let manifest_diff = manifest.diff(&planned_manifest);
    let lock_diff = lock.diff(&planned_lock);

    Ok(Plan {
        manifest: manifest_diff,
        lock: planned_lock,
        lock_changes: lock_diff,
        workflows: vec![], // Workflow patches computed during apply phase
        upgrades,
    })
}

/// Result type for the `determine_upgrades` function.
type DetermineResult = Option<(Vec<UpgradeCandidate>, Vec<ActionSpec>)>;

/// # Errors
///
/// Returns [`UpgradeError::ActionNotInManifest`] if the target action is not in the manifest.
/// Returns [`UpgradeError::TagNotFound`] if the pinned version tag does not exist.
/// Returns [`UpgradeError::TagFetchFailed`] if tags cannot be fetched from the registry.
fn determine_upgrades<R: VersionRegistry>(
    manifest: &Manifest,
    lock: &Lock,
    service: &ActionResolver<'_, R>,
    request: &UpgradeRequest,
    on_progress: &mut dyn FnMut(&str),
) -> Result<DetermineResult, UpgradeError> {
    match &request.scope {
        UpgradeScope::Pinned(id, version) => {
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
                        source: Box::new(e),
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
        UpgradeScope::All | UpgradeScope::Single(_) => {
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

            on_progress("Checking for upgrades...");
            let mut upgrades = Vec::new();
            let mut repins: Vec<ActionSpec> = Vec::new();

            for spec in &specs {
                if spec.specifier.precision().is_none() {
                    if spec.specifier.is_sha() {
                        on_progress(&format!("Skipping {spec} (bare SHA)"));
                    } else {
                        on_progress(&format!("Re-pinning {spec} (non-semver ref)"));
                        repins.push((*spec).clone());
                    }
                    continue;
                }

                match service.registry().all_tags(&spec.id) {
                    Ok(tags) => {
                        // Get lock version as floor (if entry exists)
                        let lock_version = lock.get(spec).map(|entry| entry.version.clone());

                        let allow_major = matches!(request.mode, UpgradeMode::Latest);
                        let action = find_upgrade_candidate(
                            &spec.specifier,
                            lock_version.as_ref(),
                            &tags,
                            allow_major,
                        );

                        if let Some(upgrade_action) = action {
                            upgrades.push(UpgradeCandidate {
                                id: spec.id.clone(),
                                current: spec.specifier.clone(),
                                action: upgrade_action,
                            });
                        }
                    }
                    Err(e) => {
                        on_progress(&format!(
                            "Warning: could not check upgrades for {spec}: {e}"
                        ));
                    }
                }
            }

            if upgrades.is_empty() && repins.is_empty() {
                return Ok(None);
            }

            Ok(Some((upgrades, repins)))
        }
    }
}

/// Resolve an action and store the result in the upgrade plan.
pub(super) fn resolve_and_store<R: VersionRegistry>(
    service: &ActionResolver<'_, R>,
    spec: &ActionSpec,
    lock: &mut Lock,
    unresolved_msg: &str,
    on_progress: &mut dyn FnMut(&str),
) {
    match service.resolve(spec) {
        Ok(resolved) => {
            lock.set(spec, resolved.version, resolved.commit);
        }
        Err(e) => {
            on_progress(&format!("{unresolved_msg} {spec}: {e}"));
        }
    }
}

/// Apply upgrade plan's workflow updates: update all workflow files with new lock entries.
///
/// # Errors
///
/// Returns [`UpgradeError::Workflow`] if workflow files cannot be updated.
pub fn apply_upgrade_workflows(
    writer: &WorkflowWriter,
    lock_diff: &LockDiff,
    upgrades: &[UpgradeCandidate],
) -> Result<usize, UpgradeError> {
    let pins: Vec<ResolvedAction> = lock_diff
        .added
        .iter()
        .map(|(key, entry)| ResolvedAction {
            id: key.id.clone(),
            sha: entry.commit.sha.clone(),
            version: if key.specifier.is_sha() {
                None
            } else {
                Some(entry.version.clone())
            },
        })
        .collect();

    if pins.is_empty() {
        return Ok(0);
    }

    let results = writer.update_all_with_pins(&pins)?;

    let _: &[UpgradeCandidate] = upgrades;

    Ok(results.len())
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    reason = "tests use unwrap, indexing, and other patterns freely"
)]
mod tests {
    use super::{Lock, Manifest, UpgradeMode, UpgradeRequest, UpgradeScope, plan};
    use crate::domain::action::identity::{ActionId, CommitDate, CommitSha, Repository, Version};
    use crate::domain::action::resolved::Commit;
    use crate::domain::action::spec::Spec as ActionSpec;
    use crate::domain::action::specifier::Specifier;
    use crate::domain::action::uses_ref::RefType;
    use crate::domain::resolution::testutil::FakeRegistry;

    #[test]
    fn plan_no_upgradable_actions_returns_empty() {
        let mut manifest = Manifest::default();
        manifest.set(ActionId::from("actions/checkout"), Specifier::parse("^4"));

        let mut lock = Lock::default();
        lock.set(
            &ActionSpec::new(ActionId::from("actions/checkout"), Specifier::parse("^4")),
            Version::from("v4"),
            Commit {
                sha: CommitSha::from("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
                repository: Repository::from("actions/checkout"),
                ref_type: Some(RefType::Tag),
                date: CommitDate::from("2026-01-01T00:00:00Z"),
            },
        );

        // Registry returns no tags → nothing to upgrade
        let registry = FakeRegistry::new();
        let request = UpgradeRequest::new(UpgradeMode::Safe, UpgradeScope::All);

        let result = plan(&manifest, &lock, &registry, &request, |_| {}).unwrap();
        assert!(
            result.is_empty(),
            "Plan with no upgradable actions must be empty"
        );
    }

    #[test]
    fn plan_one_upgradable_action_produces_diffs() {
        let mut manifest = Manifest::default();
        manifest.set(ActionId::from("actions/checkout"), Specifier::parse("^4"));

        let mut lock = Lock::default();
        lock.set(
            &ActionSpec::new(ActionId::from("actions/checkout"), Specifier::parse("^4")),
            Version::from("v4"),
            Commit {
                sha: CommitSha::from("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
                repository: Repository::from("actions/checkout"),
                ref_type: Some(RefType::Tag),
                date: CommitDate::from("2026-01-01T00:00:00Z"),
            },
        );
        lock.set_version(
            &ActionSpec::new(ActionId::from("actions/checkout"), Specifier::parse("^4")),
            Some("v4.1.0".to_owned()),
        );

        // Registry has v4.2.0 available (in-range upgrade from v4)
        let registry =
            FakeRegistry::new().with_all_tags("actions/checkout", vec!["v4", "v4.1.0", "v4.2.0"]);

        let request = UpgradeRequest::new(UpgradeMode::Safe, UpgradeScope::All);

        let result = plan(&manifest, &lock, &registry, &request, |_| {}).unwrap();

        // Should have upgrade candidate
        assert!(
            !result.upgrades.is_empty(),
            "Plan must include upgrade candidates, got none"
        );

        // Lock changes should have a new entry for the upgraded version
        assert!(
            !result.lock_changes.added.is_empty(),
            "Plan must include lock additions for resolved upgrade, got: {:?}",
            result.lock_changes
        );
    }

    #[test]
    fn plan_latest_mode_produces_major_version_bump() {
        let mut manifest = Manifest::default();
        manifest.set(ActionId::from("actions/checkout"), Specifier::parse("^3"));

        let mut lock = Lock::default();
        lock.set(
            &ActionSpec::new(ActionId::from("actions/checkout"), Specifier::parse("^3")),
            Version::from("v3"),
            Commit {
                sha: CommitSha::from("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
                repository: Repository::from("actions/checkout"),
                ref_type: Some(RefType::Tag),
                date: CommitDate::from("2026-01-01T00:00:00Z"),
            },
        );
        lock.set_version(
            &ActionSpec::new(ActionId::from("actions/checkout"), Specifier::parse("^3")),
            Some("v3.0.0".to_owned()),
        );

        // Registry has v4 available (cross-range)
        let registry = FakeRegistry::new()
            .with_all_tags("actions/checkout", vec!["v3", "v3.0.0", "v4", "v4.0.0"]);

        let request = UpgradeRequest::new(UpgradeMode::Latest, UpgradeScope::All);

        let result = plan(&manifest, &lock, &registry, &request, |_| {}).unwrap();

        // Should have upgrade candidates
        assert!(
            !result.upgrades.is_empty(),
            "Latest mode plan must include upgrade candidates"
        );

        // Manifest should show the version change (^3 → ^4)
        let has_manifest_change = result.manifest.updated.iter().any(|(id, v)| {
            id == &ActionId::from("actions/checkout") && v == &Specifier::parse("^4")
        });
        assert!(
            has_manifest_change,
            "Latest mode plan must include manifest version bump to v4, got: {:?}",
            result.manifest.updated
        );
    }
}
