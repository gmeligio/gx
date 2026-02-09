use std::fmt;

use super::version::{is_commit_sha, normalize_version};

/// Unique identifier for an action (e.g., "actions/checkout")
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ActionId(pub String);

impl ActionId {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Extract the base repository (owner/repo) from the action ID.
    /// Handles subpath actions like "github/codeql-action/upload-sarif".
    #[must_use]
    pub fn base_repo(&self) -> String {
        self.0.split('/').take(2).collect::<Vec<_>>().join("/")
    }
}

impl fmt::Display for ActionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for ActionId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for ActionId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

/// A version specifier (e.g., "v4", "v4.1.0")
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Version(pub String);

impl Version {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for Version {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for Version {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

/// A resolved commit SHA (40 hex characters)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CommitSha(pub String);

impl CommitSha {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for CommitSha {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for CommitSha {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for CommitSha {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

/// An action dependency specification (desired state)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionSpec {
    pub id: ActionId,
    pub version: Version,
}

impl ActionSpec {
    #[must_use]
    pub fn new(id: ActionId, version: Version) -> Self {
        Self { id, version }
    }
}

impl fmt::Display for ActionSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}@{}", self.id, self.version)
    }
}

/// A fully resolved action with its commit SHA
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedAction {
    pub id: ActionId,
    pub version: Version,
    pub sha: CommitSha,
}

impl ResolvedAction {
    #[must_use]
    pub fn new(id: ActionId, version: Version, sha: CommitSha) -> Self {
        Self { id, version, sha }
    }

    /// Format as "SHA # version" for workflow updates
    #[must_use]
    pub fn to_workflow_ref(&self) -> String {
        format!("{} # {}", self.sha, self.version)
    }
}

/// Key for the lock file combining action ID and version
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LockKey {
    pub id: ActionId,
    pub version: Version,
}

impl LockKey {
    #[must_use]
    pub fn new(id: ActionId, version: Version) -> Self {
        Self { id, version }
    }

    /// Parse from "action@version" format
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        let (action, version) = s.rsplit_once('@')?;
        Some(Self {
            id: ActionId(action.to_string()),
            version: Version(version.to_string()),
        })
    }
}

impl fmt::Display for LockKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}@{}", self.id, self.version)
    }
}

impl From<&ActionSpec> for LockKey {
    fn from(spec: &ActionSpec) -> Self {
        Self::new(spec.id.clone(), spec.version.clone())
    }
}

impl From<&ResolvedAction> for LockKey {
    fn from(resolved: &ResolvedAction) -> Self {
        Self::new(resolved.id.clone(), resolved.version.clone())
    }
}

/// Data from a `uses:` line in a workflow file.
/// Contains no interpretation - just the exact strings parsed from YAML.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsesRef {
    /// The action name (e.g., "actions/checkout")
    pub action_name: String,
    /// The ref portion after @ (could be tag, SHA, or branch)
    pub uses_ref: String,
    /// The comment after #, if present (e.g., "v4" or "v4.0.1")
    pub comment: Option<String>,
}

impl UsesRef {
    #[must_use]
    pub fn new(action_name: String, uses_ref: String, comment: Option<String>) -> Self {
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
    pub fn interpret(&self) -> InterpretedRef {
        let (version, sha) = if let Some(comment) = &self.comment {
            // Has a comment - use normalized comment as version
            let normalized = normalize_version(comment);
            // If ref is a SHA, store it
            let sha = if is_commit_sha(&self.uses_ref) {
                Some(CommitSha::from(self.uses_ref.clone()))
            } else {
                None
            };
            (Version::from(normalized), sha)
        } else {
            // No comment, use the ref as-is, no SHA stored
            (Version::from(self.uses_ref.clone()), None)
        };

        InterpretedRef {
            id: ActionId::from(self.action_name.clone()),
            version,
            sha,
        }
    }
}

/// Result of interpreting a workflow reference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InterpretedRef {
    pub id: ActionId,
    pub version: Version,
    pub sha: Option<CommitSha>,
}

