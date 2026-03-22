use super::action::identity::Version;
use super::action::resolved::Commit;
use super::action::spec::Spec;
use super::diff::LockDiff;
use std::collections::{HashMap, HashSet};

/// A single lock entry: resolved version + commit metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
#[expect(
    clippy::module_name_repetitions,
    reason = "LockEntry is clearer than Entry when imported"
)]
pub struct LockEntry {
    pub version: Version,
    pub commit: Commit,
}

/// Domain entity representing the resolved lock state.
///
/// Contains all domain logic for querying and mutating the lock. No I/O.
#[derive(Debug, Default, Clone)]
pub struct Lock {
    /// Flat map of specifier to resolved entry.
    entries: HashMap<Spec, LockEntry>,
}

impl Lock {
    /// Create a `Lock` from a flat entry map.
    #[must_use]
    pub fn new(entries: HashMap<Spec, LockEntry>) -> Self {
        Self { entries }
    }

    /// Look up the lock entry for a spec.
    #[must_use]
    pub fn get(&self, spec: &Spec) -> Option<&LockEntry> {
        self.entries.get(spec)
    }

    /// Set or update the entry for a spec.
    pub fn set(&mut self, spec: &Spec, version: Version, commit: Commit) {
        self.entries
            .insert(spec.clone(), LockEntry { version, commit });
    }

    /// Check if the lock has an entry for the given spec.
    #[must_use]
    pub fn has(&self, key: &Spec) -> bool {
        self.entries.contains_key(key)
    }

    /// Check if a spec is complete (all fields populated).
    #[must_use]
    pub fn is_complete(&self, spec: &Spec) -> bool {
        let Some(entry) = self.entries.get(spec) else {
            return false;
        };
        if entry.version.as_str().is_empty() {
            return false;
        }
        !entry.commit.sha.as_str().is_empty()
            && !entry.commit.repository.as_str().is_empty()
            && entry.commit.ref_type.is_some()
            && !entry.commit.date.as_str().is_empty()
    }

    /// Set the version for a spec's entry.
    pub fn set_version(&mut self, spec: &Spec, version: Option<String>) {
        if let Some(entry) = self.entries.get_mut(spec)
            && let Some(v) = version
        {
            entry.version = Version::from(v.as_str());
        }
    }

    /// Retain only entries for the given specs, removing all others.
    pub fn retain(&mut self, keys: &[Spec]) {
        let keep: HashSet<&Spec> = keys.iter().collect();
        self.entries.retain(|k, _| keep.contains(k));
    }

    /// Iterate over entries.
    pub fn entries(&self) -> impl Iterator<Item = (&Spec, &LockEntry)> {
        self.entries.iter()
    }

