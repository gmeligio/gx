pub mod action;
pub mod lock;
pub mod manifest;
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
pub use resolution::{
    ActionResolver, ResolutionError, ResolutionResult, ResolvedRef, VersionRegistry,
    populate_resolved_fields,
};
pub use workflow::{
    UpdateResult, WorkflowError, WorkflowScanner, WorkflowScannerLocated, WorkflowUpdater,
};
pub use workflow_actions::{LocatedAction, WorkflowActionSet, WorkflowLocation};
