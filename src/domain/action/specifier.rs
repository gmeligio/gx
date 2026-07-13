use super::identity::{CommitSha, Version, VersionPrecision};
use super::resolved::ResolvedRef;
use std::fmt;

/// A specifier for an action version in the manifest or lock key.
///
/// This replaces `Version` in manifest entries, lock keys, and overrides.
/// `Version` remains for concrete resolved tags in lock entries.
#[derive(Debug, Clone)]
pub enum Specifier {
    /// Semver range: `"^6"`, `"~1.15.2"`, `"^0.5"`.
    Range {
        /// For matching against resolved versions.
        req: semver::VersionReq,
        /// Raw specifier string for serialization roundtrip (e.g., `"^6"`).
        raw: String,
    },
    /// Non-semver ref: `"main"`, `"develop"`.
    Ref(String),
    /// Direct 40-char hex SHA.
    Sha(String),
}

impl Specifier {
    /// Parse a specifier string.
    ///
    /// - `"^6"`, `"~1.15.2"` → `Range`
    /// - 40-char hex SHA → `Sha`
    /// - Anything else → `Ref`
    #[must_use]
    pub fn parse(s: &str) -> Self {
        // Semver range: starts with ^ or ~
        if s.strip_prefix('^')
            .or_else(|| s.strip_prefix('~'))
            .is_some()
            && let Ok(req) = semver::VersionReq::parse(s)
        {
            return Self::Range {
                req,
                raw: s.to_owned(),
            };
        }
        // SHA
        if CommitSha::is_valid(s) {
            return Self::Sha(s.to_owned());
        }
        // Ref (branch name, etc.)
        Self::Ref(s.to_owned())
    }

    /// Convert a v1 version string (e.g., `"v6"`, `"v1.15.2"`) to a `Specifier`.
    ///
    /// Conversion rules:
    /// - `"v6"` (Major) → `"^6"` (`Range`)
    /// - `"v4.2"` (Minor) → `"^4.2"` (`Range`)
    /// - `"v1.15.2"` (Patch) → `"~1.15.2"` (`Range`)
    /// - `"main"` → `Ref("main")`
    /// - SHA → `Sha(...)`
    #[must_use]
    pub fn from_v1(v: &str) -> Self {
        if CommitSha::is_valid(v) {
            return Self::Sha(v.to_owned());
        }
        if let Some(spec_str) = Version::from(v).specifier() {
            return Self::parse(&spec_str);
        }
        Self::Ref(v.to_owned())
    }

    /// Check if this specifier matches a semver version.
    #[must_use]
    pub fn matches(&self, version: &semver::Version) -> bool {
        match self {
            Self::Range { req, .. } => req.matches(version),
            Self::Ref(_) | Self::Sha(_) => false,
        }
    }

    /// Whether a resolved reference is allowed under this specifier.
    ///
    /// A range constrains a real semver tag (allowed only if the tag satisfies
    /// the range) and rejects a branch (not a version, so it can never satisfy a
    /// range). A bare commit pin is exempt: it carries no version to check, so
    /// the pin itself is authoritative. A `Ref`/`Sha` specifier imposes no range
    /// at all.
    #[must_use]
    pub fn matches_version(&self, reference: &ResolvedRef) -> bool {
        match self {
            Self::Range { .. } => match reference {
                ResolvedRef::Tag(tag) => {
                    parse_semver(tag.as_str()).is_some_and(|v| self.matches(&v))
                }
                ResolvedRef::Branch(_) => false,
                ResolvedRef::Commit => true,
            },
            Self::Ref(_) | Self::Sha(_) => true,
        }
    }

    /// Get the tag name used for GitHub API lookups (e.g., `"^6"` → `"v6"`).
    #[must_use]
    pub fn to_lookup_tag(&self) -> String {
        match self {
            Self::Range { raw, .. } => {
                let rest = raw
                    .strip_prefix('^')
                    .or_else(|| raw.strip_prefix('~'))
                    .unwrap_or(raw);
                format!("v{rest}")
            }
            Self::Ref(s) | Self::Sha(s) => s.clone(),
        }
    }

    /// Returns true if this specifier is a direct SHA.
    #[must_use]
    pub const fn is_sha(&self) -> bool {
        matches!(self, Self::Sha(_))
    }

    /// Returns the precision of a Range specifier (Major/Minor/Patch).
    /// Returns None for Ref and Sha.
    #[must_use]
    pub fn precision(&self) -> Option<VersionPrecision> {
        match self {
            Self::Range { raw, .. } => {
                // Strip the operator (first char) and count dot-separated components
                let rest = raw.get(1..)?;
                match rest.split('.').count() {
                    1 => Some(VersionPrecision::Major),
                    2 => Some(VersionPrecision::Minor),
                    3 => Some(VersionPrecision::Patch),
                    _ => None,
                }
            }
            Self::Ref(_) | Self::Sha(_) => None,
        }
    }

    /// Returns the range operator character ('^' or '~') for a Range specifier.
    #[must_use]
    pub fn operator(&self) -> Option<char> {
        match self {
            Self::Range { raw, .. } => raw.chars().next(),
            Self::Ref(_) | Self::Sha(_) => None,
        }
    }

    /// Returns the raw string representation.
    #[must_use]
    pub const fn as_str(&self) -> &str {
        match self {
            Self::Range { raw, .. } => raw.as_str(),
            Self::Ref(s) | Self::Sha(s) => s.as_str(),
        }
    }
}

