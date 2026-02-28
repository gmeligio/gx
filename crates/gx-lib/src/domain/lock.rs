use std::collections::{HashMap, HashSet};

use super::{ActionId, CommitSha, LockKey, ResolvedAction};

/// Domain entity representing the resolved lock state: maps action@version → commit SHA.
/// Contains all domain logic for querying and mutating the lock. No I/O.
#[derive(Debug, Default)]
pub struct Lock {
    actions: HashMap<LockKey, CommitSha>,
}

impl Lock {
    /// Create a `Lock` from an existing map of keys to SHAs.
    #[must_use]
    pub fn new(actions: HashMap<LockKey, CommitSha>) -> Self {
        Self { actions }
    }

    /// Get the locked commit SHA for a lock key.
    #[must_use]
    pub fn get(&self, key: &LockKey) -> Option<&CommitSha> {
        self.actions.get(key)
    }

    /// Set or update a locked action with its commit SHA.
    pub fn set(&mut self, resolved: &ResolvedAction) {
        let key = LockKey::from(resolved);
        self.actions.insert(key, resolved.sha.clone());
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
                let value = if let Some(sha) = self.get(key) {
                    let resolved =
                        ResolvedAction::new(key.id.clone(), key.version.clone(), sha.clone());
                    resolved.to_workflow_ref()
                } else {
                    key.version.to_string()
                };
                (key.id.clone(), value)
            })
            .collect()
    }

    /// Iterate over all (key, sha) entries.
    pub fn entries(&self) -> impl Iterator<Item = (&LockKey, &CommitSha)> {
        self.actions.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{ActionId, CommitSha, LockKey, ResolvedAction, Version};

    fn make_key(action: &str, version: &str) -> LockKey {
        LockKey::new(ActionId::from(action), Version::from(version))
    }

    fn make_resolved(action: &str, version: &str, sha: &str) -> ResolvedAction {
        ResolvedAction::new(
            ActionId::from(action),
            Version::from(version),
            CommitSha::from(sha),
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
        assert_eq!(
            lock.get(&make_key("actions/checkout", "v4")),
            Some(&CommitSha::from("abc123def456789012345678901234567890abcd"))
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
        assert_eq!(
            lock.get(&make_key("actions/checkout", "v4")),
            Some(&CommitSha::from("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"))
        );
    }
}
