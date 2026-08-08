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
/// `firstPatchedVersion`, so a caller rendering a finding must handle all three.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Remediation {
    /// A patched version exists and the manifest specifier admits it, so
    /// `gx upgrade <action>` reaches it.
    Upgradable {
        /// The advisory's first patched version, `v`-prefixed.
        fixed: Version,
    },
    /// The advisory names no patched version gx can deliver — none at all, one
    /// that is not a version, or a prerelease no range admits. Either way there
    /// is nothing to upgrade to and the user must migrate away from the action.
    NoFixAvailable,
    /// A patched version exists but the manifest specifier cannot reach it, so
    /// no `gx upgrade` invocation delivers it. Either a range excludes the fix
    /// (typically a major bump away) or the entry is a branch/SHA pin with no
    /// range to search — in both cases the manifest entry must change, and the
    /// caller has the specifier to say which.
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
        // An identifier that names no version gx can reach is `NoFixAvailable`
        // like an absent one — never `OutOfRange`, which would claim a wider
        // specifier reaches a fix that does not exist. A prerelease qualifies:
        // semver keeps it out of any range not already carrying one.
        let Some(patched) = first_patched
            .and_then(parse_semver)
            .filter(|v| v.pre.is_empty())
        else {
            return Self::NoFixAvailable;
        };
        // Canonicalize from the parsed version, not the raw identifier, so
        // `fixed` is always a concrete lowercase-`v` version: `V46.0.1` and
        // `2.37` reach the user as `v46.0.1` and `v2.37.0`.
        let fixed = Version::normalized(&patched.to_string());

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
            // Inverts `matches_version`, which reports `Ref`/`Sha` as exempt.
            // That asks whether a pin is permitted; this asks whether
            // `gx upgrade` would reach the fix — with no range to search, it
            // would not. The obstacle is the missing range, not its width, so a
            // caller must not render this arm as "requires a major bump".
            Specifier::Ref(_) | Specifier::Sha(_) => Self::OutOfRange { fixed },
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
    /// which reports a `Ref` as exempt. `OutOfRange` here means "the entry must
    /// become a range", not "bump the major" — the caller has the specifier and
    /// distinguishes the two.
    #[test]
    fn branch_specifier_is_never_upgradable() {
        assert_eq!(
            Remediation::classify(&Specifier::parse("main"), Some("46.0.1")),
            Remediation::OutOfRange {
                fixed: Version::from("v46.0.1")
            }
        );
    }

    /// A bare SHA pin likewise gives `gx upgrade` no range to search.
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

    /// A prerelease fix is not reachable: semver excludes prereleases from a
    /// range whose own bound carries none, so `^2` does not admit `2.1.0-beta.1`.
    /// Reported as `NoFixAvailable` rather than `OutOfRange` — widening the
    /// specifier is not what stands in the way, so telling the user a major
    /// bump is required would be false. Same reasoning as an unparseable
    /// identifier: there is no fix `gx upgrade` can deliver.
    #[test]
    fn prerelease_patched_version_has_no_reachable_fix() {
        assert_eq!(
            Remediation::classify(&Specifier::parse("^2"), Some("2.1.0-beta.1")),
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

    /// `parse_semver` accepts an uppercase `V`, so classification must too —
    /// and `fixed` must still come out in gx's lowercase `v` form rather than
    /// echoing the advisory's casing to the user.
    #[test]
    fn uppercase_v_prefix_is_canonicalized() {
        assert_eq!(
            Remediation::classify(&Specifier::parse("^46"), Some("V46.0.1")),
            Remediation::Upgradable {
                fixed: Version::from("v46.0.1")
            }
        );
    }

    /// An advisory may name an imprecise identifier. `parse_semver` pads it, so
    /// `fixed` must carry the concrete padded version — telling a user "fixed
    /// in v2" when the fix is v2.0.0 is range-shaped, not a version.
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
