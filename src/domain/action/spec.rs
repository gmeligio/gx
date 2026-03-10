use super::identity::{ActionId, Specifier};
use std::fmt;

/// An action dependency specification (desired state)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionSpec {
    pub id: ActionId,
    pub version: Specifier,
}

impl ActionSpec {
    #[must_use]
    pub fn new(id: ActionId, version: Specifier) -> Self {
        Self { id, version }
    }
}

impl fmt::Display for ActionSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}@{}", self.id, self.version)
    }
}

/// Key for the lock file combining action ID and specifier
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LockKey {
    pub id: ActionId,
    pub version: Specifier,
}

impl LockKey {
    #[must_use]
    pub fn new(id: ActionId, version: Specifier) -> Self {
        Self { id, version }
    }

    /// Parse from "action@specifier" format (e.g., "actions/checkout@^6")
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        let (action, specifier) = s.rsplit_once('@')?;
        Some(Self {
            id: ActionId(action.to_string()),
            version: Specifier::parse(specifier),
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
        Self::new(ActionId::from(spec.id.as_str()), spec.version.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::{ActionId, ActionSpec, LockKey, Specifier};

    #[test]
    fn test_lock_key_display() {
        let key = LockKey::new(ActionId::from("actions/checkout"), Specifier::parse("^6"));
        assert_eq!(key.to_string(), "actions/checkout@^6");
    }

    #[test]
    fn test_lock_key_parse_specifier() {
        let key = LockKey::parse("actions/checkout@^6").unwrap();
        assert_eq!(key.id.as_str(), "actions/checkout");
        assert_eq!(key.version.as_str(), "^6");
    }

    #[test]
    fn test_lock_key_parse_tilde() {
        let key = LockKey::parse("actions/checkout@~1.15.2").unwrap();
        assert_eq!(key.id.as_str(), "actions/checkout");
        assert_eq!(key.version.as_str(), "~1.15.2");
    }

    #[test]
    fn test_lock_key_parse_with_subpath() {
        let key = LockKey::parse("github/codeql-action/upload-sarif@^3").unwrap();
        assert_eq!(key.id.as_str(), "github/codeql-action/upload-sarif");
        assert_eq!(key.version.as_str(), "^3");
    }

    #[test]
    fn test_lock_key_parse_invalid() {
        assert!(LockKey::parse("no-at-sign").is_none());
    }

    #[test]
    fn test_action_spec_to_lock_key() {
        let spec = ActionSpec::new(ActionId::from("actions/checkout"), Specifier::parse("^6"));
        let key: LockKey = (&spec).into();
        assert_eq!(key.id.as_str(), "actions/checkout");
        assert_eq!(key.version.as_str(), "^6");
    }
}