    /// Check if the lock is empty (no entries).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Compute the diff between this lock (`before`) and `other` (`after`).
    ///
    /// Entries with the same key but different SHAs are treated as replacements
    /// (they appear in both `removed` and `added`).
    #[must_use]
    pub fn diff(&self, other: &Lock) -> LockDiff {
        let before_keys: HashSet<&Spec> = self.entries.keys().collect();
        let after_keys: HashSet<&Spec> = other.entries.keys().collect();

        let mut added: Vec<(Spec, LockEntry)> = Vec::new();
        let mut removed: Vec<Spec> = Vec::new();

        // New specs
        for &spec in after_keys.difference(&before_keys) {
            if let Some(entry) = other.get(spec) {
                added.push((spec.clone(), entry.clone()));
            }
        }

        // Removed specs
        for &spec in before_keys.difference(&after_keys) {
            removed.push(spec.clone());
        }

        // Changed specs (same key, different SHA)
        for &spec in before_keys.intersection(&after_keys) {
            let before_sha = self.get(spec).map(|e| &e.commit.sha);
            let after_sha = other.get(spec).map(|e| &e.commit.sha);
            if before_sha != after_sha {
                removed.push(spec.clone());
                if let Some(entry) = other.get(spec) {
                    added.push((spec.clone(), entry.clone()));
                }
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
#[expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "tests use unwrap, indexing, and other patterns freely"
)]
mod tests {
    use super::Lock;
    use crate::domain::action::identity::ActionId;
    use crate::domain::action::identity::{CommitDate, CommitSha, Repository, Version};
    use crate::domain::action::resolved::Commit;
    use crate::domain::action::spec::Spec;
    use crate::domain::action::specifier::Specifier;
    use crate::domain::action::uses_ref::RefType;

    fn make_key(action: &str, specifier: &str) -> Spec {
        Spec::new(ActionId::from(action), Specifier::parse(specifier))
    }

    fn make_commit(sha: &str) -> Commit {
        Commit {
            sha: CommitSha::from(sha),
            repository: Repository::from("actions/checkout"),
            ref_type: Some(RefType::Tag),
            date: CommitDate::from("2026-01-01T00:00:00Z"),
        }
    }

    fn set_action(lock: &mut Lock, action: &str, specifier: &str, sha: &str, version: &str) {
        let spec = make_key(action, specifier);
        let ver = Version::from(version);
        lock.set(&spec, ver, make_commit(sha));
    }

    #[test]
    fn new_empty() {
        let lock = Lock::default();
        assert!(lock.get(&make_key("actions/checkout", "^4")).is_none());
    }

    #[test]
    fn set_and_get() {
        let mut lock = Lock::default();
        set_action(
            &mut lock,
            "actions/checkout",
            "^4",
            "abc123def456789012345678901234567890abcd",
            "v4.2.1",
        );
        let result = lock.get(&make_key("actions/checkout", "^4"));
        assert!(result.is_some());
        let entry = result.unwrap();
        assert_eq!(
            entry.commit.sha,
            CommitSha::from("abc123def456789012345678901234567890abcd")
        );
        assert_eq!(entry.version, Version::from("v4.2.1"));
        assert!(lock.get(&make_key("actions/checkout", "^3")).is_none());
    }

    #[test]
    fn has() {
        let mut lock = Lock::default();
        set_action(
            &mut lock,
            "actions/checkout",
            "^4",
            "abc123def456789012345678901234567890abcd",
            "v4.2.1",
        );
        assert!(lock.has(&make_key("actions/checkout", "^4")));
        assert!(!lock.has(&make_key("actions/checkout", "^3")));
    }

    #[test]
    fn retain() {
        let mut lock = Lock::default();
        set_action(
            &mut lock,
            "actions/checkout",
            "^4",
            "abc123def456789012345678901234567890abcd",
            "v4.2.1",
        );
        set_action(
            &mut lock,
            "actions/setup-node",
            "^3",
            "def456789012345678901234567890abcd123456",
            "v3.1.0",
        );
        set_action(
            &mut lock,
            "actions/old-action",
            "^1",
            "xyz789012345678901234567890abcd12345678a",
            "v1.0.0",
        );

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
    fn update_existing_sha() {
        let mut lock = Lock::default();
        set_action(
            &mut lock,
            "actions/checkout",
            "^4",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "v4.2.1",
        );
        set_action(
            &mut lock,
            "actions/checkout",
            "^4",
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            "v4.2.1",
        );
        let result = lock.get(&make_key("actions/checkout", "^4"));
        assert!(result.is_some());
        let entry = result.unwrap();
        assert_eq!(
            entry.commit.sha,
            CommitSha::from("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb")
        );
    }

    #[test]
    fn is_complete_all_fields() {
        let mut lock = Lock::default();
        set_action(
            &mut lock,
            "actions/checkout",
            "^4",
            "abc123def456789012345678901234567890abcd",
            "v4.0.0",
        );
        assert!(lock.is_complete(&make_key("actions/checkout", "^4")));
    }

    #[test]
    fn is_complete_missing_resolution() {
        let lock = Lock::default();
        assert!(!lock.is_complete(&make_key("actions/checkout", "^4")));
    }

    #[test]
    fn is_complete_non_semver_ref() {
        let mut lock = Lock::default();
        let spec = make_key("actions/checkout", "main");
        lock.set(
            &spec,
            Version::from("main"),
            Commit {
                sha: CommitSha::from("abc123def456789012345678901234567890abcd"),
                repository: Repository::from("actions/checkout"),
                ref_type: Some(RefType::Branch),
                date: CommitDate::from("2026-01-01T00:00:00Z"),
            },
        );
        assert!(lock.is_complete(&spec));
    }

    #[test]
    fn set_version_updates_entry() {
        let mut lock = Lock::default();
        set_action(
            &mut lock,
            "actions/checkout",
            "^4",
            "abc123def456789012345678901234567890abcd",
            "v4",
        );
        let spec = make_key("actions/checkout", "^4");
        lock.set_version(&spec, Some("v4.2.1".to_owned()));

        let entry = lock.get(&spec).unwrap();
        assert_eq!(entry.version, Version::from("v4.2.1"));
        assert_eq!(
            entry.commit.sha,
            CommitSha::from("abc123def456789012345678901234567890abcd")
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
        set_action(
            &mut after,
            "actions/checkout",
            "^4",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "v4.0.0",
        );

        let diff = before.diff(&after);
        assert_eq!(diff.added.len(), 1);
        assert_eq!(diff.added[0].0, make_key("actions/checkout", "^4"));
        assert!(diff.removed.is_empty());
    }

    #[test]
    fn lock_diff_detects_removed_entry() {
        let mut before = Lock::default();
        set_action(
            &mut before,
            "actions/checkout",
            "^4",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "v4.0.0",
        );
        let after = Lock::default();

        let diff = before.diff(&after);
        assert!(diff.added.is_empty());
        assert_eq!(diff.removed.len(), 1);
        assert_eq!(diff.removed[0], make_key("actions/checkout", "^4"));
    }

    #[test]
    fn lock_diff_same_sha_not_in_diff() {
        let mut before = Lock::default();
        set_action(
            &mut before,
            "actions/checkout",
            "^4",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "v4.0.0",
        );
        let after = before.clone();

        let diff = before.diff(&after);
        assert!(diff.is_empty());
    }

    #[test]
    fn lock_diff_sha_replaced_appears_in_both_added_and_removed() {
        let mut before = Lock::default();
        set_action(
            &mut before,
            "actions/checkout",
            "^4",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "v4.0.0",
        );
        let mut after = Lock::default();
        set_action(
            &mut after,
            "actions/checkout",
            "^4",
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            "v4.0.0",
        );

        let diff = before.diff(&after);
        assert_eq!(diff.added.len(), 1, "replaced entry should appear in added");
        assert_eq!(
            diff.removed.len(),
            1,
            "replaced entry should appear in removed"
        );
    }
}
