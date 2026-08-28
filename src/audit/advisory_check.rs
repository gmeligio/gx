//! The `known-vulnerability` check: locked actions against the GitHub Advisory Database.
//!
//! Range matching happens here rather than server-side because OSV's `GitHub Actions`
//! version filter returns an empty result for known-vulnerable versions — a false "clean",
//! which for a security check is worse than not shipping the check at all. Every path that
//! could report "not affected" without having established it routes to
//! [`Match::Undetermined`] instead.

use super::check_name::CheckName;
use super::report::Finding;
use super::target::AuditTarget;
use crate::config::Level;
use crate::infra::github::{Advisory, AdvisorySeverity};
use semver::{Op, Version, VersionReq};
use std::collections::HashMap;

/// A vulnerable pin fails the build: the action is running code a published advisory says
/// is exploitable.
const VULNERABLE_SEVERITY: Level = Level::Error;

/// An advisory gx could not place is a warning, not an error. The action may well be fine,
/// and failing a build on an unknown is the false alarm that costs a security tool its
/// credibility. Silence would be the opposite and worse failure.
const UNDETERMINED_SEVERITY: Level = Level::Warn;

/// Whether an advisory's range covers a locked version.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Match {
    /// The locked version falls inside the affected range.
    Affected,
    /// The locked version provably falls outside it.
    Unaffected,
    /// Neither could be established. Never collapsed into `Unaffected`.
    Undetermined,
}

/// How precisely a locked version names a release.
///
/// A partial version is a real pin — `v41` is a tag someone wrote — but it names a line,
/// not a release, and the distinction decides whether padding it is sound.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Precision {
    /// `41` — the whole 41 line.
    Major,
    /// `4.2` — the whole 4.2 line.
    Minor,
    /// `4.2.1` — one release.
    Exact,
}

/// A locked version parsed for comparison, remembering how precise it was.
struct Locked {
    /// The version zero-padded to three components, so `semver` can compare it.
    version: Version,
    /// What the original string actually named.
    precision: Precision,
}

/// Parse a locked version label into something comparable.
///
/// Strips a leading `v`, following the convention
/// [`crate::domain::action::identity::Version::precision`] already sets, and zero-pads
/// partial versions so `semver` accepts them. Returns `None` for anything that is not a
/// version at all — a branch name, a SHA — which the caller reports rather than ignores.
fn parse_locked(label: &str) -> Option<Locked> {
    let bare = label
        .strip_prefix('v')
        .or_else(|| label.strip_prefix('V'))
        .unwrap_or(label);

    // Split the pre-release suffix off before counting components, so `3.0.0-beta.2` is
    // exact rather than unparseable.
    let (core, suffix) = bare.split_once('-').map_or((bare, ""), |(c, s)| (c, s));
    let numeric = |part: &&str| !part.is_empty() && part.chars().all(|c| c.is_ascii_digit());
    let parts: Vec<&str> = core.split('.').collect();
    if !parts.iter().all(numeric) {
        return None;
    }

    let (padded, precision) = match parts.as_slice() {
        [major] => (format!("{major}.0.0"), Precision::Major),
        [major, minor] => (format!("{major}.{minor}.0"), Precision::Minor),
        [major, minor, patch] => (format!("{major}.{minor}.{patch}"), Precision::Exact),
        _ => return None,
    };
    let full = if suffix.is_empty() {
        padded
    } else {
        format!("{padded}-{suffix}")
    };

    Version::parse(&full)
        .ok()
        .map(|version| Locked { version, precision })
}

/// Whether a partial locked version could sit on either side of one of the range's lower
/// bounds.
///
/// This is the last false-clean route. A `v41` tag points at the 41 line's *head*, not its
/// floor, so padding it to `41.0.0` is only sound against an upper bound: there it errs
/// toward reporting, and a false alarm is dismissable. Against a lower bound it errs toward
/// silence — locked `v2` padded to `2.0.0` reads as outside `>= 2.5.0, < 3.0.0` even though
/// the 2 line's head may be `2.7.x` and genuinely affected.
///
/// So when a lower bound lands inside the locked version's own line, gx cannot tell which
/// side the tag sits on and says so.
fn straddles_a_lower_bound(locked: &Locked, range: &VersionReq) -> bool {
    if locked.precision == Precision::Exact {
        return false;
    }
    range.comparators.iter().any(|bound| {
        if !matches!(bound.op, Op::Greater | Op::GreaterEq) {
            return false;
        }
        // Same major, and for a `major.minor` pin the same minor too: only then does the
        // bound fall within the span of releases the tag could resolve to.
        bound.major == locked.version.major
            && (locked.precision == Precision::Major || bound.minor == Some(locked.version.minor))
    })
}

