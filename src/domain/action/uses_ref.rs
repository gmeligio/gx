use super::identity::{ActionId, CommitSha, Version};
use crate::domain::workflow_actions::WorkflowAction;
use std::fmt;

/// The type of reference that was resolved.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum RefType {
    /// Tag with a GitHub Release.
    #[serde(rename = "release")]
    Release,
    /// Tag without a GitHub Release (may be annotated or lightweight).
    #[serde(rename = "tag")]
    Tag,
    /// Branch reference.
    #[serde(rename = "branch")]
    Branch,
    /// Direct commit SHA.
    #[serde(rename = "commit")]
    Commit,
}

impl fmt::Display for RefType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Release => write!(f, "release"),
            Self::Tag => write!(f, "tag"),
            Self::Branch => write!(f, "branch"),
            Self::Commit => write!(f, "commit"),
        }
    }
}

impl RefType {
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "release" => Some(Self::Release),
            "tag" => Some(Self::Tag),
            "branch" => Some(Self::Branch),
            "commit" => Some(Self::Commit),
            _ => None,
        }
    }
}

/// Data from a `uses:` line in a workflow file.
/// Contains no interpretation -- just the exact strings parsed from YAML.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsesRef {
    /// The action name (e.g., `"actions/checkout"`).
    pub action_name: String,
    /// The ref portion after `@` (could be tag, SHA, or branch).
    pub uses_ref: String,
    /// The comment after `#`, if present (e.g., `"v4"` or `"v4.0.1"`).
    pub comment: Option<String>,
}

impl UsesRef {
    #[must_use]
    pub const fn new(action_name: String, uses_ref: String, comment: Option<String>) -> Self {
        Self {
            action_name,
            uses_ref,
            comment,
        }
    }

    /// Interpret this reference into domain types.
    ///
    /// Rules applied:
    /// - If comment exists, normalize it (add 'v' prefix if missing) and use as version
    /// - If comment exists and `uses_ref` is a 40-char hex SHA, store the SHA
    /// - If no comment, use `uses_ref` as version (could be tag like "v4" or SHA)
    #[must_use]
    pub fn interpret(&self) -> WorkflowAction {
        let (version, sha) = self.comment.as_ref().map_or_else(
            || {
                // No comment, use the ref as-is, no SHA stored
                (Version::from(self.uses_ref.as_str()), None)
            },
            |comment| {
                // Has a comment - use normalized comment as version
                let version = Version::normalized(comment);
                // If ref is a SHA, store it
                let sha = CommitSha::is_valid(&self.uses_ref)
                    .then(|| CommitSha::from(self.uses_ref.as_str()));
                (version, sha)
            },
        );

        WorkflowAction {
            id: ActionId::from(self.action_name.as_str()),
            version,
            sha,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{CommitSha, RefType, UsesRef};

    #[test]
    fn ref_type_display() {
        assert_eq!(RefType::Release.to_string(), "release");
        assert_eq!(RefType::Tag.to_string(), "tag");
        assert_eq!(RefType::Branch.to_string(), "branch");
        assert_eq!(RefType::Commit.to_string(), "commit");
    }

    #[test]
    fn ref_type_parse() {
        assert_eq!(RefType::parse("release"), Some(RefType::Release));
        assert_eq!(RefType::parse("tag"), Some(RefType::Tag));
        assert_eq!(RefType::parse("branch"), Some(RefType::Branch));
        assert_eq!(RefType::parse("commit"), Some(RefType::Commit));
        assert_eq!(RefType::parse("unknown"), None);
    }

    #[test]
    fn ref_type_equality() {
        assert_eq!(RefType::Release, RefType::Release);
        assert_ne!(RefType::Release, RefType::Tag);
    }

    #[test]
    fn uses_ref_interpret_tag_only() {
        let uses_ref = UsesRef::new("actions/checkout".to_owned(), "v4".to_owned(), None);
        let interpreted = uses_ref.interpret();

        assert_eq!(interpreted.id.as_str(), "actions/checkout");
        assert_eq!(interpreted.version.as_str(), "v4");
        assert!(interpreted.sha.is_none());
    }

    #[test]
    fn uses_ref_interpret_sha_with_comment() {
        let uses_ref = UsesRef::new(
            "actions/checkout".to_owned(),
            "abc123def456789012345678901234567890abcd".to_owned(),
            Some("v4".to_owned()),
        );
        let interpreted = uses_ref.interpret();

        assert_eq!(interpreted.id.as_str(), "actions/checkout");
        assert_eq!(interpreted.version.as_str(), "v4");
        assert_eq!(
            interpreted.sha.as_ref().map(CommitSha::as_str),
            Some("abc123def456789012345678901234567890abcd")
        );
    }

    #[test]
    fn uses_ref_interpret_normalizes_version() {
        let uses_ref = UsesRef::new(
            "actions/checkout".to_owned(),
            "abc123def456789012345678901234567890abcd".to_owned(),
            Some("4".to_owned()), // No 'v' prefix
        );
        let interpreted = uses_ref.interpret();

        assert_eq!(interpreted.version.as_str(), "v4"); // Should be normalized
    }

    #[test]
    fn uses_ref_interpret_sha_without_comment() {
        let uses_ref = UsesRef::new(
            "actions/checkout".to_owned(),
            "abc123def456789012345678901234567890abcd".to_owned(),
            None,
        );
        let interpreted = uses_ref.interpret();

        // Without comment, the SHA becomes the version
        assert_eq!(
            interpreted.version.as_str(),
            "abc123def456789012345678901234567890abcd"
        );
        assert!(interpreted.sha.is_none());
    }

    #[test]
    fn uses_ref_interpret_short_ref_with_comment() {
        // Short ref (not 40 chars) with comment - ref is NOT a SHA
        let uses_ref = UsesRef::new(
            "actions/checkout".to_owned(),
            "abc123".to_owned(),
            Some("v4".to_owned()),
        );
        let interpreted = uses_ref.interpret();

        assert_eq!(interpreted.version.as_str(), "v4");
        assert!(interpreted.sha.is_none()); // Short ref is not stored as SHA
    }
}
