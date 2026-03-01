use std::collections::{HashMap, HashSet};

use super::{ActionId, CommitSha, LockKey, RefType, ResolvedAction, Version};

/// Metadata about a resolved action entry in the lock file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LockEntry {
    /// The resolved commit SHA
    pub sha: CommitSha,
    /// The most specific version tag pointing to this SHA
    pub version: Option<String>,
    /// The semver range specifier derived from the manifest version
    pub specifier: Option<String>,
    /// The GitHub repository that was queried
    pub repository: String,
    /// The type of reference that was resolved
    pub ref_type: RefType,
    /// RFC 3339 timestamp of the commit (meaning depends on `ref_type`)
    pub date: String,
}

impl LockEntry {
    /// Create a new lock entry.
    #[must_use]
    pub fn new(sha: CommitSha, repository: String, ref_type: RefType, date: String) -> Self {
        Self {
            sha,
            version: None,
            specifier: None,
            repository,
            ref_type,
            date,
        }
    }

    /// Create a lock entry with version and specifier.
    #[must_use]
    pub fn with_version_and_specifier(
        sha: CommitSha,
        version: Option<String>,
        specifier: Option<String>,
        repository: String,
        ref_type: RefType,
        date: String,
    ) -> Self {
        Self {
            sha,
            version,
            specifier,
            repository,
            ref_type,
            date,
        }
    }

    /// Check if this lock entry is complete for the given manifest version.
    ///
    /// An entry is complete when:
    /// - version is present and non-empty
    /// - specifier matches what would be derived from the manifest version
    ///   (or is None if the manifest version is non-semver)
    /// - repository is non-empty
    /// - date is non-empty
    #[must_use]
    pub fn is_complete(&self, manifest_version: &Version) -> bool {
        // Check version is present and non-empty
        let version_ok = self.version.as_ref().is_some_and(|v| !v.is_empty());

        // Check repository is non-empty
        let repository_ok = !self.repository.is_empty();

        // Check date is non-empty
        let date_ok = !self.date.is_empty();

        // Check specifier matches the manifest version's derivation
        let expected_specifier = manifest_version.specifier();
        let specifier_ok = match &self.specifier {
            Some(s) if s.is_empty() => false, // Empty string is treated as missing
            actual => actual == &expected_specifier, // Must match expected
        };

        version_ok && repository_ok && date_ok && specifier_ok
    }

    /// Set the version field on this entry.
    pub fn set_version(&mut self, version: Option<String>) {
        self.version = version;
    }

    /// Set the specifier field on this entry.
    pub fn set_specifier(&mut self, specifier: Option<String>) {
        self.specifier = specifier;
    }
}

/// Domain entity representing the resolved lock state: maps action@version → lock entry.
/// Contains all domain logic for querying and mutating the lock. No I/O.
#[derive(Debug, Default)]
pub struct Lock {
    actions: HashMap<LockKey, LockEntry>,
}

impl Lock {
    /// Create a `Lock` from an existing map of keys to entries.
    #[must_use]
    pub fn new(actions: HashMap<LockKey, LockEntry>) -> Self {
        Self { actions }
    }

    /// Get the locked entry for a lock key.
    #[must_use]
    pub fn get(&self, key: &LockKey) -> Option<&LockEntry> {
        self.actions.get(key)
    }

    /// Set or update a locked action with its resolved metadata.
    pub fn set(&mut self, resolved: &ResolvedAction) {
        let key = LockKey::from(resolved);
        let entry = LockEntry::new(
            resolved.sha.clone(),
            resolved.repository.clone(),
            resolved.ref_type.clone(),
            resolved.date.clone(),
        );
        self.actions.insert(key, entry);
    }

    /// Set the version field for a lock entry.
    pub fn set_version(&mut self, key: &LockKey, version: Option<String>) {
        if let Some(entry) = self.actions.get_mut(key) {
            entry.set_version(version);
        }
    }

    /// Set the specifier field for a lock entry.
    pub fn set_specifier(&mut self, key: &LockKey, specifier: Option<String>) {
        if let Some(entry) = self.actions.get_mut(key) {
            entry.set_specifier(specifier);
        }
    }

    /// Check if the lock has an entry for the given key.
    #[must_use]
    pub fn has(&self, key: &LockKey) -> bool {
        self.actions.contains_key(key)
    }

    /// Retain only entries for the given keys, removing all others.
    pub fn retain(&mut self, keys: &[LockKey]) {
        let keep: HashSet<&LockKey> = keys.iter().collect();
        self.actions.retain(|k, _| keep.contains(k));
    }

