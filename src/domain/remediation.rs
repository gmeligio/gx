//! Whether a known vulnerability can be remediated with `gx upgrade`.
//!
//! `gx upgrade` moves an action only *within* its manifest specifier. So an
//! advisory's patched version is reachable only when the specifier admits it.
//! Suggesting a command that cannot reach the fix is worse than suggesting
//! nothing: the user runs it during a security incident, nothing changes, and
//! they lose trust in the tool. This module makes that call conservatively.

use super::action::identity::Version;
use super::action::resolved::ResolvedRef;
use super::action::specifier::{Specifier, parse_semver};

/// What a user can do about an action with a known vulnerability.
///
/// Produced from a manifest [`Specifier`] plus the advisory's
/// `firstPatchedVersion`. Exhaustive by construction: every input lands in
/// exactly one variant, so a caller rendering a finding must handle all three.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Remediation {
    /// A patched version exists and the manifest specifier admits it, so
    /// `gx upgrade <action>` reaches it.
    Upgradable {
        /// The advisory's first patched version, `v`-prefixed.
        fixed: Version,
    },
    /// The advisory names no patched version, or names one that cannot be
    /// interpreted as a version. Either way there is nothing to upgrade to and
    /// the user must migrate away from the action.
    NoFixAvailable,
    /// A patched version exists but falls outside the manifest specifier, so
    /// no `gx upgrade` invocation reaches it — the user must widen the
    /// specifier, typically across a major version.
    OutOfRange {
        /// The advisory's first patched version, `v`-prefixed.
        fixed: Version,
    },
}

impl Remediation {
    /// Classify how an advisory's fix relates to what the manifest allows.
    ///
    /// `first_patched` is the advisory's `firstPatchedVersion` identifier, which
    /// is absent for advisories with no known fix. It may or may not carry a
    /// `v` prefix; the verdict does not depend on which form is used.
    ///
    /// Determining that the action is vulnerable at all is the caller's job —
    /// this only answers what to do about it.
    #[must_use]
    pub fn classify(specifier: &Specifier, first_patched: Option<&str>) -> Self {
        // Parse before normalizing. An identifier that is not a version has no
        // reachable fix, so it is `NoFixAvailable` like an absent one — never
        // `OutOfRange`, which would claim a wider specifier could reach a fix
        // that does not exist.
        let Some(identifier) = first_patched.filter(|id| parse_semver(id).is_some()) else {
            return Self::NoFixAvailable;
        };
        let fixed = Version::normalized(identifier);

        match specifier {
            // The patched version from an advisory is always a version tag,
            // so `Branch`/`Commit` are unreachable here by construction.
            Specifier::Range { .. } => {
                if specifier.matches_version(&ResolvedRef::Tag(fixed.clone())) {
                    Self::Upgradable { fixed }
                } else {
                    Self::OutOfRange { fixed }
                }
            }
            // Note the inverted polarity against `Specifier::matches_version`,
            // which reports `Ref`/`Sha` as *exempt* (`true`) because it asks
            // whether an existing pin is permitted. The question here is
            // whether `gx upgrade` would *reach* the fix, and a branch or bare
            // SHA gives it no range to search — so it would not.
            Specifier::Ref(_) | Specifier::Sha(_) => Self::OutOfRange { fixed },
        }
    }

