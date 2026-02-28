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

    /// Determines if this version should be replaced by `other`.
    ///
    /// Returns true when this version is a SHA and `other` is a semantic version tag.
    /// This handles the case where someone upgraded from SHA to semver via comment.
    #[must_use]
    pub fn should_be_replaced_by(&self, other: &Version) -> bool {
        self != other && self.is_sha() && other.is_semver_like()
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
    /// Returns None for non-semver versions (SHAs, branches).
    #[must_use]
    pub fn precision(&self) -> Option<VersionPrecision> {
        let stripped = self
            .0
            .strip_prefix('v')
            .or_else(|| self.0.strip_prefix('V'))
            .unwrap_or(&self.0);

        let parts: Vec<&str> = stripped.split('.').collect();
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

    /// Find the highest compatible upgrade from candidates, respecting precision.
    ///
    /// - Major: v4 can upgrade to v5 (highest major, returned as "vN")
    /// - Minor: v4.1 can upgrade to v4.2 (same major, highest minor, returned as "vN.M")
    /// - Patch: v4.1.0 can upgrade to v4.1.1 (same major.minor, highest patch, returned as "vN.M.P")
    ///
    /// Returns None if already at latest or no compatible upgrade exists.
    #[must_use]
    pub fn find_upgrade(&self, candidates: &[Version]) -> Option<Version> {
        let precision = self.precision()?;
        let current = parse_semver(self.as_str())?;

        let best = candidates
            .iter()
            .filter_map(|c| {
                let parsed = parse_semver(c.as_str())?;
                // Must be strictly higher
                if parsed <= current {
                    return None;
                }
                match precision {
                    VersionPrecision::Major | VersionPrecision::Minor => {
                        (parsed.major == current.major).then_some(parsed)
                    }
                    VersionPrecision::Patch => (parsed.major == current.major
                        && parsed.minor == current.minor)
                        .then_some(parsed),
                }
            })
            .max()?;

        // Format output to match the original precision
        let formatted = match precision {
            VersionPrecision::Major => format!("v{}", best.major),
            VersionPrecision::Minor => format!("v{}.{}", best.major, best.minor),
            VersionPrecision::Patch => format!("v{}.{}.{}", best.major, best.minor, best.patch),
        };

        let result = Version::from(formatted);
        // Only return if it's actually different from current
        (result != *self).then_some(result)
    }

    /// Find the absolute latest upgrade from candidates, including major versions.
    ///
    /// Unlike `find_upgrade()`, this does not constrain by major/minor — it returns
    /// the highest version across all candidates that is strictly higher than self.
    /// Output precision matches self (Major → "vN", Minor → "vN.M", Patch → "vN.M.P").
    ///
    /// Returns None if already at latest or no compatible upgrade exists.
    #[must_use]
    pub fn find_latest_upgrade(&self, candidates: &[Version]) -> Option<Version> {
        let precision = self.precision()?;
        let current = parse_semver(self.as_str())?;

        let best = candidates
            .iter()
            .filter_map(|c| {
                let parsed = parse_semver(c.as_str())?;
                (parsed > current).then_some(parsed)
            })
            .max()?;

        let formatted = match precision {
            VersionPrecision::Major => format!("v{}", best.major),
            VersionPrecision::Minor => format!("v{}.{}", best.major, best.minor),
            VersionPrecision::Patch => format!("v{}.{}.{}", best.major, best.minor, best.patch),
        };

        Some(Version::from(formatted))
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

/// The type of reference that was resolved
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum RefType {
    /// Tag with a GitHub Release
    #[serde(rename = "release")]
    Release,
    /// Tag without a GitHub Release (may be annotated or lightweight)
    #[serde(rename = "tag")]
    Tag,
    /// Branch reference
    #[serde(rename = "branch")]
    Branch,
    /// Direct commit SHA
    #[serde(rename = "commit")]
    Commit,
}

impl fmt::Display for RefType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RefType::Release => write!(f, "release"),
            RefType::Tag => write!(f, "tag"),
            RefType::Branch => write!(f, "branch"),
            RefType::Commit => write!(f, "commit"),
        }
    }
}

impl From<String> for RefType {
    fn from(s: String) -> Self {
        RefType::from(s.as_str())
    }
}