/// Decide whether an advisory's affected range covers a locked version.
fn matches(label: &str, range: &str) -> Match {
    let (Some(locked), Ok(requirement)) = (parse_locked(label), VersionReq::parse(range)) else {
        return Match::Undetermined;
    };

    if requirement.matches(&locked.version) {
        return Match::Affected;
    }
    if straddles_a_lower_bound(&locked, &requirement) {
        return Match::Undetermined;
    }
    Match::Unaffected
}

/// How GitHub's severity reads in a finding.
const fn severity_label(severity: AdvisorySeverity) -> &'static str {
    match severity {
        AdvisorySeverity::Low => "low",
        AdvisorySeverity::Moderate => "moderate",
        AdvisorySeverity::High => "high",
        AdvisorySeverity::Critical => "critical",
    }
}

/// The advisory set, indexed for lookup by repository.
///
/// Built once per run rather than scanned per action, and keyed on the lowercased package
/// name because GitHub slugs are case-insensitive while the published names are mixed case
/// — comparing them literally would miss a real advisory.
pub struct Advisories<'set> {
    /// Lowercased package name to the advisories published against it.
    by_package: HashMap<String, Vec<&'set Advisory>>,
}

impl<'set> Advisories<'set> {
    /// Index an advisory set for lookup.
    pub fn index(advisories: &'set [Advisory]) -> Self {
        let mut by_package: HashMap<String, Vec<&'set Advisory>> = HashMap::new();
        for advisory in advisories {
            by_package
                .entry(advisory.package.to_lowercase())
                .or_default()
                .push(advisory);
        }
        Self { by_package }
    }

    /// Report every advisory that covers this target, and every one gx could not place.
    ///
    /// Silent for an action with no advisories, however imprecise its version: there is
    /// nothing whose applicability is in question.
    pub fn known_vulnerabilities(&self, target: &AuditTarget<'_>) -> Vec<Finding> {
        let Some(repository) = target.repository else {
            return Vec::new();
        };
        let Some(advisories) = self.by_package.get(&repository.as_str().to_lowercase()) else {
            return Vec::new();
        };

        advisories
            .iter()
            .filter_map(
                |advisory| match matches(target.version, &advisory.vulnerable_range) {
                    Match::Affected => Some(vulnerable_finding(target, advisory)),
                    Match::Undetermined => Some(undetermined_finding(target, advisory)),
                    Match::Unaffected => None,
                },
            )
            .collect()
    }
}

/// The finding for a pin an advisory covers.
///
/// Carries the permalink because a security claim a user cannot verify is one they must
/// either take on faith or ignore, and both are failures.
fn vulnerable_finding(target: &AuditTarget<'_>, advisory: &Advisory) -> Finding {
    let fix = advisory.first_patched.as_ref().map_or_else(
        || "no fixed version published".to_owned(),
        |version| format!("fixed in {version}"),
    );
    Finding::new(
        CheckName::KnownVulnerability,
        VULNERABLE_SEVERITY,
        format!(
            "{} {} is affected by {} ({} severity, affects {}); {} — {}",
            target.id,
            target.version,
            advisory.ghsa_id,
            severity_label(advisory.severity),
            advisory.vulnerable_range,
            fix,
            advisory.permalink
        ),
    )
}

/// The finding for an advisory gx could not place against the locked version.
fn undetermined_finding(target: &AuditTarget<'_>, advisory: &Advisory) -> Finding {
    Finding::new(
        CheckName::KnownVulnerability,
        UNDETERMINED_SEVERITY,
        format!(
            "{} {} may be affected by {} ({} severity, affects {}): \
             gx cannot tell whether {} falls in that range — {}",
            target.id,
            target.version,
            advisory.ghsa_id,
            severity_label(advisory.severity),
            advisory.vulnerable_range,
            target.version,
            advisory.permalink
        ),
    )
}

