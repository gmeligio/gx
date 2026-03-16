use super::identity::{ActionId, Version, VersionPrecision};
use super::specifier::Specifier;
use std::fmt;

/// Indicates what action to take when upgrading a version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Candidate is within the manifest's current range.
    /// Only the lock needs re-resolving; manifest stays unchanged.
    InRange { candidate: Version },
    /// Candidate is outside the manifest's range.
    /// Manifest must change.
    CrossRange {
        /// The candidate version tag to resolve (e.g., "v6.1.0").
        candidate: Version,
        /// The new specifier to write to the manifest (e.g., "^6").
        new_specifier: Specifier,
    },
}

/// An available upgrade for an action.
#[derive(Debug)]
pub struct Candidate {
    pub id: ActionId,
    pub current: Specifier,
    pub action: Action,
}

impl Candidate {
    /// Get the candidate version that will be resolved.
    #[must_use]
    pub fn candidate(&self) -> &Version {
        match &self.action {
            Action::InRange { candidate } | Action::CrossRange { candidate, .. } => candidate,
        }
    }

    /// Get the specifier to store in the manifest.
    #[must_use]
    pub fn manifest_specifier(&self) -> &Specifier {
        match &self.action {
            Action::InRange { .. } => &self.current,
            Action::CrossRange { new_specifier, .. } => new_specifier,
        }
    }
}

impl fmt::Display for Candidate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {} -> {}", self.id, self.current, self.candidate())
    }
}

/// Extract a precision-preserving specifier from a candidate tag.
/// Given a candidate version, a target precision, and the operator from the original specifier,
/// produces a new Specifier for the manifest.
///
/// Examples:
/// - Candidate `v3.0.0`, Major precision, operator `^` → `Specifier::parse("^3")`
/// - Candidate `v1.0.0`, Minor precision, operator `^` → `Specifier::parse("^1.0")`
/// - Candidate `v1.15.3`, Patch precision, operator `~` → `Specifier::parse("~1.15.3")`
fn extract_at_precision(
    candidate: &Version,
    precision: VersionPrecision,
    operator: char,
) -> Specifier {
    let stripped = candidate
        .0
        .strip_prefix('v')
        .or_else(|| candidate.0.strip_prefix('V'))
        .unwrap_or(&candidate.0);

    // Strip pre-release suffix to get the base version
    let base = stripped.split('-').next().unwrap_or(stripped);
    let parts: Vec<&str> = base.split('.').collect();

    let version_part = match (precision, parts.as_slice()) {
        (VersionPrecision::Patch, [major, minor, patch, ..]) => {
            format!("{major}.{minor}.{patch}")
        }
        (VersionPrecision::Minor, [major, minor, ..])
        | (VersionPrecision::Patch, [major, minor]) => format!("{major}.{minor}"),
        (VersionPrecision::Major, [major, ..]) | (_, [major]) => (*major).to_owned(),
        (_, _) => stripped.to_owned(),
    };

    let raw = format!("{operator}{version_part}");
    Specifier::parse(&raw)
}