impl From<&str> for RefType {
    #[allow(clippy::match_same_arms)]
    fn from(s: &str) -> Self {
        match s {
            "release" => RefType::Release,
            "tag" => RefType::Tag,
            "branch" => RefType::Branch,
            "commit" => RefType::Commit,
            _ => RefType::Tag, // default to Tag for unknown
        }
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

/// A fully resolved action with its commit SHA and metadata
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedAction {
    pub id: ActionId,
    pub version: Version,
    pub sha: CommitSha,
    pub repository: String,
    pub ref_type: RefType,
    pub date: String,
}

impl ResolvedAction {
    /// Create a new resolved action with all metadata.
    #[must_use]
    pub fn new(
        id: ActionId,
        version: Version,
        sha: CommitSha,
        repository: String,
        ref_type: RefType,
        date: String,
    ) -> Self {
        Self {
            id,
            version,
            sha,
            repository,
            ref_type,
            date,
        }
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
        Self::new(
            ActionId::from(spec.id.as_str()),
            Version::from(spec.version.as_str()),
        )
    }
}

impl From<&ResolvedAction> for LockKey {
    fn from(resolved: &ResolvedAction) -> Self {
        Self::new(
            ActionId::from(resolved.id.as_str()),
            Version::from(resolved.version.as_str()),
        )
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
            let version = Version::normalized(comment);
            // If ref is a SHA, store it
            let sha = if CommitSha::is_valid(&self.uses_ref) {
                Some(CommitSha::from(self.uses_ref.as_str()))
            } else {
                None
            };
            (version, sha)
        } else {
            // No comment, use the ref as-is, no SHA stored
            (Version::from(self.uses_ref.as_str()), None)
        };

        InterpretedRef {
            id: ActionId::from(self.action_name.as_str()),
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

/// An available upgrade for an action
#[derive(Debug)]
pub struct UpgradeCandidate {
    pub id: ActionId,
    pub current: Version,
    pub upgraded: Version,
}

impl fmt::Display for UpgradeCandidate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {} -> {}", self.id, self.current, self.upgraded)
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
    fn test_ref_type_display() {
        assert_eq!(RefType::Release.to_string(), "release");
        assert_eq!(RefType::Tag.to_string(), "tag");
        assert_eq!(RefType::Branch.to_string(), "branch");
        assert_eq!(RefType::Commit.to_string(), "commit");
    }

    #[test]
    fn test_ref_type_from_string() {
        assert_eq!(RefType::from("release"), RefType::Release);
        assert_eq!(RefType::from("tag"), RefType::Tag);
        assert_eq!(RefType::from("branch"), RefType::Branch);
        assert_eq!(RefType::from("commit"), RefType::Commit);
        assert_eq!(RefType::from("unknown"), RefType::Tag); // defaults to Tag
    }

    #[test]
    fn test_ref_type_equality() {
        assert_eq!(RefType::Release, RefType::Release);
        assert_ne!(RefType::Release, RefType::Tag);
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
            "actions/checkout".to_string(),
            RefType::Tag,
            "2026-01-01T00:00:00Z".to_string(),
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

    // --- CommitSha tests ---

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

    // --- Version tests ---

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
    fn test_should_be_replaced_by_sha_to_semver() {
        let sha = Version::from("abc123def456789012345678901234567890abcd");
        let semver = Version::from("v4");
        assert!(sha.should_be_replaced_by(&semver));
    }

    #[test]
    fn test_should_be_replaced_by_same_version() {
        let v = Version::from("v4");
        assert!(!v.should_be_replaced_by(&Version::from("v4")));
    }

    #[test]
    fn test_should_be_replaced_by_semver_to_semver() {
        let v3 = Version::from("v3");
        let v4 = Version::from("v4");
        assert!(!v3.should_be_replaced_by(&v4));
    }

    #[test]
    fn test_should_be_replaced_by_sha_to_sha() {
        let sha1 = Version::from("abc123def456789012345678901234567890abcd");
        let sha2 = Version::from("def456789012345678901234567890abcd12340000");
        assert!(!sha1.should_be_replaced_by(&sha2));
    }

    #[test]
    fn test_should_be_replaced_by_sha_to_branch() {
        let sha = Version::from("abc123def456789012345678901234567890abcd");
        let branch = Version::from("main");
        assert!(!sha.should_be_replaced_by(&branch));
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

    // --- VersionPrecision tests ---

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

    // --- find_upgrade tests ---

    #[test]
    fn test_find_upgrade_major() {
        let current = Version::from("v4");
        let candidates = vec![
            Version::from("v3"),
            Version::from("v4"),
            Version::from("v4.1.0"),
            Version::from("v5"),
            Version::from("v5.0.0"),
            Version::from("v6"),
        ];
        // v4 should NOT cross to v5 or v6 — stays within major
        assert!(current.find_upgrade(&candidates).is_none());
    }

    #[test]
    fn test_find_upgrade_major_within_same_major() {
        let current = Version::from("v4");
        let candidates = vec![
            Version::from("v3"),
            Version::from("v4"),
            Version::from("v4.1.0"),
            Version::from("v4.2.0"),
            Version::from("v5"),
        ];
        // v4 should find the highest v4.x.x tag — but output is major precision
        // parse_semver("v4") = 4.0.0, parse_semver("v4.2.0") = 4.2.0 which is > 4.0.0
        // but major must equal current.major (4), and 4.2.0 has major=4 ✓
        // Formatted at Major precision: "v4" — same as current, so no upgrade
        // Actually: the best candidate with major==4 and > 4.0.0 is 4.2.0
        // Formatted as Major: "v4" — equals current, so this returns None
        // This is correct: v4 means "latest v4", there's no higher v4 major tag
        assert!(current.find_upgrade(&candidates).is_none());
    }

    #[test]
    fn test_find_upgrade_minor() {
        let current = Version::from("v4.1");
        let candidates = vec![
            Version::from("v4.0"),
            Version::from("v4.1"),
            Version::from("v4.1.0"),
            Version::from("v4.2"),
            Version::from("v4.3.0"),
            Version::from("v5.0"),
        ];
        // v5.0 is excluded (different major), v4.3.0 parses as 4.3.0 which is same major
        assert_eq!(
            current.find_upgrade(&candidates),
            Some(Version::from("v4.3"))
        );
    }

    #[test]
    fn test_find_upgrade_patch() {
        let current = Version::from("v4.1.0");
        let candidates = vec![
            Version::from("v4.1.0"),
            Version::from("v4.1.1"),
            Version::from("v4.1.3"),
            Version::from("v4.2.0"),
            Version::from("v5.0.0"),
        ];
        assert_eq!(
            current.find_upgrade(&candidates),
            Some(Version::from("v4.1.3"))
        );
    }

    #[test]
    fn test_find_upgrade_already_latest() {
        let current = Version::from("v4");
        let candidates = vec![Version::from("v3"), Version::from("v4")];
        assert!(current.find_upgrade(&candidates).is_none());
    }

    #[test]
    fn test_find_upgrade_no_candidates() {
        let current = Version::from("v4");
        let candidates: Vec<Version> = vec![];
        assert!(current.find_upgrade(&candidates).is_none());
    }

    #[test]
    fn test_find_upgrade_non_semver_current() {
        let current = Version::from("main");
        let candidates = vec![Version::from("v5")];
        assert!(current.find_upgrade(&candidates).is_none());
    }

    #[test]
    fn test_find_upgrade_minor_stays_within_major() {
        let current = Version::from("v4.1");
        let candidates = vec![Version::from("v5.0"), Version::from("v5.1")];
        // No upgrade within major v4
        assert!(current.find_upgrade(&candidates).is_none());
    }

    #[test]
    fn test_find_upgrade_patch_stays_within_minor() {
        let current = Version::from("v4.1.0");
        let candidates = vec![Version::from("v4.2.0"), Version::from("v5.0.0")];
        // No upgrade within v4.1.x
        assert!(current.find_upgrade(&candidates).is_none());
    }

    // --- find_latest_upgrade tests ---

    #[test]
    fn test_find_latest_upgrade_crosses_major() {
        let current = Version::from("v4");
        let candidates = vec![
            Version::from("v3"),
            Version::from("v4"),
            Version::from("v5"),
            Version::from("v6"),
        ];
        assert_eq!(
            current.find_latest_upgrade(&candidates),
            Some(Version::from("v6"))
        );
    }

    #[test]
    fn test_find_latest_upgrade_minor_crosses_major() {
        let current = Version::from("v4.1");
        let candidates = vec![
            Version::from("v4.2"),
            Version::from("v5.0"),
            Version::from("v5.1"),
        ];
        assert_eq!(
            current.find_latest_upgrade(&candidates),
            Some(Version::from("v5.1"))
        );
    }

    #[test]
    fn test_find_latest_upgrade_patch_crosses_major() {
        let current = Version::from("v4.1.0");
        let candidates = vec![
            Version::from("v4.1.1"),
            Version::from("v5.0.0"),
            Version::from("v5.2.3"),
        ];
        assert_eq!(
            current.find_latest_upgrade(&candidates),
            Some(Version::from("v5.2.3"))
        );
    }

    #[test]
    fn test_find_latest_upgrade_already_latest() {
        let current = Version::from("v6");
        let candidates = vec![Version::from("v5"), Version::from("v6")];
        assert!(current.find_latest_upgrade(&candidates).is_none());
    }

    #[test]
    fn test_find_latest_upgrade_non_semver_returns_none() {
        let current = Version::from("main");
        let candidates = vec![Version::from("v5")];
        assert!(current.find_latest_upgrade(&candidates).is_none());
    }

    // --- UpgradeCandidate tests ---

    #[test]
    fn test_upgrade_candidate_display() {
        let candidate = UpgradeCandidate {
            id: ActionId::from("actions/checkout"),
            current: Version::from("v4"),
            upgraded: Version::from("v5"),
        };
        assert_eq!(candidate.to_string(), "actions/checkout v4 -> v5");
    }

    // --- UsesRef tests ---

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