#[cfg(test)]
#[expect(
    clippy::indexing_slicing,
    reason = "tests use unwrap, indexing, and other patterns freely"
)]
mod tests {
    use super::{Advisories, AdvisorySeverity, Level, Match, matches};
    use crate::audit::target::{AuditTarget, targets};
    use crate::domain::action::identity::{ActionId, CommitDate, CommitSha, Repository, Version};
    use crate::domain::action::resolved::{Commit, ResolvedRef};
    use crate::domain::action::spec::Spec;
    use crate::domain::action::specifier::Specifier;
    use crate::domain::action::uses_ref::RefType;
    use crate::domain::lock::Lock;
    use crate::infra::github::Advisory;

    const SHA: &str = "abc123def456789012345678901234567890abcd";

    fn advisory(package: &str, range: &str) -> Advisory {
        Advisory {
            package: package.to_owned(),
            ghsa_id: "GHSA-mrrh-fwg8-r2c3".to_owned(),
            summary: "leaks secrets".to_owned(),
            severity: AdvisorySeverity::High,
            permalink: "https://github.com/advisories/GHSA-mrrh-fwg8-r2c3".to_owned(),
            vulnerable_range: range.to_owned(),
            first_patched: Some("46.0.1".to_owned()),
        }
    }

    fn lock_with(action: &str, label: &str, ref_type: RefType) -> Lock {
        let mut lock = Lock::default();
        let spec = Spec::new(ActionId::from(action), Specifier::parse(label));
        lock.set(
            &spec,
            ResolvedRef::from_stored(Version::from(label), Some(&ref_type)),
            Commit {
                sha: CommitSha::from(SHA),
                repository: Repository::from(action),
                ref_type: Some(ref_type),
                date: CommitDate::from("2026-01-01T00:00:00Z"),
            },
        );
        lock
    }

    fn findings_for(lock: &Lock, advisories: &[Advisory]) -> Vec<crate::audit::report::Finding> {
        let index = Advisories::index(advisories);
        targets(lock)
            .iter()
            .flat_map(|target| index.known_vulnerabilities(target))
            .collect()
    }

    // ---- Range matching ----

    #[test]
    fn boundaries_of_every_live_range_form_are_matched_on_both_sides() {
        // Every range string is one harvested from the live advisory data, and each is
        // asserted on both sides of its boundary: a comparator that matched everything
        // and one that matched nothing would each fail half this table.
        let table = [
            ("< 46.0.1", "46.0.0", Match::Affected),
            ("< 46.0.1", "46.0.1", Match::Unaffected),
            ("< 46.0.1", "46.0.2", Match::Unaffected),
            (">= 2.25.0, < 2.37.1", "2.25.0", Match::Affected),
            (">= 2.25.0, < 2.37.1", "2.37.0", Match::Affected),
            (">= 2.25.0, < 2.37.1", "2.24.9", Match::Unaffected),
            (">= 2.25.0, < 2.37.1", "2.37.1", Match::Unaffected),
            ("<= 0.24.0", "0.24.0", Match::Affected),
            ("<= 0.24.0", "0.24.1", Match::Unaffected),
            ("<= 45.0.7", "45.0.7", Match::Affected),
            ("<= 45.0.7", "46.0.1", Match::Unaffected),
            ("< 41", "40.0.0", Match::Affected),
            ("< 41", "41.0.0", Match::Unaffected),
            (">= 87, < 90", "88.0.0", Match::Affected),
            (">= 87, < 90", "90.0.0", Match::Unaffected),
            (">= 87, < 90", "86.0.0", Match::Unaffected),
        ];
        for (range, version, expected) in table {
            assert_eq!(
                matches(version, range),
                expected,
                "{version} against {range}"
            );
        }
    }

    #[test]
    fn a_v_prefix_never_changes_the_verdict() {
        // The bug this guards against is a prefix mismatch reading as "not affected".
        assert_eq!(matches("v45.0.7", "<= 45.0.7"), Match::Affected);
        assert_eq!(
            matches("45.0.7", "<= 45.0.7"),
            matches("v45.0.7", "<= 45.0.7")
        );
        assert_eq!(matches("v40", "< 41"), Match::Affected);
        assert_eq!(matches("v41", "< 41"), Match::Unaffected);
        assert_eq!(matches("40", "< 41"), matches("v40", "< 41"));
    }

