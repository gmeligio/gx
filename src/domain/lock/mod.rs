pub mod resolution;

use super::action::identity::{ActionId, Version, VersionComment};
use super::action::resolved::{Commit, Resolved};
use super::action::spec::Spec;
use super::action::specifier::Specifier;
use super::plan::LockDiff;
use resolution::Resolution;
use std::collections::{HashMap, HashSet};

/// Key for the actions tier: (`ActionId`, resolved `Version`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ActionKey {
    pub id: ActionId,
    pub version: Version,
}

/// Domain entity representing the resolved lock state with two tiers:
/// - `resolutions`: maps `Spec` → `Resolution` (resolved version + comment)
/// - `actions`: maps `ActionKey` → `Commit` (SHA + metadata)
///
/// Contains all domain logic for querying and mutating the lock. No I/O.
#[derive(Debug, Default, Clone)]
pub struct Lock {
    /// Tier 1: specifier → resolved version + comment.
    pub resolutions: HashMap<Spec, Resolution>,
    /// Tier 2: (action, version) → commit metadata.
    pub actions: HashMap<ActionKey, Commit>,
}

impl Lock {
    /// Create a `Lock` from two-tier maps.
    #[must_use]
    pub fn new(
        resolutions: HashMap<Spec, Resolution>,
        actions: HashMap<ActionKey, Commit>,
    ) -> Self {
        Self {
            resolutions,
            actions,
        }
    }

    /// Two-step lookup: spec → resolution → commit.
    /// Returns both the resolution and the commit metadata.
    #[must_use]
    pub fn get(&self, spec: &Spec) -> Option<(&Resolution, &Commit)> {
        let resolution = self.resolutions.get(spec)?;
        let key = ActionKey {
            id: spec.id.clone(),
            version: resolution.version.clone(),
        };
        let commit = self.actions.get(&key)?;
        Some((resolution, commit))
    }

    /// Set or update both tiers for a spec.
    pub fn set(&mut self, spec: &Spec, version: Version, commit: Commit, comment: VersionComment) {
        self.resolutions.insert(
            spec.clone(),
            Resolution {
                version: version.clone(),
                comment,
            },
        );
        let action_key = ActionKey {
            id: spec.id.clone(),
            version,
        };
        self.actions.insert(action_key, commit);
    }

    /// Set from a `Resolved` action (convenience for callers that have a Resolved).
    pub fn set_resolved(&mut self, resolved: Resolved) {
        let spec = Spec::new(resolved.id.clone(), resolved.version.clone());
        let comment_str = resolved.version.to_comment();
        let comment = VersionComment::from(comment_str);
        // Version initially set to the specifier's comment (e.g., "v4") — will be refined later
        let version = Version::from(comment_str);
        self.set(&spec, version, resolved.commit, comment);
    }

    /// Check if the lock has a resolution for the given spec.
    #[must_use]
    pub fn has(&self, key: &Spec) -> bool {
        self.resolutions.contains_key(key)
    }

    /// Check if a spec is complete across both tiers.
    #[must_use]
    pub fn is_complete(&self, spec: &Spec) -> bool {
        let Some(resolution) = self.resolutions.get(spec) else {
            return false;
        };
        if resolution.version.as_str().is_empty() {
            return false;
        }
        // For semver ranges, comment must match the specifier's expected comment
        let comment_ok = match &spec.version {
            Specifier::Range { .. } => resolution.comment.as_str() == spec.version.to_comment(),
            Specifier::Ref(_) | Specifier::Sha(_) => true,
        };
        if !comment_ok {
            return false;
        }
        let key = ActionKey {
            id: spec.id.clone(),
            version: resolution.version.clone(),
        };
        let Some(commit) = self.actions.get(&key) else {
            return false;
        };
        !commit.sha.as_str().is_empty()
            && !commit.repository.as_str().is_empty()
            && commit.ref_type.is_some()
            && !commit.date.as_str().is_empty()
    }

