pub mod action;
pub mod resolution;
pub mod version;

pub use action::{ActionId, ActionSpec, CommitSha, LockKey, ResolvedAction, Version};
pub use resolution::{
    ResolutionError, ResolutionResult, ResolutionService, VersionResolver, select_highest_version,
};
pub use version::{find_highest_version, is_commit_sha};
