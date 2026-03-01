use super::identity::{ActionId, Version, VersionPrecision};
use std::fmt;

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
            UpgradeAction::InRange { candidate } | UpgradeAction::CrossRange { candidate, .. } => {
                candidate
            }
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