    /// The version to upgrade to, when `gx upgrade` can reach the fix.
    ///
    /// `None` for both no-command outcomes, so a caller can gate the suggestion
    /// on this without re-matching the variants.
    #[must_use]
    pub const fn upgrade_target(&self) -> Option<&Version> {
        match self {
            Self::Upgradable { fixed } => Some(fixed),
            Self::NoFixAvailable | Self::OutOfRange { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Remediation, Specifier, Version};

    /// The real tj-actions/changed-files case: GHSA-mrrh-fwg8-r2c3 is fixed in
    /// 46.0.1, and a `^46` manifest reaches it.
    #[test]
    fn caret_major_reaches_patch_in_same_major() {
        assert_eq!(
            Remediation::classify(&Specifier::parse("^46"), Some("46.0.1")),
            Remediation::Upgradable {
                fixed: Version::from("v46.0.1")
            }
        );
    }

    /// The real shivammathur/setup-php case: fixed in 2.37.1, and a `~2.37`
    /// manifest reaches it because the fix is on the same minor line.
    #[test]
    fn tilde_minor_reaches_patch_on_same_minor_line() {
        assert_eq!(
            Remediation::classify(&Specifier::parse("~2.37"), Some("2.37.1")),
            Remediation::Upgradable {
                fixed: Version::from("v2.37.1")
            }
        );
    }

    /// The real github/codeql-action case: the fix is 3.0.0, which a `^2`
    /// manifest can never reach — it is a major bump.
    #[test]
    fn caret_major_does_not_reach_next_major() {
        assert_eq!(
            Remediation::classify(&Specifier::parse("^2"), Some("3.0.0")),
            Remediation::OutOfRange {
                fixed: Version::from("v3.0.0")
            }
        );
    }

    /// The real aquasecurity/trivy-action case. A caret on a `0.x` version is
    /// locked to that minor under semver, so `^0.34` does not reach 0.35.0
    /// even though it looks like a mere minor bump.
    #[test]
    fn zero_major_caret_is_patch_locked() {
        assert_eq!(
            Remediation::classify(&Specifier::parse("^0.34"), Some("0.35.0")),
            Remediation::OutOfRange {
                fixed: Version::from("v0.35.0")
            }
        );
    }

    /// The real reviewdog/action-setup case: the advisory covers `= 1` and
    /// carries no `firstPatchedVersion` at all. 5 of 63 ACTIONS advisories are
    /// like this, so the branch is not hypothetical.
    #[test]
    fn absent_patched_version_has_no_fix() {
        assert_eq!(
            Remediation::classify(&Specifier::parse("^1"), None),
            Remediation::NoFixAvailable
        );
    }

    /// Advisory identifiers usually omit the `v` that gx tags carry. Both forms
    /// must reach the same verdict and yield the same normalized `fixed`.
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

    /// The `v` prefix must not flip an out-of-range verdict either.
    #[test]
    fn v_prefix_does_not_change_an_out_of_range_verdict() {
        assert_eq!(
            Remediation::classify(&Specifier::parse("^2"), Some("v3.0.0")),
            Remediation::OutOfRange {
                fixed: Version::from("v3.0.0")
            }
        );
    }

    /// A branch is not governed by a range, so `gx upgrade` cannot be relied on
    /// to move it. This is the case that inverts `Specifier::matches_version`,
    /// which reports a `Ref` as exempt.
    #[test]
    fn branch_specifier_is_never_upgradable() {
        let remediation = Remediation::classify(&Specifier::parse("main"), Some("46.0.1"));
        assert_eq!(
            remediation,
            Remediation::OutOfRange {
                fixed: Version::from("v46.0.1")
            }
        );
        assert!(remediation.upgrade_target().is_none());
    }

    /// A bare SHA pin likewise gives `gx upgrade` no range to search.
    #[test]
    fn sha_specifier_is_never_upgradable() {
        let sha = "a".repeat(40);
        let remediation = Remediation::classify(&Specifier::parse(&sha), Some("46.0.1"));
        assert_eq!(
            remediation,
            Remediation::OutOfRange {
                fixed: Version::from("v46.0.1")
            }
        );
        assert!(remediation.upgrade_target().is_none());
    }

    /// An identifier that is not a version must not be reported as a fix beyond
    /// the user's range — that would tell them to widen the specifier toward
    /// something that does not exist.
    #[test]
    fn unparseable_patched_version_has_no_fix() {
        assert_eq!(
            Remediation::classify(&Specifier::parse("^2"), Some("unreleased")),
            Remediation::NoFixAvailable
        );
    }

    /// An empty identifier is as unusable as an absent one.
    #[test]
    fn empty_patched_version_has_no_fix() {
        assert_eq!(
            Remediation::classify(&Specifier::parse("^2"), Some("")),
            Remediation::NoFixAvailable
        );
    }

    /// Only the upgradable outcome yields a command target.
    #[test]
    fn only_upgradable_yields_an_upgrade_target() {
        assert_eq!(
            Remediation::classify(&Specifier::parse("^46"), Some("46.0.1")).upgrade_target(),
            Some(&Version::from("v46.0.1"))
        );
        assert!(
            Remediation::classify(&Specifier::parse("^2"), Some("3.0.0"))
                .upgrade_target()
                .is_none()
        );
        assert!(
            Remediation::classify(&Specifier::parse("^1"), None)
                .upgrade_target()
                .is_none()
        );
    }
}
