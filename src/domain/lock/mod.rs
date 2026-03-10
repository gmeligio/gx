mod entry;

use super::plan::LockDiff;
use super::{ActionId, LockKey, ResolvedAction};
pub use entry::LockEntry;
use std::collections::{HashMap, HashSet};

/// Domain entity representing the resolved lock state: maps action@version → lock entry.
/// Contains all domain logic for querying and mutating the lock. No I/O.
#[derive(Debug, Default, Clone)]
pub struct Lock {
    pub(crate) actions: HashMap<LockKey, LockEntry>,
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
        let comment = resolved.version.to_comment().to_string();
        let entry = LockEntry::with_version_and_comment(
            resolved.sha.clone(),
            None,
            comment,
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

    /// Set the comment field for a lock entry.
    pub fn set_comment(&mut self, key: &LockKey, comment: String) {
        if let Some(entry) = self.actions.get_mut(key) {
            entry.set_comment(comment);
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

    /// Build a map of action IDs to "SHA # comment" strings for workflow updates.
    /// Falls back to the key version string if no SHA is found.
    #[must_use]
    pub fn build_update_map(&self, keys: &[LockKey]) -> HashMap<ActionId, String> {
        keys.iter()
            .map(|key| {
                let value = if let Some(entry) = self.get(key) {
                    format!("{} # {}", entry.sha, entry.comment)
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

    /// Compute the diff between this lock (`before`) and `other` (`after`).
    ///
    /// Entries with the same key but different SHAs are treated as replacements
    /// (they appear in both `removed` and `added`).
    #[must_use]
    pub fn diff(&self, other: &Lock) -> LockDiff {
        let before_keys: HashSet<LockKey> = self.entries().map(|(k, _)| k.clone()).collect();
        let after_keys: HashSet<LockKey> = other.entries().map(|(k, _)| k.clone()).collect();

        let mut added: Vec<(LockKey, LockEntry)> = after_keys
            .difference(&before_keys)
            .filter_map(|k| other.get(k).map(|e| (k.clone(), e.clone())))
            .collect();

        let mut removed: Vec<LockKey> = before_keys.difference(&after_keys).cloned().collect();

        // Detect changed entries (same key, different SHA) → treat as replace
        for key in before_keys.intersection(&after_keys) {
            if let (Some(b), Some(a)) = (self.get(key), other.get(key))
                && b.sha != a.sha
            {
                removed.push(key.clone());
                added.push((key.clone(), a.clone()));
            }
        }

        LockDiff {
            added,
            removed,
            updated: vec![],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ActionId, Lock, LockEntry, LockKey, ResolvedAction};
    use crate::domain::{CommitSha, RefType, Specifier};

    fn make_key(action: &str, specifier: &str) -> LockKey {
        LockKey::new(ActionId::from(action), Specifier::parse(specifier))
    }

    fn make_resolved(action: &str, specifier: &str, sha: &str) -> ResolvedAction {
        ResolvedAction::new(
            ActionId::from(action),
            Specifier::parse(specifier),
            CommitSha::from(sha),
            ActionId::from(action).base_repo(),
            Some(RefType::Tag),
            "2026-01-01T00:00:00Z".to_string(),
        )
    }

    fn make_entry(sha: &str) -> LockEntry {
        LockEntry::with_version_and_comment(
            CommitSha::from(sha),
            Some("v4.0.0".to_string()),
            "v4".to_string(),
            "actions/checkout".to_string(),
            Some(RefType::Tag),
            "2026-01-01T00:00:00Z".to_string(),
        )
    }

    #[test]
    fn test_new_empty() {
        let lock = Lock::default();
        assert!(lock.get(&make_key("actions/checkout", "^4")).is_none());
    }

    #[test]
    fn test_set_and_get() {
        let mut lock = Lock::default();
        lock.set(&make_resolved(
            "actions/checkout",
            "^4",
            "abc123def456789012345678901234567890abcd",
        ));
        let entry = lock.get(&make_key("actions/checkout", "^4"));
        assert!(entry.is_some());
        assert_eq!(
            entry.unwrap().sha,
            CommitSha::from("abc123def456789012345678901234567890abcd")
        );
        assert_eq!(lock.get(&make_key("actions/checkout", "^3")), None);
    }

    #[test]
    fn test_has() {
        let mut lock = Lock::default();
        lock.set(&make_resolved(
            "actions/checkout",
            "^4",
            "abc123def456789012345678901234567890abcd",
        ));
        assert!(lock.has(&make_key("actions/checkout", "^4")));
        assert!(!lock.has(&make_key("actions/checkout", "^3")));
    }

    #[test]
    fn test_retain() {
        let mut lock = Lock::default();
        lock.set(&make_resolved(
            "actions/checkout",
            "^4",
            "abc123def456789012345678901234567890abcd",
        ));
        lock.set(&make_resolved(
            "actions/setup-node",
            "^3",
            "def456789012345678901234567890abcd123456",
        ));
        lock.set(&make_resolved(
            "actions/old-action",
            "^1",
            "xyz789012345678901234567890abcd12345678a",
        ));

        let keep = vec![
            make_key("actions/checkout", "^4"),
            make_key("actions/setup-node", "^3"),
        ];
        lock.retain(&keep);

        assert!(lock.has(&make_key("actions/checkout", "^4")));
        assert!(lock.has(&make_key("actions/setup-node", "^3")));
        assert!(!lock.has(&make_key("actions/old-action", "^1")));
    }

    #[test]
    fn test_build_update_map() {
        let mut lock = Lock::default();
        let mut entry1 = LockEntry::new(
            CommitSha::from("abc123def456789012345678901234567890abcd"),
            "actions/checkout".to_string(),
            Some(RefType::Tag),
            "2026-01-01T00:00:00Z".to_string(),
        );
        entry1.set_comment("v4".to_string());
        lock.actions
            .insert(make_key("actions/checkout", "^4"), entry1);

        let mut entry2 = LockEntry::new(
            CommitSha::from("def456789012345678901234567890abcd123456"),
            "actions/setup-node".to_string(),
            Some(RefType::Tag),
            "2026-01-01T00:00:00Z".to_string(),
        );
        entry2.set_comment("v3".to_string());
        lock.actions
            .insert(make_key("actions/setup-node", "^3"), entry2);

        let keys = vec![
            make_key("actions/checkout", "^4"),
            make_key("actions/setup-node", "^3"),
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
        let keys = vec![make_key("actions/checkout", "^4")];
        let map = lock.build_update_map(&keys);
        assert_eq!(
            map.get(&ActionId::from("actions/checkout")),
            Some(&"^4".to_string())
        );
    }

    #[test]
    fn test_update_existing_sha() {
        let mut lock = Lock::default();
        lock.set(&make_resolved(
            "actions/checkout",
            "^4",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        ));
        lock.set(&make_resolved(
            "actions/checkout",
            "^4",
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        ));
        let entry = lock.get(&make_key("actions/checkout", "^4"));
        assert!(entry.is_some());
        assert_eq!(
            entry.unwrap().sha,
            CommitSha::from("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb")
        );
    }

    // --- Lock::diff tests ---

    #[test]
    fn lock_diff_empty_locks_is_empty() {
        let before = Lock::default();
        let after = Lock::default();
        assert!(before.diff(&after).is_empty());
    }

    #[test]
    fn lock_diff_detects_added_entry() {
        let before = Lock::default();
        let mut after = Lock::default();
        let key = make_key("actions/checkout", "^4");
        after.actions.insert(
            key.clone(),
            make_entry("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
        );

        let diff = before.diff(&after);
        assert_eq!(diff.added.len(), 1);
        assert_eq!(diff.added[0].0, key);
        assert!(diff.removed.is_empty());
    }

    #[test]
    fn lock_diff_detects_removed_entry() {
        let mut before = Lock::default();
        let key = make_key("actions/checkout", "^4");
        before.actions.insert(
            key.clone(),
            make_entry("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
        );
        let after = Lock::default();

        let diff = before.diff(&after);
        assert!(diff.added.is_empty());
        assert_eq!(diff.removed.len(), 1);
        assert_eq!(diff.removed[0], key);
    }

    #[test]
    fn lock_diff_same_sha_not_in_diff() {
        let mut before = Lock::default();
        let key = make_key("actions/checkout", "^4");
        before.actions.insert(
            key.clone(),
            make_entry("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
        );
        let after = before.clone();

        let diff = before.diff(&after);
        assert!(diff.is_empty());
    }

    #[test]
    fn lock_diff_sha_replaced_appears_in_both_added_and_removed() {
        let mut before = Lock::default();
        let key = make_key("actions/checkout", "^4");
        before.actions.insert(
            key.clone(),
            make_entry("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
        );
        let mut after = Lock::default();
        after.actions.insert(
            key.clone(),
            make_entry("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"),
        );

        let diff = before.diff(&after);
        assert_eq!(diff.added.len(), 1, "replaced entry should appear in added");
        assert_eq!(
            diff.removed.len(),
            1,
            "replaced entry should appear in removed"
        );
        assert_eq!(diff.added[0].0, key);
        assert_eq!(diff.removed[0], key);
    }
}
