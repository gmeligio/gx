use super::action::identity::Version;
use super::action::resolved::Commit;
use super::action::spec::Spec;
use super::plan::LockDiff;
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
#[path = "tests.rs"]
mod tests;
