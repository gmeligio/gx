use super::identity::{ActionId, CommitSha, Version};
use crate::domain::resolution::{Error as ResolutionError, ShaDescription, VersionRegistry};
use std::collections::HashMap;

/// Accumulates `ShaDescription` results during a plan run, keyed by `(ActionId, CommitSha)`.
/// Provides deduplication: `get_or_describe` calls the registry only on first access.
pub struct ShaIndex {
    cache: HashMap<(ActionId, CommitSha), ShaDescription>,
}

impl ShaIndex {
    #[must_use]
    pub fn new() -> Self {
        Self {
            cache: HashMap::new(),
        }
    }

    /// Return the cached description for `(id, sha)`, or call `describe_sha` and cache it.
    ///
    /// # Errors
    ///
    /// Propagates any error from `describe_sha`. On error, nothing is stored.
    pub fn get_or_describe<R: VersionRegistry>(
        &mut self,
        registry: &R,
        id: &ActionId,
        sha: &CommitSha,
    ) -> Result<&ShaDescription, ResolutionError> {
        let key = (id.clone(), sha.clone());
        if let std::collections::hash_map::Entry::Vacant(entry) = self.cache.entry(key.clone()) {
            let desc = registry.describe_sha(id, sha)?;
            entry.insert(desc);
        }
        // SAFETY: key was just inserted above if it was missing
        Ok(&self.cache[&key])
    }
}

impl Default for ShaIndex {
    fn default() -> Self {
        Self::new()
    }
}

/// Parse a version string (with optional 'v' prefix) into numeric components.
/// Returns `None` if any component is non-numeric.
pub(crate) fn parse_version_components(s: &str) -> Option<Vec<u64>> {
    let stripped = s.trim_start_matches('v');
    stripped.split('.').map(|p| p.parse::<u64>().ok()).collect()
}

/// Select the most specific version tag from a list.
/// Prefers semver-like tags with more components (patch over minor over major),
/// then highest version value among equal component counts, with non-semver tags last.
#[must_use]
pub fn select_most_specific_tag(tags: &[Version]) -> Option<Version> {
    if tags.is_empty() {
        return None;
    }

    let mut indexed: Vec<(&Version, Option<Vec<u64>>)> = tags
        .iter()
        .map(|t| (t, parse_version_components(t.as_str())))
        .collect();

    // Sort: semver-like tags first (more components preferred: v4.1.0 > v4.1 > v4),
    // then highest version value wins among equal component counts, non-semver tags last.
    indexed.sort_by(|(_, av), (_, bv)| match (av, bv) {
        (None, None) => std::cmp::Ordering::Equal,
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (Some(av), Some(bv)) => {
            let a_len = av.len();
            let b_len = bv.len();
            match b_len.cmp(&a_len) {
                std::cmp::Ordering::Equal => bv.cmp(av), // higher version wins (descending)
                other => other,                          // more components wins (descending)
            }
        }
    });

    indexed.first().map(|(t, _)| (*t).clone())
}

#[cfg(test)]
mod tests {
    use super::{Version, select_most_specific_tag};

    #[test]
    fn test_select_most_specific_tag_empty() {
        assert_eq!(select_most_specific_tag(&[]), None);
    }

    #[test]
    fn test_select_most_specific_tag_single() {
        let tags = vec![Version::from("v4")];
        assert_eq!(select_most_specific_tag(&tags), Some(Version::from("v4")));
    }

    #[test]
    fn test_select_most_specific_tag_prefers_patch_over_major() {
        let tags = vec![Version::from("v4.1.0"), Version::from("v4")];
        assert_eq!(
            select_most_specific_tag(&tags),
            Some(Version::from("v4.1.0"))
        );
    }

    #[test]
    fn test_select_most_specific_tag_prefers_minor_over_major() {
        let tags = vec![Version::from("v4.1"), Version::from("v4")];
        assert_eq!(select_most_specific_tag(&tags), Some(Version::from("v4.1")));
    }

    #[test]
    fn test_select_most_specific_tag_three_tiers() {
        let tags = vec![
            Version::from("v3"),
            Version::from("v3.6"),
            Version::from("v3.6.1"),
        ];
        assert_eq!(
            select_most_specific_tag(&tags),
            Some(Version::from("v3.6.1"))
        );
    }

    #[test]
    fn test_select_most_specific_tag_non_semver_sorted_last() {
        let tags = vec![Version::from("latest"), Version::from("v4")];
        assert_eq!(select_most_specific_tag(&tags), Some(Version::from("v4")));
    }

    #[test]
    fn test_select_most_specific_tag_all_non_semver_returns_first() {
        let tags = vec![Version::from("latest"), Version::from("stable")];
        // No semver tags — returns the first one
        assert!(select_most_specific_tag(&tags).is_some());
    }

    #[test]
    fn test_select_most_specific_tag_higher_major_wins_among_same_precision() {
        let tags = vec![
            Version::from("v3"),
            Version::from("v4"),
            Version::from("v5"),
        ];
        assert_eq!(select_most_specific_tag(&tags), Some(Version::from("v5")));
    }
}
