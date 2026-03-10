pub mod action;
pub mod event;
pub mod lock;
pub mod manifest;
pub mod plan;
pub mod resolution;
pub mod workflow;
pub mod workflow_actions;

/// Wraps a parsed value with a flag indicating whether format migration occurred.
#[derive(Debug)]
pub struct Parsed<T> {
    pub value: T,
    pub migrated: bool,
}

pub use action::{
    ActionId, ActionSpec, CommitSha, InterpretedRef, LockKey, RefType, ResolvedAction, ShaIndex,
    Specifier, UpgradeAction, UpgradeCandidate, UsesRef, Version, VersionCorrection,
    VersionPrecision, find_upgrade_candidate, select_most_specific_tag,
};
pub use event::SyncEvent;
pub use lock::{Lock, LockEntry};
pub use manifest::{ActionOverride, Manifest};
pub use plan::{LockDiff, LockEntryPatch, ManifestDiff, WorkflowPatch};
pub use resolution::{
    ActionResolver, ResolutionError, ResolvedRef, ShaDescription, VersionRegistry,
};
pub use workflow::{UpdateResult, WorkflowError, WorkflowScanner, WorkflowUpdater};
pub use workflow_actions::{LocatedAction, WorkflowActionSet, WorkflowLocation};
