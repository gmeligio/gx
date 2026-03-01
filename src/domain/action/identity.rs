use std::fmt;

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

    /// Create a normalized version with a 'v' prefix.
    /// Only adds a 'v' prefix when the string starts with a digit (semver without prefix).
    /// Non-numeric refs like branch names ("main", "develop") are returned as-is.
    /// Examples: "4" -> "v4", "4.1.0" -> "v4.1.0", "v4" -> "v4", "main" -> "main"
    #[must_use]
    pub fn normalized(s: &str) -> Self {
        if s.starts_with(|c: char| c.is_ascii_digit()) {
            Self(format!("v{s}"))
        } else {
            Self(s.to_string())
        }
    }

    /// Returns true if this version is a full commit SHA (40 hex characters)
    #[must_use]
    pub fn is_sha(&self) -> bool {
        CommitSha::is_valid(&self.0)
    }

    /// Returns true if this version looks like a semantic version tag (e.g., "v4", "v4.1.0")
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
        if parts.is_empty() || !parts[0].chars().all(|c| c.is_ascii_digit()) || parts[0].is_empty()
        {
            return None;
        }

        match parts.len() {
            1 => Some(VersionPrecision::Major),
            2 if parts[1].chars().all(|c| c.is_ascii_digit()) && !parts[1].is_empty() => {
                Some(VersionPrecision::Minor)
            }
            3 if parts[1].chars().all(|c| c.is_ascii_digit())
                && parts[2].chars().all(|c| c.is_ascii_digit())
                && !parts[1].is_empty()
                && !parts[2].is_empty() =>
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
    /// Non-semver (SHAs, branches) → None
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
        Self(s.to_string())
    }
}

/// How precisely a version is pinned, following semver component conventions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VersionPrecision {
    /// Only major version specified (e.g., "v4")
    Major,
    /// Major and minor specified (e.g., "v4.1")
    Minor,
    /// Full major.minor.patch specified (e.g., "v4.1.0")
    Patch,
}

/// A resolved commit SHA (40 hex characters)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CommitSha(pub String);

impl CommitSha {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Check if a string is a full commit SHA (40 hexadecimal characters)
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
        Self(s.to_string())
    }
}

/// Compares two versions and returns the higher one.
/// If both are valid semver, uses semver comparison.
/// If only one is valid semver, that one wins.
/// If neither is valid semver, returns the first one.
fn higher_version<'a>(a: &'a Version, b: &'a Version) -> &'a Version {
    let parsed_a = parse_semver(a.as_str());
    let parsed_b = parse_semver(b.as_str());

    match (parsed_a, parsed_b) {
        (Some(va), Some(vb)) => {
            if va >= vb {
                a
            } else {
                b
            }
        }
        (_, None) => a,
        (None, Some(_)) => b,
    }
}

