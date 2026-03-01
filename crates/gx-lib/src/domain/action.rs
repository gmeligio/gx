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

/// Extract a precision-preserving version from a candidate tag.
/// Given a candidate version and a target precision, returns the candidate at that precision level.
///
/// Examples:
/// - Candidate `v3.0.0`, Major precision → `v3`
/// - Candidate `v1.0.0`, Minor precision → `v1.0`
/// - Candidate `v1.15.3`, Patch precision → `v1.15.3`
fn extract_at_precision(candidate: &Version, precision: VersionPrecision) -> Version {
    let stripped = candidate
        .0
        .strip_prefix('v')
        .or_else(|| candidate.0.strip_prefix('V'))
        .unwrap_or(&candidate.0);

    // Strip pre-release suffix to get the base version
    let base = stripped.split('-').next().unwrap_or(stripped);
    let parts: Vec<&str> = base.split('.').collect();

    let result = match precision {
        VersionPrecision::Major => {
            if parts.is_empty() {
                candidate.0.clone()
            } else {
                format!("v{}", parts[0])
            }
        }
        VersionPrecision::Minor => {
            if parts.len() >= 2 {
                format!("v{}.{}", parts[0], parts[1])
            } else if !parts.is_empty() {
                format!("v{}", parts[0])
            } else {
                candidate.0.clone()
            }
        }
        VersionPrecision::Patch => {
            if parts.len() >= 3 {
                format!("v{}.{}.{}", parts[0], parts[1], parts[2])
            } else if parts.len() >= 2 {
                format!("v{}.{}", parts[0], parts[1])
            } else if !parts.is_empty() {
                format!("v{}", parts[0])
            } else {
                candidate.0.clone()
            }
        }
    };

    Version(result)
}