    /// Build a map of action IDs to "SHA # version" strings for workflow updates.
    /// Falls back to the version string if no SHA is found.
    #[must_use]
    pub fn build_update_map(&self, keys: &[LockKey]) -> HashMap<ActionId, String> {
        keys.iter()
            .map(|key| {
                let value = if let Some(entry) = self.get(key) {
                    format!("{} # {}", entry.sha, key.version)
                } else {
                    key.version.to_string()
                };
                (key.id.clone(), value)
            })
            .collect()
    }

    /// Iterate over all (key, entry) entries.
    pub fn entries(&self) -> impl Iterator<Item = (&LockKey, &LockEntry)> {
        self.actions.iter()
    }

    /// Check if the lock is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.actions.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{ActionId, CommitSha, LockKey, RefType, ResolvedAction, Version};

    fn make_key(action: &str, version: &str) -> LockKey {
        LockKey::new(ActionId::from(action), Version::from(version))
    }

    fn make_resolved(action: &str, version: &str, sha: &str) -> ResolvedAction {
        ResolvedAction::new(
            ActionId::from(action),
            Version::from(version),
            CommitSha::from(sha),
            ActionId::from(action).base_repo(),
            RefType::Tag,
            "2026-01-01T00:00:00Z".to_string(),
        )
    }

    #[test]
    fn test_new_empty() {
        let lock = Lock::default();
        assert!(lock.get(&make_key("actions/checkout", "v4")).is_none());
    }

    #[test]
    fn test_set_and_get() {
        let mut lock = Lock::default();
        lock.set(&make_resolved(
            "actions/checkout",
            "v4",
            "abc123def456789012345678901234567890abcd",
        ));
        let entry = lock.get(&make_key("actions/checkout", "v4"));
        assert!(entry.is_some());
        assert_eq!(
            entry.unwrap().sha,
            CommitSha::from("abc123def456789012345678901234567890abcd")
        );
        assert_eq!(lock.get(&make_key("actions/checkout", "v3")), None);
    }

    #[test]
    fn test_has() {
        let mut lock = Lock::default();
        lock.set(&make_resolved(
            "actions/checkout",
            "v4",
            "abc123def456789012345678901234567890abcd",
        ));
        assert!(lock.has(&make_key("actions/checkout", "v4")));
        assert!(!lock.has(&make_key("actions/checkout", "v3")));
    }

    #[test]
    fn test_retain() {
        let mut lock = Lock::default();
        lock.set(&make_resolved(
            "actions/checkout",
            "v4",
            "abc123def456789012345678901234567890abcd",
        ));
        lock.set(&make_resolved(
            "actions/setup-node",
            "v3",
            "def456789012345678901234567890abcd123456",
        ));
        lock.set(&make_resolved(
            "actions/old-action",
            "v1",
            "xyz789012345678901234567890abcd12345678a",
        ));

        let keep = vec![
            make_key("actions/checkout", "v4"),
            make_key("actions/setup-node", "v3"),
        ];
        lock.retain(&keep);

        assert!(lock.has(&make_key("actions/checkout", "v4")));
        assert!(lock.has(&make_key("actions/setup-node", "v3")));
        assert!(!lock.has(&make_key("actions/old-action", "v1")));
    }

    #[test]
    fn test_build_update_map() {
        let mut lock = Lock::default();
        lock.set(&make_resolved(
            "actions/checkout",
            "v4",
            "abc123def456789012345678901234567890abcd",
        ));
        lock.set(&make_resolved(
            "actions/setup-node",
            "v3",
            "def456789012345678901234567890abcd123456",
        ));

        let keys = vec![
            make_key("actions/checkout", "v4"),
            make_key("actions/setup-node", "v3"),
        ];
        let map = lock.build_update_map(&keys);

        assert_eq!(
            map.get(&ActionId::from("actions/checkout")),
            Some(&"abc123def456789012345678901234567890abcd # v4".to_string())
        );
        assert_eq!(
            map.get(&ActionId::from("actions/setup-node")),
            Some(&"def456789012345678901234567890abcd123456 # v3".to_string())
        );
    }

    #[test]
    fn test_build_update_map_missing_sha_falls_back_to_version() {
        let lock = Lock::default();
        let keys = vec![make_key("actions/checkout", "v4")];
        let map = lock.build_update_map(&keys);
        assert_eq!(
            map.get(&ActionId::from("actions/checkout")),
            Some(&"v4".to_string())
        );
    }

