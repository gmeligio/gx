//! The `known-vulnerability` check: locked actions against the GitHub Advisory Database.
//!
//! Range matching happens here rather than server-side because OSV's `GitHub Actions`
//! version filter returns an empty result for known-vulnerable versions — a false "clean",
//! which for a security check is worse than not shipping the check at all. Every path that
//! could report "not affected" without having established it routes to
//! [`Match::Undetermined`] instead.

use super::advisory_range::{Match, matches};
use super::check_name::CheckName;
use super::report::Finding;
use super::target::AuditTarget;
use crate::config::Level;
use crate::infra::github::{Advisory, AdvisorySeverity};
use std::collections::HashMap;

/// A vulnerable pin fails the build: the action is running code a published advisory says
/// is exploitable.
const VULNERABLE_SEVERITY: Level = Level::Error;

/// An advisory gx could not place is a warning, not an error. The action may well be fine,
/// and failing a build on an unknown is the false alarm that costs a security tool its
/// credibility. Silence would be the opposite and worse failure.
const UNDETERMINED_SEVERITY: Level = Level::Warn;

/// How GitHub's severity reads in a finding.
const fn severity_label(severity: AdvisorySeverity) -> &'static str {
    match severity {
        AdvisorySeverity::Low => "low",
        AdvisorySeverity::Moderate => "moderate",
        AdvisorySeverity::High => "high",
        AdvisorySeverity::Critical => "critical",
    }
}

/// The advisory set, indexed by repository or scoped action identity.
/// Built once per run with case-insensitive repositories and case-sensitive subpaths.
pub struct Advisories<'set> {
    /// Normalized package identity to the advisories published against it.
    by_package: HashMap<String, Vec<&'set Advisory>>,
}

/// Normalize GitHub package identities without widening a nested action to its repository.
/// Repository names are case-insensitive; action subpaths are case-sensitive.
fn package_key(package: &str) -> Option<String> {
    let slug = if let Some((scheme, rest)) = package.split_once("://") {
        let (host, path) = rest.split_once('/')?;
        if !scheme.eq_ignore_ascii_case("https") || !host.eq_ignore_ascii_case("github.com") {
            return None;
        }
        path
    } else {
        package
    };
    let mut parts = slug.splitn(3, '/');
    let owner = parts.next()?;
    let repo = parts.next()?;
    if owner.is_empty() || repo.is_empty() {
        return None;
    }
    let mut key = format!("{}/{}", owner.to_lowercase(), repo.to_lowercase());
    if let Some(path) = parts.next().filter(|path| !path.is_empty()) {
        key.push('/');
        key.push_str(path);
    }
    Some(key)
}

impl<'set> Advisories<'set> {
    /// Index an advisory set for lookup.
    pub fn index(advisories: &'set [Advisory]) -> Self {
        let mut by_package: HashMap<String, Vec<&'set Advisory>> = HashMap::new();
        for advisory in advisories {
            if let Some(key) = package_key(&advisory.package) {
                by_package.entry(key).or_default().push(advisory);
            }
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
        let repo_key = package_key(repository.as_str());
        let action_key =
            package_key(target.id.as_str()).filter(|key| Some(key) != repo_key.as_ref());
        [repo_key, action_key]
            .into_iter()
            .flatten()
            .filter_map(|key| self.by_package.get(&key))
            .flatten()
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
    fn prereleases_are_not_silently_excluded_by_dependency_semantics() {
        assert_eq!(matches("v45.0.7-beta.1", "<=45.0.7"), Match::Undetermined);
        assert_eq!(
            matches("v45.0.7-beta.1", ">=45.0.7-beta.0"),
            Match::Affected
        );
    }

    #[test]
    fn partial_pins_can_overlap_equality_and_implicit_lower_bounds() {
        for (pin, range) in [
            ("v2", "=2.5.0"),
            ("v2", "=2.5"),
            ("v2.5", "=2.5.3"),
            ("v2", "2.5.0"),
            ("v2", "~2.5.0"),
        ] {
            assert_eq!(matches(pin, range), Match::Undetermined, "{pin}: {range}");
        }
        for (pin, range) in [
            ("v2", "=3.5.0"),
            ("v2.4", "=2.5.0"),
            ("v2.6", "=2.5.0"),
            ("v2", ">=2.5.0, <2.0.0"),
        ] {
            assert_eq!(matches(pin, range), Match::Unaffected, "{pin}: {range}");
        }
        assert_eq!(matches("v2", "<2.5.0"), Match::Affected);
    }

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
        // literal comparison would miss a real advisory. The two sides are cased
        // differently from each other and neither is already lowercase, so a match
        // requires normalizing both — dropping it on either side loses the finding.
        let lock = lock_with("AZURE/setup-kubectl", "v3.0.0", RefType::Tag);
        let found = findings_for(&lock, &[advisory("Azure/Setup-Kubectl", "<= 3.0.0")]);

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
    fn github_url_packages_preserve_nested_action_scope() {
        let action = "pytorch/pytorch/.github/actions/filter-test-configs";
        let odd = advisory(
            "https://github.com/pytorch/pytorch/.github/actions/filter-test-configs",
            "<2.0.1",
        );
        let id = ActionId::from(action);
        let repository = Repository::from("pytorch/pytorch");
        let sha = CommitSha::from(SHA);
        let mut target = AuditTarget {
            id: &id,
            version: "v2.0.0",
            sha: &sha,
            ref_type: None,
            repository: Some(&repository),
        };
        let advisories = [odd];
        let index = Advisories::index(&advisories);
        assert_eq!(index.known_vulnerabilities(&target).len(), 1);
        let mixed = [advisory(
            "https://GitHub.com/PyTorch/PyTorch/.github/actions/filter-test-configs",
            "<2.0.1",
        )];
        assert_eq!(
            Advisories::index(&mixed)
                .known_vulnerabilities(&target)
                .len(),
            1
        );
        let wrong_case = [advisory(
            "https://github.com/pytorch/pytorch/.github/actions/FILTER-test-configs",
            "<2.0.1",
        )];
        assert!(
            Advisories::index(&wrong_case)
                .known_vulnerabilities(&target)
                .is_empty()
        );
        let foreign = [advisory(&format!("https://example.com/{action}"), "<2.0.1")];
        assert!(
            Advisories::index(&foreign)
                .known_vulnerabilities(&target)
                .is_empty()
        );
        let other = ActionId::from("pytorch/pytorch/.github/actions/other");
        target.id = &other;
        assert!(index.known_vulnerabilities(&target).is_empty());
        let root = ActionId::from("pytorch/pytorch");
        target.id = &root;
        assert!(index.known_vulnerabilities(&target).is_empty());
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