/// Tracks a version correction when SHA doesn't match the version comment
#[derive(Debug)]
pub struct VersionCorrection {
    pub action: ActionId,
    pub old_version: Version,
    pub new_version: Version,
    pub sha: CommitSha,
}

impl fmt::Display for VersionCorrection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} {} -> {} (SHA {} points to {})",
            self.action, self.old_version, self.new_version, self.sha, self.new_version
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_action_id_base_repo() {
        let simple = ActionId::from("actions/checkout");
        assert_eq!(simple.base_repo(), "actions/checkout");

        let subpath = ActionId::from("github/codeql-action/upload-sarif");
        assert_eq!(subpath.base_repo(), "github/codeql-action");
    }

    #[test]
    fn test_lock_key_display() {
        let key = LockKey::new(ActionId::from("actions/checkout"), Version::from("v4"));
        assert_eq!(key.to_string(), "actions/checkout@v4");
    }

    #[test]
    fn test_lock_key_parse() {
        let key = LockKey::parse("actions/checkout@v4").unwrap();
        assert_eq!(key.id.as_str(), "actions/checkout");
        assert_eq!(key.version.as_str(), "v4");
    }

    #[test]
    fn test_lock_key_parse_with_subpath() {
        let key = LockKey::parse("github/codeql-action/upload-sarif@v3").unwrap();
        assert_eq!(key.id.as_str(), "github/codeql-action/upload-sarif");
        assert_eq!(key.version.as_str(), "v3");
    }

    #[test]
    fn test_lock_key_parse_invalid() {
        assert!(LockKey::parse("no-at-sign").is_none());
    }

    #[test]
    fn test_resolved_action_to_workflow_ref() {
        let resolved = ResolvedAction::new(
            ActionId::from("actions/checkout"),
            Version::from("v4"),
            CommitSha::from("abc123def456789012345678901234567890abcd"),
        );
        assert_eq!(
            resolved.to_workflow_ref(),
            "abc123def456789012345678901234567890abcd # v4"
        );
    }

    #[test]
    fn test_action_spec_to_lock_key() {
        let spec = ActionSpec::new(ActionId::from("actions/checkout"), Version::from("v4"));
        let key: LockKey = (&spec).into();
        assert_eq!(key.id.as_str(), "actions/checkout");
        assert_eq!(key.version.as_str(), "v4");
    }

    #[test]
    fn test_uses_ref_interpret_tag_only() {
        let uses_ref = UsesRef::new("actions/checkout".to_string(), "v4".to_string(), None);
        let interpreted = uses_ref.interpret();

        assert_eq!(interpreted.id.as_str(), "actions/checkout");
        assert_eq!(interpreted.version.as_str(), "v4");
        assert!(interpreted.sha.is_none());
    }

    #[test]
    fn test_uses_ref_interpret_sha_with_comment() {
        let uses_ref = UsesRef::new(
            "actions/checkout".to_string(),
            "abc123def456789012345678901234567890abcd".to_string(),
            Some("v4".to_string()),
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
    fn test_uses_ref_interpret_normalizes_version() {
        let uses_ref = UsesRef::new(
            "actions/checkout".to_string(),
            "abc123def456789012345678901234567890abcd".to_string(),
            Some("4".to_string()), // No 'v' prefix
        );
        let interpreted = uses_ref.interpret();

        assert_eq!(interpreted.version.as_str(), "v4"); // Should be normalized
    }

    #[test]
    fn test_uses_ref_interpret_sha_without_comment() {
        let uses_ref = UsesRef::new(
            "actions/checkout".to_string(),
            "abc123def456789012345678901234567890abcd".to_string(),
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
    fn test_uses_ref_interpret_short_ref_with_comment() {
        // Short ref (not 40 chars) with comment - ref is NOT a SHA
        let uses_ref = UsesRef::new(
            "actions/checkout".to_string(),
            "abc123".to_string(),
            Some("v4".to_string()),
        );
        let interpreted = uses_ref.interpret();

        assert_eq!(interpreted.version.as_str(), "v4");
        assert!(interpreted.sha.is_none()); // Short ref is not stored as SHA
    }
}