/// Find the best upgrade candidate from a list of version tags.
///
/// Returns a richer type indicating whether the candidate is in-range or cross-range.
/// The comparison floor is `max(specifier_semver, lock_version_semver)`.
///
/// # Arguments
///
/// - `specifier` — the current specifier in the manifest (determines range constraint)
/// - `lock_version` — the resolved version from the lock file (if present, used as a floor)
/// - `candidates` — all available version tags (these are actual tags, not parsed)
/// - `allow_major` — if false (safe mode), constrain to same major version or major.minor range
///
/// # Returns
///
/// An `Action` indicating whether to update just the lock (`InRange`) or both manifest and lock (`CrossRange`).
/// Returns None if no suitable candidate exists.
#[must_use]
pub fn find_upgrade_candidate(
    specifier: &Specifier,
    lock_version: Option<&Version>,
    candidates: &[Version],
    allow_major: bool,
) -> Option<Action> {
    let precision = specifier.precision()?;
    let specifier_semver = parse_semver(specifier.as_str())?;

    // Determine if the specifier represents a pre-release
    let manifest_is_prerelease = !specifier_semver.pre.is_empty();

    // Compute the floor: max of specifier version and lock version
    let floor = if let Some(lock_ver) = lock_version {
        if let Some(lock_semver) = parse_semver(lock_ver.as_str()) {
            specifier_semver.clone().max(lock_semver)
        } else {
            specifier_semver.clone()
        }
    } else {
        specifier_semver.clone()
    };

    // Find the best candidate that is strictly greater than the floor
    // and (if !allow_major) satisfies the range constraint
    let best_tag = candidates
        .iter()
        .filter_map(|c| {
            let parsed = parse_semver(c.as_str())?;
            // Must be strictly higher than the floor
            if parsed <= floor {
                return None;
            }

            // Pre-release filtering: stable specifier excludes all pre-releases
            if !manifest_is_prerelease && !parsed.pre.is_empty() {
                return None;
            }

            // Apply range constraint if safe mode
            if allow_major {
                // Latest mode: no range constraint
                Some((c.clone(), parsed))
            } else {
                match precision {
                    VersionPrecision::Major | VersionPrecision::Minor => {
                        // Stay within same major version
                        (parsed.major == specifier_semver.major).then_some((c.clone(), parsed))
                    }
                    VersionPrecision::Patch => {
                        // Stay within same major.minor version
                        (parsed.major == specifier_semver.major
                            && parsed.minor == specifier_semver.minor)
                            .then_some((c.clone(), parsed))
                    }
                }
            }
        })
        .max_by(|(_, a), (_, b)| {
            // Prefer stable over pre-release when specifier is pre-release
            match (a.pre.is_empty(), b.pre.is_empty()) {
                (true, false) => std::cmp::Ordering::Greater,
                (false, true) => std::cmp::Ordering::Less,
                _ => a.cmp(b),
            }
        })
        .map(|(tag, _)| tag)?;

    // Determine if this is in-range or cross-range using VersionReq::matches
    if let Some(best_semver) = parse_semver(best_tag.as_str()) {
        let is_in_range = specifier.matches(&best_semver);

        if is_in_range {
            Some(Action::InRange {
                candidate: best_tag,
            })
        } else {
            let operator = specifier.operator().unwrap_or('^');
            let new_specifier = extract_at_precision(&best_tag, precision, operator);
            Some(Action::CrossRange {
                candidate: best_tag,
                new_specifier,
            })
        }
    } else {
        None
    }
}