    /// Set the version for a spec's resolution and update the action key.
    pub fn set_version(&mut self, spec: &Spec, version: Option<String>) {
        if let Some(resolution) = self.resolutions.get_mut(spec)
            && let Some(v) = version
        {
            let old_key = ActionKey {
                id: spec.id.clone(),
                version: resolution.version.clone(),
            };
            let new_version = Version::from(v.as_str());
            // Move the commit entry to the new key if version changed
            if resolution.version != new_version
                && let Some(commit) = self.actions.remove(&old_key)
            {
                let new_key = ActionKey {
                    id: spec.id.clone(),
                    version: new_version.clone(),
                };
                self.actions.insert(new_key, commit);
            }
            resolution.version = new_version;
        }
    }

    /// Set the comment for a spec's resolution.
    pub fn set_comment(&mut self, spec: &Spec, comment: VersionComment) {
        if let Some(resolution) = self.resolutions.get_mut(spec) {
            resolution.comment = comment;
        }
    }

    /// Retain only resolutions for the given specs, removing all others.
    /// Does NOT prune orphaned action entries — use `cleanup_orphans()` for that.
    pub fn retain(&mut self, keys: &[Spec]) {
        let keep: HashSet<&Spec> = keys.iter().collect();
        self.resolutions.retain(|k, _| keep.contains(k));
    }

    /// Prune action entries that are not referenced by any resolution.
    pub fn cleanup_orphans(&mut self) {
        let referenced: HashSet<ActionKey> = self
            .resolutions
            .iter()
            .map(|(spec, res)| ActionKey {
                id: spec.id.clone(),
                version: res.version.clone(),
            })
            .collect();
        self.actions.retain(|k, _| referenced.contains(k));
    }

    /// Build a map of action IDs to "SHA # comment" strings for workflow updates.
    /// Falls back to the key version string if no resolution/commit is found.
    #[must_use]
    pub fn build_update_map(&self, keys: &[Spec]) -> HashMap<ActionId, String> {
        keys.iter()
            .map(|spec| {
                if let Some((res, commit)) = self.get(spec) {
                    (spec.id.clone(), format!("{} # {}", commit.sha, res.comment))
                } else {
                    (spec.id.clone(), spec.version.to_string())
                }
            })
            .collect()
    }

    /// Iterate over resolutions.
    pub fn resolution_entries(&self) -> impl Iterator<Item = (&Spec, &Resolution)> {
        self.resolutions.iter()
    }

    /// Iterate over action entries.
    pub fn action_entries(&self) -> impl Iterator<Item = (&ActionKey, &Commit)> {
        self.actions.iter()
    }

    /// Check if the lock is empty (no resolutions).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.resolutions.is_empty()
    }

    /// Compute the diff between this lock (`before`) and `other` (`after`).
    ///
    /// Operates on the resolutions tier. Entries with the same key but different
    /// SHAs are treated as replacements (they appear in both `removed` and `added`).
    #[must_use]
    pub fn diff(&self, other: &Lock) -> LockDiff {
        let before_keys: HashSet<&Spec> = self.resolutions.keys().collect();
        let after_keys: HashSet<&Spec> = other.resolutions.keys().collect();

        let mut added: Vec<(Spec, Resolution, Commit)> = Vec::new();
        let mut removed: Vec<Spec> = Vec::new();

        // New specs
        for &spec in after_keys.difference(&before_keys) {
            if let Some((res, commit)) = other.get(spec) {
                added.push((spec.clone(), res.clone(), commit.clone()));
            }
        }

        // Removed specs
        for &spec in before_keys.difference(&after_keys) {
            removed.push(spec.clone());
        }

        // Changed specs (same key, different SHA)
        for &spec in before_keys.intersection(&after_keys) {
            let before_sha = self.get(spec).map(|(_, c)| &c.sha);
            let after_sha = other.get(spec).map(|(_, c)| &c.sha);
            if before_sha != after_sha {
                removed.push(spec.clone());
                if let Some((res, commit)) = other.get(spec) {
                    added.push((spec.clone(), res.clone(), commit.clone()));
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
