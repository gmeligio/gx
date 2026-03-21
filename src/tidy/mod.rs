#![expect(clippy::pub_use, reason = "reexport from extracted submodule")]

/// Tidy command: error types, struct, and `Command` implementation.
mod command;
/// Lock file synchronization: resolving and updating lock entries.
mod lock_sync;
/// Manifest synchronization: adding, removing, and upgrading action specs.
mod manifest_sync;
/// Workflow patch computation for updating pinned SHAs in workflow files.
mod patches;
pub mod report;

pub use command::{Error, Plan, RunError, Tidy, apply_workflow_patches, plan};