/// Attempts to parse a version string into a semver Version.
/// Handles common formats like "v4", "v4.1", "v4.1.2", "4.1.2"
fn parse_semver(version: &str) -> Option<semver::Version> {
    // Strip leading 'v' or 'V' if present
    let normalized = version
        .strip_prefix('v')
        .or_else(|| version.strip_prefix('V'))
        .unwrap_or(version);

    // Try parsing as-is
    if let Ok(v) = semver::Version::parse(normalized) {
        return Some(v);
    }

    // Try adding .0 for versions like "4.1"
    let with_patch = format!("{normalized}.0");
    if let Ok(v) = semver::Version::parse(&with_patch) {
        return Some(v);
    }

    // Try adding .0.0 for versions like "4"
    let with_minor_patch = format!("{normalized}.0.0");
    if let Ok(v) = semver::Version::parse(&with_minor_patch) {
        return Some(v);
    }

    None
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
    fn test_commit_sha_is_valid() {
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
    fn test_commit_sha_is_valid_invalid_length() {
        assert!(!CommitSha::is_valid("abc123")); // Too short
        assert!(!CommitSha::is_valid(
            "a1b2c3d4e5f6789012345678901234567890abcde"
        )); // Too long (41 chars)
        assert!(!CommitSha::is_valid("")); // Empty
    }

    #[test]
    fn test_commit_sha_is_valid_invalid_chars() {
        assert!(!CommitSha::is_valid(
            "g1b2c3d4e5f6789012345678901234567890abcd"
        )); // 'g' is not hex
        assert!(!CommitSha::is_valid(
            "a1b2c3d4e5f6789012345678901234567890abc!"
        )); // '!' is not hex
    }

    #[test]
    fn test_version_normalized_with_v_prefix() {
        assert_eq!(Version::normalized("v4").as_str(), "v4");
        assert_eq!(Version::normalized("v4.1.0").as_str(), "v4.1.0");
        assert_eq!(Version::normalized("V4").as_str(), "V4");
    }

    #[test]
    fn test_version_normalized_without_v_prefix() {
        assert_eq!(Version::normalized("4").as_str(), "v4");
        assert_eq!(Version::normalized("4.1.0").as_str(), "v4.1.0");
    }

    #[test]
    fn test_version_is_sha() {
        assert!(Version::from("abc123def456789012345678901234567890abcd").is_sha());
        assert!(!Version::from("v4").is_sha());
        assert!(!Version::from("main").is_sha());
    }

    #[test]
    fn test_version_is_semver_like() {
        assert!(Version::from("v4").is_semver_like());
        assert!(Version::from("v4.1").is_semver_like());
        assert!(Version::from("v4.1.0").is_semver_like());
        assert!(Version::from("4.1.0").is_semver_like());
        assert!(Version::from("V4").is_semver_like());
    }

    #[test]
    fn test_version_is_semver_like_invalid() {
        assert!(!Version::from("main").is_semver_like());
        assert!(!Version::from("develop").is_semver_like());
        assert!(!Version::from("abc123def456789012345678901234567890abcd").is_semver_like());
        assert!(!Version::from("").is_semver_like());
    }

    #[test]
    fn test_highest_version() {
        let versions = vec![
            Version::from("v3"),
            Version::from("v4"),
            Version::from("v2"),
        ];
        assert_eq!(
            Version::highest(&versions).map(|v| v.0),
            Some("v4".to_string())
        );
    }

    #[test]
    fn test_highest_version_empty() {
        let versions: Vec<Version> = vec![];
        assert!(Version::highest(&versions).is_none());
    }

    #[test]
    fn test_highest_version_detailed() {
        assert_eq!(
            Version::highest(&[Version::from("v4.1"), Version::from("v4.0")]),
            Some(Version::from("v4.1"))
        );
        assert_eq!(
            Version::highest(&[Version::from("v4.0.1"), Version::from("v4.0.0")]),
            Some(Version::from("v4.0.1"))
        );
    }

    #[test]
    fn test_highest_version_one_semver() {
        assert_eq!(
            Version::highest(&[Version::from("v4"), Version::from("main")]),
            Some(Version::from("v4"))
        );
        assert_eq!(
            Version::highest(&[Version::from("main"), Version::from("v4")]),
            Some(Version::from("v4"))
        );
    }

    #[test]
    fn test_highest_version_neither_semver() {
        assert_eq!(
            Version::highest(&[Version::from("main"), Version::from("develop")]),
            Some(Version::from("main"))
        );
    }

    #[test]
    fn test_parse_semver_full() {
        let v = parse_semver("1.2.3").unwrap();
        assert_eq!(v.major, 1);
        assert_eq!(v.minor, 2);
        assert_eq!(v.patch, 3);
    }

    #[test]
    fn test_parse_semver_with_v_prefix() {
        let v = parse_semver("v1.2.3").unwrap();
        assert_eq!(v.major, 1);
        assert_eq!(v.minor, 2);
        assert_eq!(v.patch, 3);
    }

    #[test]
    fn test_parse_semver_major_only() {
        let v = parse_semver("v4").unwrap();
        assert_eq!(v.major, 4);
        assert_eq!(v.minor, 0);
        assert_eq!(v.patch, 0);
    }

    #[test]
    fn test_parse_semver_major_minor() {
        let v = parse_semver("v4.1").unwrap();
        assert_eq!(v.major, 4);
        assert_eq!(v.minor, 1);
        assert_eq!(v.patch, 0);
    }

    #[test]
    fn test_parse_semver_branch_returns_none() {
        assert!(parse_semver("main").is_none());
        assert!(parse_semver("develop").is_none());
    }

    #[test]
    fn test_parse_semver_sha_returns_none() {
        assert!(parse_semver("a1b2c3d4e5f6").is_none());
    }

    #[test]
    fn test_precision_major() {
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
    fn test_precision_minor() {
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
    fn test_precision_patch() {
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
    fn test_precision_non_semver() {
        assert!(Version::from("main").precision().is_none());
        assert!(
            Version::from("abc123def456789012345678901234567890abcd")
                .precision()
                .is_none()
        );
        assert!(Version::from("").precision().is_none());
    }

    #[test]
    fn test_precision_prerelease_patch() {
        assert_eq!(
            Version::from("v3.0.0-beta.2").precision(),
            Some(VersionPrecision::Patch)
        );
    }

    #[test]
    fn test_precision_prerelease_minor() {
        assert_eq!(
            Version::from("v3.0-rc.1").precision(),
            Some(VersionPrecision::Minor)
        );
    }

    #[test]
    fn test_precision_prerelease_major() {
        assert_eq!(
            Version::from("v3-alpha").precision(),
            Some(VersionPrecision::Major)
        );
    }

    #[test]
    fn test_specifier_major() {
        assert_eq!(Version::from("v4").specifier(), Some("^4".to_string()));
        assert_eq!(Version::from("v12").specifier(), Some("^12".to_string()));
    }

    #[test]
    fn test_specifier_minor() {
        assert_eq!(Version::from("v4.2").specifier(), Some("^4.2".to_string()));
        assert_eq!(Version::from("v4.0").specifier(), Some("^4.0".to_string()));
    }

    #[test]
    fn test_specifier_patch() {
        assert_eq!(
            Version::from("v4.1.0").specifier(),
            Some("~4.1.0".to_string())
        );
        assert_eq!(
            Version::from("v4.1.2").specifier(),
            Some("~4.1.2".to_string())
        );
    }

    #[test]
    fn test_specifier_non_semver() {
        assert!(Version::from("main").specifier().is_none());
        assert!(
            Version::from("abc123def456789012345678901234567890abcd")
                .specifier()
                .is_none()
        );
    }

    #[test]
    fn test_specifier_without_v_prefix() {
        // Version without prefix should still work
        let v = Version::from("4.2");
        assert_eq!(v.specifier(), Some("^4.2".to_string()));
    }

    #[test]
    fn test_specifier_prerelease_patch() {
        assert_eq!(
            Version::from("v3.0.0-beta.2").specifier(),
            Some("~3.0.0-beta.2".to_string())
        );
    }

    #[test]
    fn test_specifier_prerelease_minor() {
        assert_eq!(
            Version::from("v3.0-rc.1").specifier(),
            Some("^3.0-rc.1".to_string())
        );
    }

    #[test]
    fn test_specifier_prerelease_major() {
        assert_eq!(
            Version::from("v3-alpha").specifier(),
            Some("^3-alpha".to_string())
        );
    }
}