    #[test]
    fn a_partial_version_below_a_lower_bound_in_its_own_line_is_undetermined() {
        // The false-clean this check exists to prevent. `v2` padded to 2.0.0 reads as
        // outside `>= 2.5.0`, but the tag points at the 2 line's head, which may be 2.7.x
        // and genuinely affected. Reporting Unaffected here would be a silent lie.
        assert_eq!(matches("v2", ">= 2.5.0, < 3.0.0"), Match::Undetermined);
        assert_eq!(matches("2", ">= 2.5.0, < 3.0.0"), Match::Undetermined);
        // Minor precision straddles only when the bound shares its minor.
        assert_eq!(matches("v2.5", ">= 2.5.3, < 3.0.0"), Match::Undetermined);
        assert_eq!(matches("v2.4", ">= 2.5.3, < 3.0.0"), Match::Unaffected);
    }

    #[test]
    fn an_exact_version_is_placed_rather_than_left_undetermined() {
        // The narrowing that keeps the rule above from swallowing every verdict: a fully
        // specified version needs no guessing, so it is answered outright.
        assert_eq!(matches("v2.0.0", ">= 2.5.0, < 3.0.0"), Match::Unaffected);
        assert_eq!(matches("v2.7.0", ">= 2.5.0, < 3.0.0"), Match::Affected);
    }

    #[test]
    fn a_lower_bound_outside_the_locked_line_still_decides() {
        // Padding is only ambiguous within the tag's own line. A `v2` tag cannot reach 3.x
        // however far the line has advanced, so this is answerable.
        assert_eq!(matches("v2", ">= 3.0.0"), Match::Unaffected);
    }

    #[test]
    fn an_unparseable_version_or_range_is_undetermined_not_clean() {
        assert_eq!(matches("main", "< 46.0.1"), Match::Undetermined);
        assert_eq!(matches(SHA, "< 46.0.1"), Match::Undetermined);
        assert_eq!(matches("45.0.7", "not a range"), Match::Undetermined);
    }

    // ---- Check behavior ----

    #[test]
    fn a_pin_inside_a_range_is_one_error_finding_naming_the_advisory() {
        let lock = lock_with("tj-actions/changed-files", "v45.0.7", RefType::Tag);
        let found = findings_for(&lock, &[advisory("tj-actions/changed-files", "<= 45.0.7")]);

        assert_eq!(found.len(), 1);
        assert_eq!(found[0].level, Level::Error);
        let message = &found[0].message;
        for element in [
            "tj-actions/changed-files",
            "45.0.7",
            "GHSA-mrrh-fwg8-r2c3",
            "high",
            "<= 45.0.7",
            "46.0.1",
            "https://github.com/advisories/GHSA-mrrh-fwg8-r2c3",
        ] {
            assert!(
                message.contains(element),
                "finding must carry {element}: {message}"
            );
        }
    }

    #[test]
    fn a_pin_outside_every_range_yields_nothing_while_an_affected_one_still_reports() {
        // Paired so an inert check cannot satisfy both halves.
        let advisories = [advisory("tj-actions/changed-files", "<= 45.0.7")];

        let safe = lock_with("tj-actions/changed-files", "v46.0.1", RefType::Tag);
        assert!(findings_for(&safe, &advisories).is_empty());

        let vulnerable = lock_with("tj-actions/changed-files", "v45.0.7", RefType::Tag);
        assert_eq!(findings_for(&vulnerable, &advisories).len(), 1);
    }

    #[test]
    fn an_advisory_for_an_unlocked_package_yields_nothing_while_a_locked_one_reports() {
        let advisories = [
            advisory("some/unlocked-action", "<= 99.0.0"),
            advisory("tj-actions/changed-files", "<= 45.0.7"),
        ];
        let lock = lock_with("tj-actions/changed-files", "v45.0.7", RefType::Tag);

        let found = findings_for(&lock, &advisories);

        // Exactly one: the unlocked package contributed nothing despite covering v45.0.7.
        assert_eq!(found.len(), 1);
        assert!(found[0].message.contains("tj-actions/changed-files"));
    }