    #[test]
    fn test_update_existing_sha() {
        let mut lock = Lock::default();
        lock.set(&make_resolved(
            "actions/checkout",
            "v4",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        ));
        lock.set(&make_resolved(
            "actions/checkout",
            "v4",
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        ));
        let entry = lock.get(&make_key("actions/checkout", "v4"));
        assert!(entry.is_some());
        assert_eq!(
            entry.unwrap().sha,
            CommitSha::from("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb")
        );
    }

    // Tests for LockEntry::is_complete

    #[test]
    fn test_is_complete_with_all_fields() {
        let entry = LockEntry::with_version_and_specifier(
            CommitSha::from("abc123def456789012345678901234567890abcd"),
            Some("v4".to_string()),
            Some("^4".to_string()),
            "actions/checkout".to_string(),
            RefType::Tag,
            "2026-01-01T00:00:00Z".to_string(),
        );
        assert!(entry.is_complete(&Version::from("v4")));
    }

    #[test]
    fn test_is_complete_missing_specifier() {
        let entry = LockEntry::with_version_and_specifier(
            CommitSha::from("abc123def456789012345678901234567890abcd"),
            Some("v4".to_string()),
            None,
            "actions/checkout".to_string(),
            RefType::Tag,
            "2026-01-01T00:00:00Z".to_string(),
        );
        assert!(!entry.is_complete(&Version::from("v4")));
    }

    #[test]
    fn test_is_complete_empty_specifier_string() {
        let entry = LockEntry::with_version_and_specifier(
            CommitSha::from("abc123def456789012345678901234567890abcd"),
            Some("v4".to_string()),
            Some("".to_string()),
            "actions/checkout".to_string(),
            RefType::Tag,
            "2026-01-01T00:00:00Z".to_string(),
        );
        assert!(!entry.is_complete(&Version::from("v4")));
    }

    #[test]
    fn test_is_complete_stale_specifier() {
        // Version changed from v6 to v6.1, specifier is wrong
        let entry = LockEntry::with_version_and_specifier(
            CommitSha::from("abc123def456789012345678901234567890abcd"),
            Some("v6.0.2".to_string()),
            Some("^6".to_string()), // Should be ^6.1 for v6.1
            "actions/checkout".to_string(),
            RefType::Tag,
            "2026-01-01T00:00:00Z".to_string(),
        );
        assert!(!entry.is_complete(&Version::from("v6.1")));
    }

    #[test]
    fn test_is_complete_missing_version() {
        let entry = LockEntry::with_version_and_specifier(
            CommitSha::from("abc123def456789012345678901234567890abcd"),
            None,
            Some("^4".to_string()),
            "actions/checkout".to_string(),
            RefType::Tag,
            "2026-01-01T00:00:00Z".to_string(),
        );
        assert!(!entry.is_complete(&Version::from("v4")));
    }

    #[test]
    fn test_is_complete_missing_date() {
        let entry = LockEntry::with_version_and_specifier(
            CommitSha::from("abc123def456789012345678901234567890abcd"),
            Some("v4".to_string()),
            Some("^4".to_string()),
            "actions/checkout".to_string(),
            RefType::Tag,
            "".to_string(),
        );
        assert!(!entry.is_complete(&Version::from("v4")));
    }

    #[test]
    fn test_is_complete_non_semver_version() {
        // Non-semver (branch) versions have no specifier
        let entry = LockEntry::with_version_and_specifier(
            CommitSha::from("abc123def456789012345678901234567890abcd"),
            Some("main".to_string()),
            None,
            "actions/checkout".to_string(),
            RefType::Branch,
            "2026-01-01T00:00:00Z".to_string(),
        );
        assert!(entry.is_complete(&Version::from("main")));
    }

    #[test]
    fn test_is_complete_patch_version() {
        let entry = LockEntry::with_version_and_specifier(
            CommitSha::from("abc123def456789012345678901234567890abcd"),
            Some("v4.1.0".to_string()),
            Some("~4.1.0".to_string()),
            "actions/checkout".to_string(),
            RefType::Tag,
            "2026-01-01T00:00:00Z".to_string(),
        );
        assert!(entry.is_complete(&Version::from("v4.1.0")));
    }

    #[test]
    fn test_is_complete_minor_version() {
        let entry = LockEntry::with_version_and_specifier(
            CommitSha::from("abc123def456789012345678901234567890abcd"),
            Some("v4.1".to_string()),
            Some("^4.1".to_string()),
            "actions/checkout".to_string(),
            RefType::Tag,
            "2026-01-01T00:00:00Z".to_string(),
        );
        assert!(entry.is_complete(&Version::from("v4.1")));
    }
}
