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

/// What a parsed `uses:` reference actually is.
///
/// This encodes the *kind* of a reference at parse time, so consumers never
/// re-derive it from a stringly-typed [`Version`] via `is_sha()`. It is the
/// parse-side mirror of [`super::resolved::ResolvedRef`], but honest about what
/// the YAML alone can tell us: a bare ref like `v4` (tag) and `main` (branch)
/// are indistinguishable until network resolution, so both are [`ParsedRef::Ref`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParsedRef {
    /// A bare ref with no comment: a tag (`"v4"`) or branch (`"main"`),
    /// indistinguishable until resolved. `uses: owner/repo@v4`.
    Ref(Version),
    /// A bare 40-hex commit SHA with no comment. `uses: owner/repo@<sha>`.
    Sha(CommitSha),
    /// A SHA pin annotated with a human-readable version.
    /// `uses: owner/repo@<sha> # v4`.
    Pinned { sha: CommitSha, comment: Version },
}

impl ParsedRef {
    /// The pinning SHA, if this reference is pinned to a commit.
    ///
    /// `Some` for [`Sha`](Self::Sha) and [`Pinned`](Self::Pinned); `None` for a
    /// bare [`Ref`](Self::Ref). Answers "is this pinned to a SHA?" by type.
    #[must_use]
    pub const fn pin_sha(&self) -> Option<&CommitSha> {
        match self {
            Self::Sha(sha) | Self::Pinned { sha, .. } => Some(sha),
            Self::Ref(_) => None,
        }
    }

    /// The version-shaped label for this reference.
    ///
    /// This is the key aggregated by [`ActionSet`](crate::domain::workflow_actions::ActionSet)
    /// and the string handed to `Specifier::from_v1`. For [`Ref`](Self::Ref) /
    /// [`Pinned`](Self::Pinned) it is the tag/comment; for a bare
    /// [`Sha`](Self::Sha) it is the SHA string itself, preserving the existing
    /// manifest label and occurrence-count behavior.
    #[must_use]
    pub fn label(&self) -> &str {
        match self {
            Self::Ref(v) | Self::Pinned { comment: v, .. } => v.as_str(),
            Self::Sha(sha) => sha.as_str(),
        }
    }

    /// The [`label`](Self::label) as an owned [`Version`], for callers that key
    /// maps or compare on `Version` (e.g. dominant-version selection).
    #[must_use]
    pub fn label_version(&self) -> Version {
        Version::from(self.label())
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

    /// Interpret this reference into a typed [`WorkflowAction`].
    ///
    /// The kind is established once, here, so no consumer re-derives it:
    /// - comment + 40-hex ref → [`ParsedRef::Pinned`] (normalized comment as version)
    /// - comment + short ref → [`ParsedRef::Ref`] (ref is not a SHA; keep the comment)
    /// - no comment + 40-hex ref → [`ParsedRef::Sha`]
    /// - no comment + other ref → [`ParsedRef::Ref`] (tag or branch, undecidable yet)
    #[must_use]
    pub fn interpret(&self) -> WorkflowAction {
        let is_sha = CommitSha::is_valid(&self.uses_ref);
        let reference = match (self.comment.as_ref(), is_sha) {
            (Some(comment), true) => ParsedRef::Pinned {
                sha: CommitSha::from(self.uses_ref.as_str()),
                comment: Version::normalized(comment),
            },
            (Some(comment), false) => ParsedRef::Ref(Version::normalized(comment)),
            (None, true) => ParsedRef::Sha(CommitSha::from(self.uses_ref.as_str())),
            (None, false) => ParsedRef::Ref(Version::from(self.uses_ref.as_str())),
        };

        WorkflowAction {
            id: ActionId::from(self.action_name.as_str()),
            reference,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{CommitSha, ParsedRef, RefType, UsesRef, Version};

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
        assert_eq!(interpreted.reference, ParsedRef::Ref(Version::from("v4")));
        assert!(interpreted.reference.pin_sha().is_none());
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
        assert_eq!(
            interpreted.reference,
            ParsedRef::Pinned {
                sha: CommitSha::from("abc123def456789012345678901234567890abcd"),
                comment: Version::from("v4"),
            }
        );
        assert_eq!(
            interpreted.reference.pin_sha().map(CommitSha::as_str),
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

        assert_eq!(interpreted.reference.label(), "v4"); // Should be normalized
    }

    #[test]
    fn uses_ref_interpret_sha_without_comment() {
        let uses_ref = UsesRef::new(
            "actions/checkout".to_owned(),
            "abc123def456789012345678901234567890abcd".to_owned(),
            None,
        );
        let interpreted = uses_ref.interpret();

        // Without a comment, a bare 40-hex ref is a SHA pin. Its label is the
        // SHA string, preserving the pre-typed behavior.
        assert_eq!(
            interpreted.reference,
            ParsedRef::Sha(CommitSha::from("abc123def456789012345678901234567890abcd"))
        );
        assert_eq!(
            interpreted.reference.label(),
            "abc123def456789012345678901234567890abcd"
        );
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

        assert_eq!(interpreted.reference, ParsedRef::Ref(Version::from("v4")));
        assert!(interpreted.reference.pin_sha().is_none()); // Short ref is not stored as SHA
    }

    #[test]
    fn parsed_ref_pin_sha_and_label() {
        let tag = ParsedRef::Ref(Version::from("v4"));
        assert!(tag.pin_sha().is_none());
        assert_eq!(tag.label(), "v4");

        let sha = ParsedRef::Sha(CommitSha::from("abc123def456789012345678901234567890abcd"));
        assert_eq!(
            sha.pin_sha().map(CommitSha::as_str),
            Some("abc123def456789012345678901234567890abcd")
        );
        assert_eq!(sha.label(), "abc123def456789012345678901234567890abcd");

        let pinned = ParsedRef::Pinned {
            sha: CommitSha::from("abc123def456789012345678901234567890abcd"),
            comment: Version::from("v4"),
        };
        assert_eq!(
            pinned.pin_sha().map(CommitSha::as_str),
            Some("abc123def456789012345678901234567890abcd")
        );
        assert_eq!(pinned.label(), "v4");
        assert_eq!(pinned.label_version(), Version::from("v4"));
    }
}
