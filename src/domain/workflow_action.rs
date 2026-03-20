use super::action::identity::{ActionId, CommitSha, Version};

/// An action as declared in a workflow file.
///
/// Represents the interpreted form of a `uses:` line: the action identity,
/// its version (from the comment or ref), and optionally the pinned SHA.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowAction {
    /// The parsed action identifier.
    pub id: ActionId,
    /// The resolved version.
    pub version: Version,
    /// The commit SHA, if the ref was a full SHA.
    pub sha: Option<CommitSha>,
}