/// Find the best upgrade candidate from a list of version tags.
///
/// Returns a richer type indicating whether the candidate is in-range or cross-range.
/// The comparison floor is `max(manifest_semver, lock_version_semver)`.
///
/// # Arguments
///
/// - `manifest_version` — the current version in the manifest (determines range constraint via precision)
/// - `lock_version` — the resolved version from the lock file (if present, used as a floor to avoid same-SHA "upgrades")
/// - `candidates` — all available version tags (these are actual tags, not parsed)
/// - `allow_major` — if false (safe mode), constrain to same major version or major.minor range
///
/// # Returns
///
/// An `UpgradeAction` indicating whether to update just the lock (`InRange`) or both manifest and lock (`CrossRange`).
/// Returns None if no suitable candidate exists.
///
/// # Behavior
///
/// - Major precision (`v4`): safe mode keeps major equal; latest mode unconstrained
/// - Minor precision (`v4.2`): safe mode keeps major equal; latest mode unconstrained
/// - Patch precision (`v4.1.0`): safe mode keeps major and minor equal; latest mode unconstrained
/// - Stable manifest excludes pre-release candidates entirely
/// - Pre-release manifest includes both stable and pre-release candidates, preferring stable
/// - Non-semver candidates and non-semver manifest version return None
#[must_use]
pub fn find_upgrade_candidate(
    manifest_version: &Version,
    lock_version: Option<&Version>,
    candidates: &[Version],
    allow_major: bool,
) -> Option<UpgradeAction> {
    let manifest_precision = manifest_version.precision()?;
    let manifest_semver = parse_semver(manifest_version.as_str())?;

    // Determine if manifest is a pre-release
    let manifest_is_prerelease = !manifest_semver.pre.is_empty();

    // Compute the floor: max of manifest version and lock version
    let floor = if let Some(lock_ver) = lock_version {
        if let Some(lock_semver) = parse_semver(lock_ver.as_str()) {
            manifest_semver.clone().max(lock_semver)
        } else {
            manifest_semver.clone()
        }
    } else {
        manifest_semver.clone()
    };

    // Find the best candidate that is strictly greater than the floor
    // and (if !allow_major) satisfies the precision-based range constraint
    let best_tag = candidates
        .iter()
        .filter_map(|c| {
            let parsed = parse_semver(c.as_str())?;
            // Must be strictly higher than the floor
            if parsed <= floor {
                return None;
            }

            // Pre-release filtering: stable manifest excludes all pre-releases
            if !manifest_is_prerelease && !parsed.pre.is_empty() {
                return None;
            }

            // Apply range constraint if safe mode
            if allow_major {
                // Latest mode: no range constraint
                Some((c.clone(), parsed))
            } else {
                match manifest_precision {
                    VersionPrecision::Major | VersionPrecision::Minor => {
                        // Stay within same major version
                        (parsed.major == manifest_semver.major).then_some((c.clone(), parsed))
                    }
                    VersionPrecision::Patch => {
                        // Stay within same major.minor version
                        (parsed.major == manifest_semver.major
                            && parsed.minor == manifest_semver.minor)
                            .then_some((c.clone(), parsed))
                    }
                }
            }
        })
        .max_by(|(_, a), (_, b)| {
            // Prefer stable over pre-release when manifest is pre-release
            match (a.pre.is_empty(), b.pre.is_empty()) {
                (true, false) => std::cmp::Ordering::Greater, // a is stable, b is not
                (false, true) => std::cmp::Ordering::Less,    // b is stable, a is not
                _ => a.cmp(b),                                // same stability: compare versions
            }
        })
        .map(|(tag, _)| tag)?;

    // Determine if this is in-range or cross-range
    if let Some(best_semver) = parse_semver(best_tag.as_str()) {
        let is_in_range = match manifest_precision {
            VersionPrecision::Major | VersionPrecision::Minor => {
                best_semver.major == manifest_semver.major
            }
            VersionPrecision::Patch => {
                best_semver.major == manifest_semver.major
                    && best_semver.minor == manifest_semver.minor
            }
        };

        if is_in_range {
            Some(UpgradeAction::InRange {
                candidate: best_tag,
            })
        } else {
            let new_manifest_version = extract_at_precision(&best_tag, manifest_precision);
            Some(UpgradeAction::CrossRange {
                candidate: best_tag,
                new_manifest_version,
            })
        }
    } else {
        None
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

/// Indicates what action to take when upgrading a version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpgradeAction {
    /// Candidate is within the manifest's current range.
    /// Only the lock needs re-resolving; manifest stays unchanged.
    InRange { candidate: Version },
    /// Candidate is outside the manifest's range.
    /// Manifest must change. `new_manifest_version` preserves the original precision.
    CrossRange {
        candidate: Version,
        new_manifest_version: Version,
    },
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
    /// The most specific resolved version (via SHA matching against all tags)
    pub resolved_version: Option<Version>,
    /// The semver range specifier derived from the manifest version's precision
    pub specifier: Option<String>,
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
            resolved_version: None,
            specifier: None,
        }
    }

    /// Format as "SHA # version" for workflow updates
    #[must_use]
    pub fn to_workflow_ref(&self) -> String {
        format!("{} # {}", self.sha, self.version)
    }

    /// Create a new `ResolvedAction` with the SHA replaced.
    /// Used when a workflow has a pinned SHA that differs from the registry.
    #[must_use]
    pub fn with_sha(&self, sha: CommitSha) -> Self {
        Self {
            id: self.id.clone(),
            version: self.version.clone(),
            sha,
            repository: self.repository.clone(),
            ref_type: self.ref_type.clone(),
            date: self.date.clone(),
            resolved_version: self.resolved_version.clone(),
            specifier: self.specifier.clone(),
        }
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
    pub action: UpgradeAction,
}

impl UpgradeCandidate {
    /// Get the candidate version that will be resolved
    #[must_use]
    pub fn candidate(&self) -> &Version {
        match &self.action {
            UpgradeAction::InRange { candidate } => candidate,
            UpgradeAction::CrossRange { candidate, .. } => candidate,
        }
    }

    /// Get the version to store in the manifest
    #[must_use]
    pub fn manifest_version(&self) -> &Version {
        match &self.action {
            UpgradeAction::InRange { .. } => &self.current,
            UpgradeAction::CrossRange {
                new_manifest_version,
                ..
            } => new_manifest_version,
        }
    }
}

