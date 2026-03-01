pub mod identity;
pub mod resolved;
pub mod spec;
pub mod upgrade;
pub mod uses_ref;

// Re-export all public types to maintain the same API surface
pub use identity::{ActionId, CommitSha, Version, VersionPrecision};
pub use resolved::{ResolvedAction, VersionCorrection};
pub use spec::{ActionSpec, LockKey};
pub use upgrade::{UpgradeAction, UpgradeCandidate, find_upgrade_candidate};
pub use uses_ref::{InterpretedRef, RefType, UsesRef};