impl PartialEq for Specifier {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Range { raw: a, .. }, Self::Range { raw: b, .. })
            | (Self::Ref(a), Self::Ref(b))
            | (Self::Sha(a), Self::Sha(b)) => a == b,
            (Self::Range { .. }, Self::Ref(_) | Self::Sha(_))
            | (Self::Ref(_), Self::Range { .. } | Self::Sha(_))
            | (Self::Sha(_), Self::Range { .. } | Self::Ref(_)) => false,
        }
    }
}

impl Eq for Specifier {}

impl std::hash::Hash for Specifier {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        match self {
            Self::Range { raw, .. } => {
                0_u8.hash(state);
                raw.hash(state);
            }
            Self::Ref(s) => {
                1_u8.hash(state);
                s.hash(state);
            }
            Self::Sha(s) => {
                2_u8.hash(state);
                s.hash(state);
            }
        }
    }
}

impl fmt::Display for Specifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Range { raw, .. } => write!(f, "{raw}"),
            Self::Ref(s) | Self::Sha(s) => write!(f, "{s}"),
        }
    }
}

impl From<String> for Specifier {
    fn from(s: String) -> Self {
        Self::parse(&s)
    }
}

impl From<&str> for Specifier {
    fn from(s: &str) -> Self {
        Self::parse(s)
    }
}

/// Attempts to parse a version string into a semver `Version`.
/// Handles common formats like `"v4"`, `"v4.1"`, `"v4.1.2"`, `"4.1.2"`.
pub(super) fn parse_semver(version: &str) -> Option<semver::Version> {
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

/// Compares two versions and returns the higher one.
/// If both are valid semver, uses semver comparison.
/// If only one is valid semver, that one wins.
/// If neither is valid semver, returns the first one.
pub(super) fn higher_version<'ver>(a: &'ver Version, b: &'ver Version) -> &'ver Version {
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

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    reason = "tests use unwrap, indexing, and other patterns freely"
)]
mod tests {
    use super::{ResolvedRef, Specifier, Version, parse_semver};

    #[test]
    fn matches_version_caret_in_range() {
        assert!(Specifier::parse("^5").matches_version(&ResolvedRef::Tag(Version::from("v5.4.0"))));
    }

    #[test]
    fn matches_version_caret_out_of_range() {
        assert!(
            !Specifier::parse("^5").matches_version(&ResolvedRef::Tag(Version::from("v6.0.2")))
        );
    }

    #[test]
    fn matches_version_tilde_in_range() {
        assert!(
            Specifier::parse("~1.15.2")
                .matches_version(&ResolvedRef::Tag(Version::from("v1.15.9")))
        );
    }

    #[test]
    fn matches_version_tilde_out_of_range() {
        assert!(
            !Specifier::parse("~1.15.2")
                .matches_version(&ResolvedRef::Tag(Version::from("v1.16.0")))
        );
    }

    #[test]
    fn matches_version_ref_is_exempt() {
        assert!(
            Specifier::Ref("main".to_owned())
                .matches_version(&ResolvedRef::Tag(Version::from("v6.0.2")))
        );
    }

    #[test]
    fn matches_version_sha_specifier_is_exempt() {
        let sha = "a".repeat(40);
        assert!(Specifier::parse(&sha).matches_version(&ResolvedRef::Tag(Version::from("v6.0.2"))));
    }

    #[test]
    fn matches_version_commit_reference_is_exempt() {
        // A bare commit pin exposes no tag, so a range is inapplicable — the
        // reference is always allowed, without any is_sha() discrimination.
        assert!(Specifier::parse("^5").matches_version(&ResolvedRef::Commit));
    }

    #[test]
    fn matches_version_branch_reference_never_satisfies_range() {
        assert!(
            !Specifier::parse("^5").matches_version(&ResolvedRef::Branch(Version::from("main")))
        );
    }

    #[test]
    fn parse_semver_full() {
        let v = parse_semver("1.2.3").unwrap();
        assert_eq!(v.major, 1);
        assert_eq!(v.minor, 2);
        assert_eq!(v.patch, 3);
    }

    #[test]
    fn parse_semver_with_v_prefix() {
        let v = parse_semver("v1.2.3").unwrap();
        assert_eq!(v.major, 1);
        assert_eq!(v.minor, 2);
        assert_eq!(v.patch, 3);
    }

    #[test]
    fn parse_semver_major_only() {
        let v = parse_semver("v4").unwrap();
        assert_eq!(v.major, 4);
        assert_eq!(v.minor, 0);
        assert_eq!(v.patch, 0);
    }

    #[test]
    fn parse_semver_major_minor() {
        let v = parse_semver("v4.1").unwrap();
        assert_eq!(v.major, 4);
        assert_eq!(v.minor, 1);
        assert_eq!(v.patch, 0);
    }

    #[test]
    fn parse_semver_branch_returns_none() {
        assert!(parse_semver("main").is_none());
        assert!(parse_semver("develop").is_none());
    }

    #[test]
    fn parse_semver_sha_returns_none() {
        assert!(parse_semver("a1b2c3d4e5f6").is_none());
    }

    #[test]
    fn highest_version() {
        let versions = vec![
            Version::from("v3"),
            Version::from("v4"),
            Version::from("v2"),
        ];
        assert_eq!(
            Version::highest(&versions).map(|v| v.0),
            Some("v4".to_owned())
        );
    }

    #[test]
    fn highest_version_empty() {
        let versions: Vec<Version> = vec![];
        assert!(Version::highest(&versions).is_none());
    }

    #[test]
    fn highest_version_detailed() {
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
    fn highest_version_one_semver() {
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
    fn highest_version_neither_semver() {
        assert_eq!(
            Version::highest(&[Version::from("main"), Version::from("develop")]),
            Some(Version::from("main"))
        );
    }
}
