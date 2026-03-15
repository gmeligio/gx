use crate::domain::action::identity::Version;

/// A resolved specifier entry in the lock file: maps a spec to its resolved version and comment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolution {
    /// The resolved concrete version (e.g., "v4.2.1").
    pub version: Version,
    /// Human-readable version comment for workflow output (e.g., "v4").
    pub comment: String,
}