    #[test]
    fn a_mixed_case_package_name_still_matches() {
        // GitHub slugs are case-insensitive and published names are mixed case, so a
        // literal comparison would miss a real advisory.
        let lock = lock_with("azure/setup-kubectl", "v3.0.0", RefType::Tag);
        let found = findings_for(&lock, &[advisory("Azure/setup-kubectl", "<= 3.0.0")]);

        assert_eq!(found.len(), 1);
    }

    #[test]
    fn a_nested_path_action_matches_on_its_repository() {
        // The action id carries the subpath; the advisory is published against the repo.
        let mut lock = Lock::default();
        let spec = Spec::new(
            ActionId::from("github/codeql-action/upload-sarif"),
            Specifier::parse("v2"),
        );
        lock.set(
            &spec,
            ResolvedRef::from_stored(Version::from("v2.0.0"), Some(&RefType::Tag)),
            Commit {
                sha: CommitSha::from(SHA),
                repository: Repository::from("github/codeql-action"),
                ref_type: Some(RefType::Tag),
                date: CommitDate::from("2026-01-01T00:00:00Z"),
            },
        );

        let found = findings_for(&lock, &[advisory("github/codeql-action", "<= 2.0.0")]);

        assert_eq!(
            found.len(),
            1,
            "matching on the id alone would find nothing"
        );
    }

    #[test]
    fn a_branch_pin_with_advisories_warns_rather_than_going_silent() {
        let lock = lock_with("tj-actions/changed-files", "main", RefType::Branch);
        let found = findings_for(&lock, &[advisory("tj-actions/changed-files", "<= 45.0.7")]);

        assert_eq!(found.len(), 1);
        assert_eq!(found[0].level, Level::Warn);
        assert!(
            found[0].message.contains("cannot tell"),
            "the finding must say what is unknown: {}",
            found[0].message
        );
    }

    #[test]
    fn a_branch_pin_without_advisories_yields_nothing_while_one_with_them_warns() {
        // The narrowing: an imprecise version is only interesting when something is
        // published against the action. Paired so an inert check fails one half.
        let lock = lock_with("actions/checkout", "main", RefType::Branch);
        assert!(findings_for(&lock, &[advisory("some/other-action", "< 1.0.0")]).is_empty());

        assert_eq!(
            findings_for(&lock, &[advisory("actions/checkout", "< 1.0.0")]).len(),
            1
        );
    }

    #[test]
    fn an_advisory_with_no_fix_is_still_reported() {
        let mut without_fix = advisory("tj-actions/changed-files", "<= 45.0.7");
        without_fix.first_patched = None;
        let lock = lock_with("tj-actions/changed-files", "v45.0.7", RefType::Tag);

        let found = findings_for(&lock, &[without_fix]);

        assert_eq!(found.len(), 1);
        assert_eq!(found[0].level, Level::Error);
        assert!(found[0].message.contains("GHSA-mrrh-fwg8-r2c3"));
        assert!(
            found[0].message.contains("no fixed version"),
            "the absence of a fix must be stated, not omitted: {}",
            found[0].message
        );
    }

    #[test]
    fn a_non_slug_package_name_is_indexed_without_matching_anything() {
        // One live advisory is published against a URL rather than a slug. It must not
        // poison the index or match a locked action.
        let odd = advisory(
            "https://github.com/pytorch/pytorch/.github/actions/filter-test-configs",
            "<= 99.0.0",
        );
        let lock = lock_with("pytorch/pytorch", "v1.0.0", RefType::Tag);

        assert!(findings_for(&lock, &[odd]).is_empty());
    }

    #[test]
    fn an_action_with_no_recorded_repository_is_not_looked_up() {
        // A row that stored no repository cannot be keyed on one; reporting it against
        // every advisory would be worse than the silence.
        let target = AuditTarget {
            id: &ActionId::from("actions/checkout"),
            version: "v4.2.1",
            sha: &CommitSha::from(SHA),
            ref_type: None,
            repository: None,
        };
        let advisories = [advisory("actions/checkout", "<= 5.0.0")];

        assert!(
            Advisories::index(&advisories)
                .known_vulnerabilities(&target)
                .is_empty()
        );
    }
}
