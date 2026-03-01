use super::identity::{ActionId, Version};
use std::fmt;

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
        Self::new(
            ActionId::from(spec.id.as_str()),
            Version::from(spec.version.as_str()),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn test_action_spec_to_lock_key() {
        let spec = ActionSpec::new(ActionId::from("actions/checkout"), Version::from("v4"));
        let key: LockKey = (&spec).into();
        assert_eq!(key.id.as_str(), "actions/checkout");
        assert_eq!(key.version.as_str(), "v4");
    }
}
