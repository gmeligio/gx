use super::specifier::higher_version;
use std::fmt;

/// Unique identifier for an action (e.g., "actions/checkout").
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
    pub fn base_repo(&self) -> Repository {
        Repository::from(self.0.split('/').take(2).collect::<Vec<_>>().join("/"))
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
        Self(s.to_owned())
    }
}

/// A version specifier (e.g., "v4", "v4.1.0").
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Version(pub String);

impl Version {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Create a normalized version with a 'v' prefix.
    /// Only adds a 'v' prefix when the string starts with a digit (semver without prefix).
    /// Non-numeric refs like branch names ("main", "develop") are returned as-is.
    /// Examples: "4" -> "v4", "4.1.0" -> "v4.1.0", "v4" -> "v4", "main" -> "main".
    #[must_use]
    pub fn normalized(s: &str) -> Self {
        if s.starts_with(|c: char| c.is_ascii_digit()) {
            Self(format!("v{s}"))
        } else {
            Self(s.to_owned())
        }
    }

    /// Returns true if this version is a full commit SHA (40 hex characters).
    #[must_use]
    pub fn is_sha(&self) -> bool {
        CommitSha::is_valid(&self.0)
    }

    /// Returns true if this version looks like a semantic version tag (e.g., "v4", "v4.1.0").
    #[must_use]
    pub fn is_semver_like(&self) -> bool {
        let normalized = self
            .0
            .strip_prefix('v')
            .or_else(|| self.0.strip_prefix('V'))
            .unwrap_or(&self.0);
        normalized
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_digit())
    }

    /// Select the highest version from a list.
    /// Prefers the highest semantic version if available.
    #[must_use]
    pub fn highest(versions: &[Version]) -> Option<Version> {
        versions
            .iter()
            .reduce(|a, b| if higher_version(a, b) == a { a } else { b })
            .cloned()
    }

    /// Detect the precision of this version string.
    /// "v4" → Major, "v4.1" → Minor, "v4.1.0" → Patch.
    /// For pre-releases, strips the suffix before counting (e.g., "v3.0.0-beta.2" → Patch).
    /// Returns None for non-semver versions (SHAs, branches).
    #[must_use]
    pub fn precision(&self) -> Option<VersionPrecision> {
        let stripped = self
            .0
            .strip_prefix('v')
            .or_else(|| self.0.strip_prefix('V'))
            .unwrap_or(&self.0);

        // Strip pre-release suffix (everything after the first '-') before counting components
        let base = stripped.split('-').next().unwrap_or(stripped);

        let parts: Vec<&str> = base.split('.').collect();
        match parts.as_slice() {
            [major] if !major.is_empty() && major.chars().all(|c| c.is_ascii_digit()) => {
                Some(VersionPrecision::Major)
            }
            [major, minor]
                if !major.is_empty()
                    && major.chars().all(|c| c.is_ascii_digit())
                    && !minor.is_empty()
                    && minor.chars().all(|c| c.is_ascii_digit()) =>
            {
                Some(VersionPrecision::Minor)
            }
            [major, minor, patch]
                if !major.is_empty()
                    && major.chars().all(|c| c.is_ascii_digit())
                    && !minor.is_empty()
                    && minor.chars().all(|c| c.is_ascii_digit())
                    && !patch.is_empty()
                    && patch.chars().all(|c| c.is_ascii_digit()) =>
            {
                Some(VersionPrecision::Patch)
            }
            _ => None,
        }
    }

    /// Derive a semver range specifier from this version based on its precision.
    /// Major ("v4") → "^4"
    /// Minor ("v4.2") → "^4.2"
    /// Patch ("v4.1.0") → "~4.1.0"
    /// Non-semver (SHAs, branches) → None.
    #[must_use]
    pub fn specifier(&self) -> Option<String> {
        let stripped = self
            .0
            .strip_prefix('v')
            .or_else(|| self.0.strip_prefix('V'))
            .unwrap_or(&self.0);

        match self.precision()? {
            VersionPrecision::Major | VersionPrecision::Minor => Some(format!("^{stripped}")),
            VersionPrecision::Patch => Some(format!("~{stripped}")),
        }
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
        Self(s.to_owned())
    }
}