impl fmt::Display for UpgradeCandidate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {} -> {}", self.id, self.current, self.candidate())
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

    // --- specifier tests ---

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

    // --- find_upgrade_candidate tests ---

    #[test]
    fn test_find_upgrade_candidate_safe_mode_major_precision_in_range() {
        let manifest = Version::from("v4");
        let candidates = vec![
            Version::from("v3"),
            Version::from("v4"),
            Version::from("v4.2.1"),
            Version::from("v5"),
            Version::from("v6"),
        ];
        // Safe mode (allow_major=false), major precision: stays within v4.x
        // Best candidate within major is v4.2.1 (in-range)
        assert_eq!(
            find_upgrade_candidate(&manifest, None, &candidates, false),
            Some(UpgradeAction::InRange {
                candidate: Version::from("v4.2.1")
            })
        );
    }

    #[test]
    fn test_find_upgrade_candidate_safe_mode_minor_precision_in_range() {
        let manifest = Version::from("v4.2");
        let candidates = vec![
            Version::from("v4.1"),
            Version::from("v4.2"),
            Version::from("v4.3.0"),
            Version::from("v5.0"),
        ];
        // Safe mode, minor precision: stays within v4.x, best is v4.3.0 (in-range)
        assert_eq!(
            find_upgrade_candidate(&manifest, None, &candidates, false),
            Some(UpgradeAction::InRange {
                candidate: Version::from("v4.3.0")
            })
        );
    }

    #[test]
    fn test_find_upgrade_candidate_safe_mode_patch_precision_in_range() {
        let manifest = Version::from("v4.1.0");
        let candidates = vec![
            Version::from("v4.0.0"),
            Version::from("v4.1.0"),
            Version::from("v4.1.3"),
            Version::from("v4.2.0"),
            Version::from("v5.0.0"),
        ];
        // Safe mode, patch precision: stays within v4.1.x, best is v4.1.3 (in-range)
        assert_eq!(
            find_upgrade_candidate(&manifest, None, &candidates, false),
            Some(UpgradeAction::InRange {
                candidate: Version::from("v4.1.3")
            })
        );
    }

    #[test]
    fn test_find_upgrade_candidate_latest_mode_crosses_major() {
        let manifest = Version::from("v4");
        let candidates = vec![
            Version::from("v4"),
            Version::from("v4.2.1"),
            Version::from("v5.0.0"),
            Version::from("v6.1.0"),
        ];
        // Latest mode (allow_major=true): no range constraint, returns highest (cross-range)
        assert_eq!(
            find_upgrade_candidate(&manifest, None, &candidates, true),
            Some(UpgradeAction::CrossRange {
                candidate: Version::from("v6.1.0"),
                new_manifest_version: Version::from("v6")
            })
        );
    }

    #[test]
    fn test_find_upgrade_candidate_latest_mode_preserves_precision() {
        let manifest = Version::from("v4.1");
        let candidates = vec![Version::from("v5.0.0")];
        // Latest mode with minor precision: result should preserve minor precision
        assert_eq!(
            find_upgrade_candidate(&manifest, None, &candidates, true),
            Some(UpgradeAction::CrossRange {
                candidate: Version::from("v5.0.0"),
                new_manifest_version: Version::from("v5.0")
            })
        );
    }

    #[test]
    fn test_find_upgrade_candidate_latest_mode_preserves_patch_precision() {
        let manifest = Version::from("v4.1.2");
        let candidates = vec![Version::from("v5.0.0")];
        // Latest mode with patch precision: result should preserve patch precision
        assert_eq!(
            find_upgrade_candidate(&manifest, None, &candidates, true),
            Some(UpgradeAction::CrossRange {
                candidate: Version::from("v5.0.0"),
                new_manifest_version: Version::from("v5.0.0")
            })
        );
    }

    #[test]
    fn test_find_upgrade_candidate_with_lock_floor() {
        let manifest = Version::from("v4");
        let lock_version = Some(Version::from("v4.2.1"));
        let candidates = vec![
            Version::from("v4.2.1"),
            Version::from("v4.3.0"),
            Version::from("v5.0.0"),
        ];
        // Safe mode with lock version as floor: v4.2.1 is excluded (equal to floor), returns v4.3.0 (in-range)
        assert_eq!(
            find_upgrade_candidate(&manifest, lock_version.as_ref(), &candidates, false),
            Some(UpgradeAction::InRange {
                candidate: Version::from("v4.3.0")
            })
        );
    }

    #[test]
    fn test_find_upgrade_candidate_lock_floor_no_upgrade() {
        let manifest = Version::from("v4");
        let lock_version = Some(Version::from("v4.3.0"));
        let candidates = vec![Version::from("v4.2.1"), Version::from("v4.3.0")];
        // Safe mode: no candidate > max(4.0.0, 4.3.0) = 4.3.0 within v4.x
        assert!(
            find_upgrade_candidate(&manifest, lock_version.as_ref(), &candidates, false).is_none()
        );
    }

    #[test]
    fn test_find_upgrade_candidate_stable_manifest_filters_prerelease() {
        let manifest = Version::from("v2");
        let candidates = vec![
            Version::from("v2.2.1"),
            Version::from("v3.0.0"),
            Version::from("v3.0.0-beta.2"),
        ];
        // Stable manifest: pre-releases filtered out, returns v3.0.0 (not beta)
        assert_eq!(
            find_upgrade_candidate(&manifest, None, &candidates, true),
            Some(UpgradeAction::CrossRange {
                candidate: Version::from("v3.0.0"),
                new_manifest_version: Version::from("v3")
            })
        );
    }

    #[test]
    fn test_find_upgrade_candidate_prerelease_manifest_prefers_stable() {
        let manifest = Version::from("v3.0.0-beta.1");
        let candidates = vec![Version::from("v3.0.0"), Version::from("v3.1.0-dev.1")];
        // Pre-release manifest: includes both, prefers stable, returns v3.0.0
        assert_eq!(
            find_upgrade_candidate(&manifest, None, &candidates, true),
            Some(UpgradeAction::InRange {
                candidate: Version::from("v3.0.0")
            })
        );
    }

    #[test]
    fn test_find_upgrade_candidate_prerelease_manifest_falls_back_to_prerelease() {
        let manifest = Version::from("v3.1.0-dev.1");
        let candidates = vec![Version::from("v3.1.0-dev.2"), Version::from("v3.1.0-dev.3")];
        // Pre-release manifest: no stable exists, falls back to newest pre-release
        assert_eq!(
            find_upgrade_candidate(&manifest, None, &candidates, true),
            Some(UpgradeAction::InRange {
                candidate: Version::from("v3.1.0-dev.3")
            })
        );
    }

    #[test]
    fn test_find_upgrade_candidate_non_semver_manifest() {
        let manifest = Version::from("main");
        let candidates = vec![Version::from("v5")];
        // Non-semver manifest version returns None
        assert!(find_upgrade_candidate(&manifest, None, &candidates, true).is_none());
    }

    #[test]
    fn test_find_upgrade_candidate_non_semver_candidates_filtered() {
        let manifest = Version::from("v4");
        let candidates = vec![
            Version::from("main"),
            Version::from("develop"),
            Version::from("v5"),
        ];
        // Non-semver candidates are skipped
        assert_eq!(
            find_upgrade_candidate(&manifest, None, &candidates, true),
            Some(UpgradeAction::CrossRange {
                candidate: Version::from("v5"),
                new_manifest_version: Version::from("v5")
            })
        );
    }

    #[test]
    fn test_find_upgrade_candidate_no_candidates() {
        let manifest = Version::from("v4");
        let candidates: Vec<Version> = vec![];
        assert!(find_upgrade_candidate(&manifest, None, &candidates, true).is_none());
    }

    // --- UpgradeCandidate tests ---

    #[test]
    fn test_upgrade_candidate_display_in_range() {
        let candidate = UpgradeCandidate {
            id: ActionId::from("actions/checkout"),
            current: Version::from("v4"),
            action: UpgradeAction::InRange {
                candidate: Version::from("v4.5.0"),
            },
        };
        assert_eq!(candidate.to_string(), "actions/checkout v4 -> v4.5.0");
    }

    #[test]
    fn test_upgrade_candidate_display_cross_range() {
        let candidate = UpgradeCandidate {
            id: ActionId::from("actions/checkout"),
            current: Version::from("v4"),
            action: UpgradeAction::CrossRange {
                candidate: Version::from("v5.0.0"),
                new_manifest_version: Version::from("v5"),
            },
        };
        assert_eq!(candidate.to_string(), "actions/checkout v4 -> v5.0.0");
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
