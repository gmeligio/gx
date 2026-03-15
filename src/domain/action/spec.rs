use super::identity::ActionId;
use super::specifier::Specifier;
use std::fmt;

/// An action dependency specification (desired state).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Spec {
    pub id: ActionId,
    pub version: Specifier,
}

impl Spec {
    #[must_use]
    pub fn new(id: ActionId, version: Specifier) -> Self {
        Self { id, version }
    }

    /// Parse from "action@specifier" format (e.g., "actions/checkout@^6").
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        let (action, specifier) = s.rsplit_once('@')?;
        Some(Self {
            id: ActionId(action.to_owned()),
            version: Specifier::parse(specifier),
        })
    }
}

impl fmt::Display for Spec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}@{}", self.id, self.version)
    }
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    reason = "tests use unwrap, indexing, and other patterns freely"
)]
mod tests {
    use super::{ActionId, Spec, Specifier};

    #[test]
    fn spec_display() {
        let spec = Spec::new(ActionId::from("actions/checkout"), Specifier::parse("^6"));
        assert_eq!(spec.to_string(), "actions/checkout@^6");
    }

    #[test]
    fn spec_parse_specifier() {
        let spec = Spec::parse("actions/checkout@^6").unwrap();
        assert_eq!(spec.id.as_str(), "actions/checkout");
        assert_eq!(spec.version.as_str(), "^6");
    }

    #[test]
    fn spec_parse_tilde() {
        let spec = Spec::parse("actions/checkout@~1.15.2").unwrap();
        assert_eq!(spec.id.as_str(), "actions/checkout");
        assert_eq!(spec.version.as_str(), "~1.15.2");
    }

    #[test]
    fn spec_parse_with_subpath() {
        let spec = Spec::parse("github/codeql-action/upload-sarif@^3").unwrap();
        assert_eq!(spec.id.as_str(), "github/codeql-action/upload-sarif");
        assert_eq!(spec.version.as_str(), "^3");
    }

    #[test]
    fn spec_parse_invalid() {
        assert!(Spec::parse("no-at-sign").is_none());
    }

    #[test]
    fn spec_hash_eq() {
        use std::collections::HashSet;
        let a = Spec::new(ActionId::from("actions/checkout"), Specifier::parse("^6"));
        let b = Spec::new(ActionId::from("actions/checkout"), Specifier::parse("^6"));
        assert_eq!(a, b);
        let mut set = HashSet::new();
        set.insert(a);
        assert!(set.contains(&b));
    }
}
