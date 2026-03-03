pub mod action;
pub mod lock;
pub mod manifest;
pub mod plan;
pub mod resolution;
pub mod workflow;
pub mod workflow_actions;

pub use action::{
    ActionId, ActionSpec, CommitSha, InterpretedRef, LockKey, RefType, ResolvedAction,
    UpgradeAction, UpgradeCandidate, UsesRef, Version, VersionCorrection, VersionPrecision,
    find_upgrade_candidate,
};
pub use lock::{Lock, LockEntry};
pub use manifest::{ActionOverride, Manifest};
pub use plan::{LockDiff, LockEntryPatch, ManifestDiff, TidyPlan, UpgradePlan, WorkflowPatch};
pub use resolution::select_most_specific_tag;
pub use resolution::{
    ActionResolver, ResolutionError, ResolvedRef, ShaDescription, VersionRegistry,
};
pub use workflow::{UpdateResult, WorkflowError, WorkflowScanner, WorkflowUpdater};
pub use workflow_actions::{LocatedAction, WorkflowActionSet, WorkflowLocation};