/// Attempts to parse a version string into a semver Version.
/// Handles common formats like "v4", "v4.1", "v4.1.2", "4.1.2".
fn parse_semver(version: &str) -> Option<semver::Version> {
    // Strip leading 'v' or 'V' if present; also strip operators
    let normalized = version
        .trim_start_matches('^')
        .trim_start_matches('~')
        .strip_prefix('v')
        .or_else(|| {
            version
                .trim_start_matches('^')
                .trim_start_matches('~')
                .strip_prefix('V')
        })
        .unwrap_or_else(|| version.trim_start_matches('^').trim_start_matches('~'));

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
    use super::{Action, ActionId, Candidate, Specifier, Version, find_upgrade_candidate};

    #[test]
    fn find_upgrade_candidate_safe_mode_major_precision_in_range() {
        let specifier = Specifier::parse("^4");
        let candidates = vec![
            Version::from("v3"),
            Version::from("v4"),
            Version::from("v4.2.1"),
            Version::from("v5"),
            Version::from("v6"),
        ];
        // Safe mode, major precision: stays within v4.x
        // Best candidate within major is v4.2.1 (in-range)
        assert_eq!(
            find_upgrade_candidate(&specifier, None, &candidates, false),
            Some(Action::InRange {
                candidate: Version::from("v4.2.1")
            })
        );
    }

    #[test]
    fn find_upgrade_candidate_latest_mode_crosses_major() {
        let specifier = Specifier::parse("^4");
        let candidates = vec![
            Version::from("v4"),
            Version::from("v4.2.1"),
            Version::from("v5.0.0"),
            Version::from("v6.1.0"),
        ];
        // Latest mode: no range constraint, returns highest (cross-range)
        assert_eq!(
            find_upgrade_candidate(&specifier, None, &candidates, true),
            Some(Action::CrossRange {
                candidate: Version::from("v6.1.0"),
                new_specifier: Specifier::parse("^6"),
            })
        );
    }

    #[test]
    fn find_upgrade_candidate_latest_mode_preserves_minor_precision() {
        let specifier = Specifier::parse("^4.1");
        let candidates = vec![Version::from("v5.0.0")];
        // Latest mode with minor precision: result should preserve minor precision
        assert_eq!(
            find_upgrade_candidate(&specifier, None, &candidates, true),
            Some(Action::CrossRange {
                candidate: Version::from("v5.0.0"),
                new_specifier: Specifier::parse("^5.0"),
            })
        );
    }

    #[test]
    fn find_upgrade_candidate_latest_mode_preserves_patch_precision() {
        let specifier = Specifier::parse("~4.1.2");
        let candidates = vec![Version::from("v5.0.0")];
        // Latest mode with patch precision (tilde): result should preserve tilde and patch precision
        assert_eq!(
            find_upgrade_candidate(&specifier, None, &candidates, true),
            Some(Action::CrossRange {
                candidate: Version::from("v5.0.0"),
                new_specifier: Specifier::parse("~5.0.0"),
            })
        );
    }

    #[test]
    fn find_upgrade_candidate_with_lock_floor() {
        let specifier = Specifier::parse("^4");
        let lock_version = Some(Version::from("v4.2.1"));
        let candidates = vec![
            Version::from("v4.2.1"),
            Version::from("v4.3.0"),
            Version::from("v5.0.0"),
        ];
        // Safe mode with lock version as floor: v4.2.1 excluded, returns v4.3.0 (in-range)
        assert_eq!(
            find_upgrade_candidate(&specifier, lock_version.as_ref(), &candidates, false),
            Some(Action::InRange {
                candidate: Version::from("v4.3.0")
            })
        );
    }

    #[test]
    fn find_upgrade_candidate_stable_filters_prerelease() {
        let specifier = Specifier::parse("^2");
        let candidates = vec![
            Version::from("v2.2.1"),
            Version::from("v3.0.0"),
            Version::from("v3.0.0-beta.2"),
        ];
        // Stable specifier: pre-releases filtered out
        assert_eq!(
            find_upgrade_candidate(&specifier, None, &candidates, true),
            Some(Action::CrossRange {
                candidate: Version::from("v3.0.0"),
                new_specifier: Specifier::parse("^3"),
            })
        );
    }

    #[test]
    fn find_upgrade_candidate_non_semver_specifier() {
        let specifier = Specifier::Ref("main".to_owned());
        let candidates = vec![Version::from("v5")];
        // Non-semver specifier returns None (no precision)
        assert!(find_upgrade_candidate(&specifier, None, &candidates, true).is_none());
    }

    #[test]
    fn find_upgrade_candidate_no_candidates() {
        let specifier = Specifier::parse("^4");
        let candidates: Vec<Version> = vec![];
        assert!(find_upgrade_candidate(&specifier, None, &candidates, true).is_none());
    }

    #[test]
    fn upgrade_candidate_display_in_range() {
        let candidate = Candidate {
            id: ActionId::from("actions/checkout"),
            current: Specifier::parse("^4"),
            action: Action::InRange {
                candidate: Version::from("v4.5.0"),
            },
        };
        assert_eq!(candidate.to_string(), "actions/checkout ^4 -> v4.5.0");
    }

    #[test]
    fn upgrade_candidate_display_cross_range() {
        let candidate = Candidate {
            id: ActionId::from("actions/checkout"),
            current: Specifier::parse("^4"),
            action: Action::CrossRange {
                candidate: Version::from("v5.0.0"),
                new_specifier: Specifier::parse("^5"),
            },
        };
        assert_eq!(candidate.to_string(), "actions/checkout ^4 -> v5.0.0");
    }
}
