//! Whether a known vulnerability can be remediated with `gx upgrade`.
//!
//! `gx upgrade` moves an action only *within* its manifest specifier, so a patched version
//! is reachable only when the specifier admits it. Suggesting a command that cannot reach
//! the fix is worse than suggesting nothing — the user runs it mid-incident and nothing
//! changes — so this module errs conservative.

use super::action::identity::Version;
use super::action::specifier::{Specifier, parse_semver};

/// What a user can do about an action with a known vulnerability.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Remediation {
    /// `gx upgrade <action>` reaches the fix.
    Upgradable {
        /// The advisory's first patched version, `v`-prefixed.
        fixed: Version,
    },
    /// Nothing to upgrade to; the user must migrate away from the action.
    NoFixAvailable,
    /// A fix exists but the manifest entry must change to reach it.
    ///
    /// The obstacle is not always range width. Only a [`Specifier::Range`] needs widening;
    /// a branch, a SHA, and an exact pin like `46.0.0` have no range at all, so a caller
    /// must read the specifier before naming the obstacle.
    OutOfRange {
        /// The advisory's first patched version, `v`-prefixed.
        fixed: Version,
    },
}

impl Remediation {
    /// Classify how an advisory's fix relates to what the manifest allows.
    ///
    /// `first_patched` is the advisory's `firstPatchedVersion`, absent when there is no
    /// known fix, and `v`-prefixed or not. Whether the action is vulnerable at all is the
    /// caller's question; this only answers what to do about it.
    #[must_use]
    pub fn classify(specifier: &Specifier, first_patched: Option<&str>) -> Self {
        // An unreadable identifier is treated as an absent one. Reporting it as
        // `OutOfRange` would promise that a wider specifier reaches a fix.
        let Some(patched) = first_patched.and_then(parse_semver) else {
            return Self::NoFixAvailable;
        };
        // From the parsed version, not the raw identifier, so `V46.0.1` and `2.37` reach
        // the user as `v46.0.1` and `v2.37.0`.
        let fixed = Version::normalized(&patched.to_string());

        // `matches` reports a rangeless pin as unmatched, which is the verdict wanted here
        // — unlike `matches_version`, which exempts such pins because it asks whether a pin
        // is permitted, not whether an upgrade reaches it.
        if specifier.matches(&patched) {
            return Self::Upgradable { fixed };
        }
        // Semver admits a prerelease only into a range whose own bound carries one, so a
        // rejected prerelease is beyond every range the user could write — not merely
        // beyond this one.
        if patched.pre.is_empty() {
            Self::OutOfRange { fixed }
        } else {
            Self::NoFixAvailable
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Remediation, Specifier, Version};

    /// Real case: tj-actions/changed-files, GHSA-mrrh-fwg8-r2c3.
    #[test]
    fn caret_major_reaches_patch_in_same_major() {
        assert_eq!(
            Remediation::classify(&Specifier::parse("^46"), Some("46.0.1")),
            Remediation::Upgradable {
                fixed: Version::from("v46.0.1")
            }
        );
    }

    /// Real case: shivammathur/setup-php.
    #[test]
    fn tilde_minor_reaches_patch_on_same_minor_line() {
        assert_eq!(
            Remediation::classify(&Specifier::parse("~2.37"), Some("2.37.1")),
            Remediation::Upgradable {
                fixed: Version::from("v2.37.1")
            }
        );
    }

    /// Real case: github/codeql-action.
    #[test]
    fn caret_major_does_not_reach_next_major() {
        assert_eq!(
            Remediation::classify(&Specifier::parse("^2"), Some("3.0.0")),
            Remediation::OutOfRange {
                fixed: Version::from("v3.0.0")
            }
        );
    }

    /// Real case: aquasecurity/trivy-action. A caret on `0.x` is locked to that
    /// minor, so this looks like a reachable minor bump but is not.
    #[test]
    fn zero_major_caret_is_patch_locked() {
        assert_eq!(
            Remediation::classify(&Specifier::parse("^0.34"), Some("0.35.0")),
            Remediation::OutOfRange {
                fixed: Version::from("v0.35.0")
            }
        );
    }

    /// Real case: reviewdog/action-setup. 5 of 63 ACTIONS advisories name no
    /// patched version, so this arm is not hypothetical.
    #[test]
    fn absent_patched_version_has_no_fix() {
        assert_eq!(
            Remediation::classify(&Specifier::parse("^1"), None),
            Remediation::NoFixAvailable
        );
    }

    /// Advisories usually omit the `v` that gx tags carry.
    #[test]
    fn v_prefix_does_not_change_the_verdict() {
        let bare = Remediation::classify(&Specifier::parse("^46"), Some("46.0.1"));
        let prefixed = Remediation::classify(&Specifier::parse("^46"), Some("v46.0.1"));
        assert_eq!(bare, prefixed);
        assert_eq!(
            prefixed,
            Remediation::Upgradable {
                fixed: Version::from("v46.0.1")
            }
        );
    }

    #[test]
    fn v_prefix_does_not_change_an_out_of_range_verdict() {
        assert_eq!(
            Remediation::classify(&Specifier::parse("^2"), Some("v3.0.0")),
            Remediation::OutOfRange {
                fixed: Version::from("v3.0.0")
            }
        );
    }

    #[test]
    fn branch_specifier_is_never_upgradable() {
        assert_eq!(
            Remediation::classify(&Specifier::parse("main"), Some("46.0.1")),
            Remediation::OutOfRange {
                fixed: Version::from("v46.0.1")
            }
        );
    }

    #[test]
    fn sha_specifier_is_never_upgradable() {
        let sha = "a".repeat(40);
        assert_eq!(
            Remediation::classify(&Specifier::parse(&sha), Some("46.0.1")),
            Remediation::OutOfRange {
                fixed: Version::from("v46.0.1")
            }
        );
    }

    /// A manifest value is not validated into range form, so an exact pin becomes
    /// a `Ref`. The fix is one patch away and still unreachable — this is why
    /// `OutOfRange` must not be rendered as "requires a major bump".
    #[test]
    fn exact_pin_has_no_range_to_search() {
        assert_eq!(
            Remediation::classify(&Specifier::parse("46.0.0"), Some("46.0.1")),
            Remediation::OutOfRange {
                fixed: Version::from("v46.0.1")
            }
        );
    }

    /// Not `OutOfRange`: widening a specifier cannot reach a version that does
    /// not exist.
    #[test]
    fn unparseable_patched_version_has_no_fix() {
        assert_eq!(
            Remediation::classify(&Specifier::parse("^2"), Some("unreleased")),
            Remediation::NoFixAvailable
        );
    }

    /// `NoFixAvailable` rather than `OutOfRange`: no ordinary range admits a
    /// prerelease, so no specifier the user could write reaches this fix.
    #[test]
    fn prerelease_patched_version_has_no_reachable_fix() {
        assert_eq!(
            Remediation::classify(&Specifier::parse("^2"), Some("2.1.0-beta.1")),
            Remediation::NoFixAvailable
        );
    }

    /// A range carrying a prerelease does admit one, and gx derives such ranges
    /// (`Version::specifier` maps `v3.0.0-beta.2` to `~3.0.0-beta.2`), so
    /// filtering prereleases up front hid reachable fixes.
    #[test]
    fn prerelease_range_reaches_a_prerelease_fix() {
        assert_eq!(
            Remediation::classify(&Specifier::parse("^2.0.0-beta.1"), Some("2.0.0-beta.2")),
            Remediation::Upgradable {
                fixed: Version::from("v2.0.0-beta.2")
            }
        );
        assert_eq!(
            Remediation::classify(&Specifier::parse("~2.1.0-beta.1"), Some("2.1.0-beta.2")),
            Remediation::Upgradable {
                fixed: Version::from("v2.1.0-beta.2")
            }
        );
    }

    /// A prerelease range does not make every prerelease reachable.
    #[test]
    fn prerelease_range_does_not_reach_a_later_major_prerelease() {
        assert_eq!(
            Remediation::classify(&Specifier::parse("^2.0.0-beta.1"), Some("3.0.0-beta.1")),
            Remediation::NoFixAvailable
        );
    }

    #[test]
    fn empty_patched_version_has_no_fix() {
        assert_eq!(
            Remediation::classify(&Specifier::parse("^2"), Some("")),
            Remediation::NoFixAvailable
        );
    }

    /// `fixed` carries gx's lowercase `v`, not the advisory's casing.
    #[test]
    fn uppercase_v_prefix_is_canonicalized() {
        assert_eq!(
            Remediation::classify(&Specifier::parse("^46"), Some("V46.0.1")),
            Remediation::Upgradable {
                fixed: Version::from("v46.0.1")
            }
        );
    }

    /// Reporting "fixed in v2" would read as a range; the user needs a version.
    #[test]
    fn imprecise_patched_version_is_padded_to_a_concrete_version() {
        assert_eq!(
            Remediation::classify(&Specifier::parse("~2.37"), Some("2.37")),
            Remediation::Upgradable {
                fixed: Version::from("v2.37.0")
            }
        );
        assert_eq!(
            Remediation::classify(&Specifier::parse("^2"), Some("2")),
            Remediation::Upgradable {
                fixed: Version::from("v2.0.0")
            }
        );
    }
}
