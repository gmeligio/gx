use crate::domain::action::identity::{ActionId, Version};
use crate::domain::action::upgrade::Candidate as UpgradeCandidate;
use crate::domain::lock::Lock;
use crate::domain::plan::{LockDiff, ManifestDiff, WorkflowPatch};
use crate::domain::resolution::Error as ResolutionError;
use crate::domain::workflow::Error as WorkflowError;
use thiserror::Error;

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

/// Which actions to upgrade: all, a single action, or a pinned action+version.
#[non_exhaustive]
#[derive(Debug, Clone)]
pub enum Scope {
    /// Upgrade all actions in the manifest.
    All,
    /// Upgrade a single action by ID.
    Single(ActionId),
    /// Pin a specific action to an exact version.
    Pinned(ActionId, Version),
}

/// How the upgrade command should find new versions.
#[non_exhaustive]
#[derive(Debug)]
pub enum Mode {
    /// Default: upgrade within the current major version.
    Safe,
    /// Upgrade to the absolute latest version, including major versions.
    Latest,
}

/// A request to upgrade actions with a specific mode and scope.
#[derive(Debug)]
pub struct Request {
    pub mode: Mode,
    pub scope: Scope,
}

impl Request {
    /// Create a new upgrade request.
    #[must_use]
    pub fn new(mode: Mode, scope: Scope) -> Self {
        Self { mode, scope }
    }
}

/// Errors that can occur during the upgrade command.
#[derive(Debug, Error)]
pub enum Error {
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
    use super::{ActionId, Mode, Request, Scope, Version};

    #[test]
    fn new_should_accept_pinned_scope() {
        let req = Request::new(
            Mode::Safe,
            Scope::Pinned(ActionId::from("actions/checkout"), Version::from("v5")),
        );
        assert!(matches!(req.scope, Scope::Pinned(_, _)));
    }

    #[test]
    fn new_should_accept_safe_all() {
        let req = Request::new(Mode::Safe, Scope::All);
        assert!(matches!(req.mode, Mode::Safe));
        assert!(matches!(req.scope, Scope::All));
    }
}