/// How precisely a version is pinned, following semver component conventions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VersionPrecision {
    /// Only major version specified (e.g., "v4").
    Major,
    /// Major and minor specified (e.g., "v4.1").
    Minor,
    /// Full major.minor.patch specified (e.g., "v4.1.0").
    Patch,
}

/// A resolved commit SHA (40 hex characters).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CommitSha(pub String);

impl CommitSha {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Check if a string is a full commit SHA (40 hexadecimal characters).
    #[must_use]
    pub fn is_valid(s: &str) -> bool {
        s.len() == 40 && s.chars().all(|c| c.is_ascii_hexdigit())
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
        Self(s.to_owned())
    }
}

/// An owner/repo identifier (e.g., "actions/checkout").
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Repository(String);

impl Repository {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Repository {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for Repository {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for Repository {
    fn from(s: &str) -> Self {
        Self(s.to_owned())
    }
}

/// A derived version comment (e.g., "v6" from specifier "^6").
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct VersionComment(String);

impl VersionComment {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for VersionComment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for VersionComment {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for VersionComment {
    fn from(s: &str) -> Self {
        Self(s.to_owned())
    }
}

/// An ISO 8601 date string from commit metadata.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CommitDate(String);

impl CommitDate {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for CommitDate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for CommitDate {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for CommitDate {
    fn from(s: &str) -> Self {
        Self(s.to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::{ActionId, CommitSha, Version, VersionPrecision};

    #[test]
    fn action_id_base_repo() {
        let simple = ActionId::from("actions/checkout");
        assert_eq!(simple.base_repo().as_str(), "actions/checkout");

        let subpath = ActionId::from("github/codeql-action/upload-sarif");
        assert_eq!(subpath.base_repo().as_str(), "github/codeql-action");
    }

    #[test]
    fn commit_sha_is_valid() {
        assert!(CommitSha::is_valid(
            "a1b2c3d4e5f6789012345678901234567890abcd"
        ));
        assert!(CommitSha::is_valid(
            "0000000000000000000000000000000000000000"
        ));
        assert!(CommitSha::is_valid(
            "ffffffffffffffffffffffffffffffffffffffff"
        ));
    }

    #[test]
    fn commit_sha_is_valid_invalid_length() {
        assert!(!CommitSha::is_valid("abc123")); // Too short
        assert!(!CommitSha::is_valid(
            "a1b2c3d4e5f6789012345678901234567890abcde"
        )); // Too long (41 chars)
        assert!(!CommitSha::is_valid("")); // Empty
    }

    #[test]
    fn commit_sha_is_valid_invalid_chars() {
        assert!(!CommitSha::is_valid(
            "g1b2c3d4e5f6789012345678901234567890abcd"
        )); // 'g' is not hex
        assert!(!CommitSha::is_valid(
            "a1b2c3d4e5f6789012345678901234567890abc!"
        )); // '!' is not hex
    }

    #[test]
    fn version_normalized_with_v_prefix() {
        assert_eq!(Version::normalized("v4").as_str(), "v4");
        assert_eq!(Version::normalized("v4.1.0").as_str(), "v4.1.0");
        assert_eq!(Version::normalized("V4").as_str(), "V4");
    }

    #[test]
    fn version_normalized_without_v_prefix() {
        assert_eq!(Version::normalized("4").as_str(), "v4");
        assert_eq!(Version::normalized("4.1.0").as_str(), "v4.1.0");
    }

    #[test]
    fn version_is_sha() {
        assert!(Version::from("abc123def456789012345678901234567890abcd").is_sha());
        assert!(!Version::from("v4").is_sha());
        assert!(!Version::from("main").is_sha());
    }

    #[test]
    fn version_is_semver_like() {
        assert!(Version::from("v4").is_semver_like());
        assert!(Version::from("v4.1").is_semver_like());
        assert!(Version::from("v4.1.0").is_semver_like());
        assert!(Version::from("4.1.0").is_semver_like());
        assert!(Version::from("V4").is_semver_like());
    }

    #[test]
    fn version_is_semver_like_invalid() {
        assert!(!Version::from("main").is_semver_like());
        assert!(!Version::from("develop").is_semver_like());
        assert!(!Version::from("abc123def456789012345678901234567890abcd").is_semver_like());
        assert!(!Version::from("").is_semver_like());
    }

    #[test]
    fn precision_major() {
        assert_eq!(
            Version::from("v4").precision(),
            Some(VersionPrecision::Major)
        );
        assert_eq!(
            Version::from("v12").precision(),
            Some(VersionPrecision::Major)
        );
    }

    #[test]
    fn precision_minor() {
        assert_eq!(
            Version::from("v4.1").precision(),
            Some(VersionPrecision::Minor)
        );
        assert_eq!(
            Version::from("v4.0").precision(),
            Some(VersionPrecision::Minor)
        );
    }

    #[test]
    fn precision_patch() {
        assert_eq!(
            Version::from("v4.1.0").precision(),
            Some(VersionPrecision::Patch)
        );
        assert_eq!(
            Version::from("v4.1.2").precision(),
            Some(VersionPrecision::Patch)
        );
    }

    #[test]
    fn precision_non_semver() {
        assert!(Version::from("main").precision().is_none());
        assert!(
            Version::from("abc123def456789012345678901234567890abcd")
                .precision()
                .is_none()
        );
        assert!(Version::from("").precision().is_none());
    }

    #[test]
    fn precision_prerelease_patch() {
        assert_eq!(
            Version::from("v3.0.0-beta.2").precision(),
            Some(VersionPrecision::Patch)
        );
    }

    #[test]
    fn precision_prerelease_minor() {
        assert_eq!(
            Version::from("v3.0-rc.1").precision(),
            Some(VersionPrecision::Minor)
        );
    }

    #[test]
    fn precision_prerelease_major() {
        assert_eq!(
            Version::from("v3-alpha").precision(),
            Some(VersionPrecision::Major)
        );
    }

    #[test]
    fn specifier_major() {
        assert_eq!(Version::from("v4").specifier(), Some("^4".to_owned()));
        assert_eq!(Version::from("v12").specifier(), Some("^12".to_owned()));
    }

    #[test]
    fn specifier_minor() {
        assert_eq!(Version::from("v4.2").specifier(), Some("^4.2".to_owned()));
        assert_eq!(Version::from("v4.0").specifier(), Some("^4.0".to_owned()));
    }

    #[test]
    fn specifier_patch() {
        assert_eq!(
            Version::from("v4.1.0").specifier(),
            Some("~4.1.0".to_owned())
        );
        assert_eq!(
            Version::from("v4.1.2").specifier(),
            Some("~4.1.2".to_owned())
        );
    }

    #[test]
    fn specifier_non_semver() {
        assert!(Version::from("main").specifier().is_none());
        assert!(
            Version::from("abc123def456789012345678901234567890abcd")
                .specifier()
                .is_none()
        );
    }

    #[test]
    fn specifier_without_v_prefix() {
        // Version without prefix should still work
        let v = Version::from("4.2");
        assert_eq!(v.specifier(), Some("^4.2".to_owned()));
    }

    #[test]
    fn specifier_prerelease_patch() {
        assert_eq!(
            Version::from("v3.0.0-beta.2").specifier(),
            Some("~3.0.0-beta.2".to_owned())
        );
    }

    #[test]
    fn specifier_prerelease_minor() {
        assert_eq!(
            Version::from("v3.0-rc.1").specifier(),
            Some("^3.0-rc.1".to_owned())
        );
    }

    #[test]
    fn specifier_prerelease_major() {
        assert_eq!(
            Version::from("v3-alpha").specifier(),
            Some("^3-alpha".to_owned())
        );
    }

    #[test]
    fn version_specifier_uses_parse_semver() {
        // Ensure that Version::highest and parse_semver integration works correctly
        assert_eq!(
            Version::highest(&[Version::from("v4"), Version::from("main")]),
            Some(Version::from("v4"))
        );
    }
}
