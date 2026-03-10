use crate::domain::{
    ActionId, LockDiff, ManifestDiff, ResolutionError, UpgradeCandidate, Version, WorkflowError,
    WorkflowPatch,
};
use thiserror::Error;

/// The complete plan produced by an upgrade operation.
#[derive(Debug)]
pub struct UpgradePlan {
    pub manifest: ManifestDiff,
    pub lock: LockDiff,
    pub workflows: Vec<WorkflowPatch>,
    pub upgrades: Vec<UpgradeCandidate>,
}

impl UpgradePlan {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.manifest.is_empty() && self.lock.is_empty() && self.workflows.is_empty()
    }
}

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
        source: Box<ResolutionError>,
    },

    /// Workflow files could not be updated.
    #[error(transparent)]
    Workflow(#[from] WorkflowError),
}

#[cfg(test)]
mod tests {
    use super::{ActionId, UpgradeMode, UpgradeRequest, UpgradeScope, Version};

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
