#![expect(clippy::pub_use, reason = "reexport from extracted submodule")]

//! `gx audit` — checks `gx.lock` against knowledge that changes without the repository
//! changing, including published security advisories and archived repositories.
//!
//! That is the line against `gx lint`, which is offline and whose verdict changes only when
//! you edit a file. Audit requires a token for GitHub's advisory API.

/// The `known-vulnerability` check and package matching.
mod advisory_check;
/// Conservative local matching of advisory ranges.
mod advisory_range;
/// The `archived-action` check.
mod archived;
/// The audit check identity, built from `rule_ids!`.
mod check_name;
/// Findings, report rendering, and the `--json` contract.
mod report;
/// The `tag-moved` and `tag-unverified` checks.
mod tag_moved;
/// The per-lock-row view checks consume, and the `mutable-ref` check.
mod target;

pub use check_name::CheckName;
pub use report::{Finding, Report};

use crate::command::Command;
use crate::config::{Config, Settings};
use crate::domain::resolution::Forge;
use crate::infra::github::{
    AdvisoryQuery, Error as GitHubError, GitTags, GraphQlAdvisories, Registry, RepoMetadata,
    RestRepoMetadata, TagResolver,
};
use std::path::Path;
use thiserror::Error;

/// Errors that can occur during the audit command.
#[derive(Debug, Error)]
pub enum Error {
    /// No GitHub token was available. Audit requires authenticated upstream lookups
    /// rather than falling back to an incomplete offline report.
    #[error(
        "gx audit requires a {forge} token, but {var} is not set.\n\
         Set it and run again, e.g. `{var}=$(gh auth token) gx audit`.\n\
         In GitHub Actions, pass `{var}: ${{{{ secrets.GITHUB_TOKEN }}}}` under `env:`.\n\
         Refusing to continue: an audit without a token could only report a false \"clean\".",
        var = forge.token_env()
    )]
    MissingToken {
        /// The forge whose credential is missing.
        forge: Forge,
    },

    /// The advisory lookup did not succeed. Structurally an `Err`, so it can never be
    /// rendered, serialized, or exit-coded as a clean report — the audit did not happen.
    #[error(
        "gx audit could not reach the GitHub Advisory Database: {source}\n\
         Refusing to continue: an unchecked run could only report a false \"clean\"."
    )]
    Advisories {
        /// What the lookup failed with.
        #[source]
        source: GitHubError,
    },

    /// The HTTP client could not be built, so no check could issue a request. A
    /// precondition like the missing token, not a per-action failure.
    #[error("could not reach {forge}")]
    Registry {
        /// The forge whose client could not be built.
        forge: Forge,
        /// What went wrong building it.
        #[source]
        source: crate::infra::github::Error,
    },
}

/// Every offline check audit runs. Adding one is a line here.
const CHECKS: &[fn(&target::AuditTarget<'_>) -> Option<Finding>] = &[target::mutable_ref];

/// Run every check over the locked action set.
///
/// Takes both upstream sources as seams so tests drive the whole command with canned
/// data and no network.
///
/// # Errors
///
/// Returns [`Error::Advisories`] when the advisory lookup fails, so a run that could not
/// check never becomes a report that says it did.
fn collect_findings(
    config: &Config,
    advisories: &dyn AdvisoryQuery,
    repos: &dyn RepoMetadata,
    tags: &dyn TagResolver,
    on_progress: &mut dyn FnMut(&str),
) -> Result<Vec<Finding>, Error> {
    on_progress("Auditing locked actions...");
    let targets = target::targets(&config.lock);

    // Nothing locked means nothing to check, so the query is skipped entirely rather than
    // letting an empty lock fail for a network reason.
    if targets.is_empty() {
        return Ok(Vec::new());
    }

    on_progress("Fetching security advisories...");
    let published = advisories
        .all_actions_advisories()
        .map_err(|source| Error::Advisories { source })?;
    let index = advisory_check::Advisories::index(&published);

    Ok(targets
        .iter()
        .flat_map(|target| {
            CHECKS
                .iter()
                .filter_map(move |check| check(target))
                .chain(index.known_vulnerabilities(target))
                // A per-action metadata failure is a finding, not a short circuit:
                // retain this action's advisories and every other action's findings.
                .chain(archived::archived_action(target, repos))
                .chain(tag_moved::tag_moved(target, tags))
        })
        .collect())
}

/// Fail before any check runs, so no run reports findings it could not have gathered.
///
/// Public because the token requirement is a property of the command, not of one code path
/// through it: a run that never reaches [`Audit::run`] — no `.github` folder, so no repo
/// root — must still refuse rather than emit an empty report that reads as "clean".
///
/// # Errors
///
/// Returns [`Error::MissingToken`] when no forge credential is configured.
pub fn require_token(settings: &Settings) -> Result<(), Error> {
    if settings.github_token.is_none() {
        return Err(Error::MissingToken {
            forge: Forge::GitHub,
        });
    }
    Ok(())
}

/// The audit command.
pub struct Audit<A, R, T> {
    /// Advisory source shared by every check in a run.
    advisories: A,
    /// Repository metadata source shared by every check in a run.
    repos: R,
    /// Resolves each pinned tag to the commit it points at now.
    tags: T,
}

impl<A, R, T> Audit<A, R, T> {
    /// Construct a command with advisory, repository metadata, and tag sources.
    pub const fn new(advisories: A, repos: R, tags: T) -> Self {
        Self {
            advisories,
            repos,
            tags,
        }
    }
}

impl Audit<GraphQlAdvisories, RestRepoMetadata, GitTags> {
    /// Compose the production advisory adapter from configured credentials.
    ///
    /// # Errors
    ///
    /// Returns an error when credentials are absent or the HTTP client cannot be built.
    pub fn from_settings(settings: &Settings) -> Result<Self, Error> {
        require_token(settings)?;
        let registry =
            Registry::new(settings.github_token.clone()).map_err(|source| Error::Registry {
                forge: Forge::GitHub,
                source,
            })?;
        Ok(Self::new(
            GraphQlAdvisories::new(registry.clone()),
            RestRepoMetadata::new(registry.clone()),
            GitTags::new(registry),
        ))
    }
}

impl<A: AdvisoryQuery, R: RepoMetadata, T: TagResolver> Command for Audit<A, R, T> {
    type Report = Report;
    type Error = Error;

    fn run(
        &self,
        _repo_root: &Path,
        config: Config,
        on_progress: &mut dyn FnMut(&str),
    ) -> Result<Report, Error> {
        require_token(&config.settings)?;
        Ok(Report::from_diagnostics(collect_findings(
            &config,
            &self.advisories,
            &self.repos,
            &self.tags,
            on_progress,
        )?))
    }
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    reason = "tests require successful canned command runs"
)]
mod tests {
    use super::{Audit, CheckName, Error, Forge, Report};
    use crate::command::{Command as _, CommandReport as _};
    use crate::config::{Config, GitHubToken, Level, Settings};
    use crate::domain::action::identity::{ActionId, CommitDate, CommitSha, Repository, Version};
    use crate::domain::action::resolved::{Commit, ResolvedRef};
    use crate::domain::action::spec::Spec;
    use crate::domain::action::specifier::Specifier;
    use crate::domain::action::uses_ref::RefType;
    use crate::domain::lock::Lock;
    use crate::domain::manifest::Manifest;
    use crate::infra::github::{Advisory, AdvisorySeverity, CannedRepos, FakeAdvisories, FakeTags};
    use crate::output::lines::Line;
    use std::path::PathBuf;

    const SHA: &str = "abc123def456789012345678901234567890abcd";

    /// The command with every pinned tag still where the lock recorded it.
    fn audit_of(
        advisories: FakeAdvisories,
        repos: CannedRepos,
    ) -> Audit<FakeAdvisories, CannedRepos, FakeTags> {
        Audit::new(advisories, repos, FakeTags::resolving(SHA))
    }

    /// Run `audit` over `lock` with a token present.
    fn run(
        audit: &Audit<FakeAdvisories, CannedRepos, FakeTags>,
        lock: Lock,
    ) -> Result<Report, Error> {
        audit.run(
            std::path::Path::new("."),
            config_with(lock, Some("token")),
            &mut |_| {},
        )
    }

    fn config_with(lock: Lock, token: Option<&str>) -> Config {
        Config {
            settings: Settings {
                github_token: token.map(|t| GitHubToken::from(t.to_owned())),
            },
            manifest: Manifest::default(),
            lock,
            lint_config: crate::config::Lint::default(),
            manifest_path: PathBuf::from("gx.toml"),
            lock_path: PathBuf::from("gx.lock"),
            manifest_migrated: false,
        }
    }

    fn branch_lock() -> Lock {
        let mut lock = Lock::default();
        let spec = Spec::new(ActionId::from("actions/checkout"), Specifier::parse("main"));
        lock.set(
            &spec,
            ResolvedRef::from_stored(Version::from("main"), Some(&RefType::Branch)),
            Commit {
                sha: CommitSha::from("abc123def456789012345678901234567890abcd"),
                repository: Repository::from("actions/checkout"),
                ref_type: Some(RefType::Branch),
                date: CommitDate::from("2026-01-01T00:00:00Z"),
            },
        );
        lock
    }

    fn two_entry_lock() -> Lock {
        let mut lock = branch_lock();
        let spec = Spec::new(ActionId::from("actions/setup-node"), Specifier::parse("^4"));
        lock.set(
            &spec,
            ResolvedRef::from_stored(Version::from("v4.0.0"), Some(&RefType::Tag)),
            Commit {
                sha: CommitSha::from("abc123def456789012345678901234567890abcd"),
                repository: Repository::from("actions/setup-node"),
                ref_type: Some(RefType::Tag),
                date: CommitDate::from("2026-01-01T00:00:00Z"),
            },
        );
        lock
    }

    #[test]
    fn missing_token_is_an_error_not_a_clean_report() {
        let config = config_with(Lock::default(), None);
        let result = audit_of(FakeAdvisories::new(Vec::new()), CannedRepos::active()).run(
            std::path::Path::new("."),
            config,
            &mut |_| {},
        );

        // Structurally an Err, so it cannot be rendered or serialized as "clean".
        assert!(matches!(result, Err(Error::MissingToken { .. })));
    }

    #[test]
    fn missing_token_message_names_the_variable_and_a_fix() {
        let message = Error::MissingToken {
            forge: Forge::GitHub,
        }
        .to_string();
        // Asserted against `token_env()` rather than a literal, so the message and the
        // forge's own notion of its credential variable cannot drift apart.
        assert!(
            message.contains(Forge::GitHub.token_env()),
            "got: {message}"
        );
        assert!(message.contains("gh auth token"), "got: {message}");
        assert!(message.contains("false \"clean\""), "got: {message}");
    }

    #[test]
    fn token_guard_precedes_the_lock_read() {
        // A lock that would produce a finding still yields MissingToken, proving the
        // guard runs first rather than after a partial audit.
        let config = config_with(branch_lock(), None);
        let audit = audit_of(FakeAdvisories::failing(), CannedRepos::failing());
        let result = audit.run(std::path::Path::new("."), config, &mut |_| {});
        assert!(matches!(result, Err(Error::MissingToken { .. })));
        // The guard ran before the lookup, so no query was even attempted.
        assert_eq!(audit.advisories.calls(), 0);
        assert!(audit.repos.seen.borrow().is_empty());
    }

    /// Drive the real command with both upstream sources injected.
    fn report_for(lock: Lock, repos: CannedRepos) -> Report {
        run(&audit_of(FakeAdvisories::new(Vec::new()), repos), lock).unwrap()
    }

    #[test]
    fn branch_entry_produces_a_finding() {
        let report = report_for(branch_lock(), CannedRepos::active());
        assert_eq!(report.diagnostics.len(), 1);
        assert_eq!(report.warning_count, 1);
        assert_eq!(report.error_count, 0);
    }

    #[test]
    fn vulnerable_and_clean_pins_set_the_command_exit_code() {
        for (version, expected_exit) in [("v4.0.0", 1), ("v5.0.0", 0)] {
            let mut lock = Lock::default();
            lock.set(
                &Spec::new(ActionId::from("actions/checkout"), Specifier::parse("v4")),
                ResolvedRef::from_stored(Version::from(version), Some(&RefType::Tag)),
                Commit {
                    sha: CommitSha::from("abc123def456789012345678901234567890abcd"),
                    repository: Repository::from("actions/checkout"),
                    ref_type: Some(RefType::Tag),
                    date: CommitDate::from("2026-01-01T00:00:00Z"),
                },
            );
            let audit = audit_of(
                FakeAdvisories::new(vec![Advisory {
                    package: "actions/checkout".to_owned(),
                    ghsa_id: "GHSA-test".to_owned(),
                    summary: "test vulnerability".to_owned(),
                    severity: AdvisorySeverity::High,
                    permalink: "https://github.com/advisories/GHSA-test".to_owned(),
                    vulnerable_range: "< 5.0.0".to_owned(),
                    first_patched: Some("5.0.0".to_owned()),
                }]),
                CannedRepos::active(),
            );
            let report = run(&audit, lock).unwrap();
            assert_eq!(report.exit_code(), expected_exit, "{version}");
            assert_eq!(audit.advisories.calls(), 1);
        }
    }

    #[test]
    fn empty_lock_is_clean() {
        let report = report_for(Lock::default(), CannedRepos::active());
        assert!(report.diagnostics.is_empty());
    }

    #[test]
    fn empty_lock_is_clean_and_issues_no_query() {
        // Nothing to check must not fail for a network reason.
        let audit = audit_of(FakeAdvisories::failing(), CannedRepos::failing());

        let report = run(&audit, Lock::default()).unwrap();

        assert!(report.diagnostics.is_empty());
        assert_eq!(audit.advisories.calls(), 0);
        assert!(audit.repos.seen.borrow().is_empty());
    }

    #[test]
    fn a_failed_lookup_is_an_error_not_a_clean_report() {
        // The distinction the whole command turns on: "could not check" is a different
        // type from "checked and clean", so it can never be rendered or serialized.
        let audit = audit_of(FakeAdvisories::failing(), CannedRepos::active());

        let result = run(&audit, branch_lock());

        assert!(matches!(result, Err(Error::Advisories { .. })));
    }

    #[test]
    fn a_multi_action_lock_issues_exactly_one_query() {
        // The point of fetching wholesale: request count does not scale with lock size.
        let mut lock = branch_lock();
        for action in [
            "actions/setup-node",
            "actions/cache",
            "actions/upload-artifact",
        ] {
            let spec = Spec::new(ActionId::from(action), Specifier::parse("v4"));
            lock.set(
                &spec,
                ResolvedRef::from_stored(Version::from("v4.0.0"), Some(&RefType::Tag)),
                Commit {
                    sha: CommitSha::from("abc123def456789012345678901234567890abcd"),
                    repository: Repository::from(action),
                    ref_type: Some(RefType::Tag),
                    date: CommitDate::from("2026-01-01T00:00:00Z"),
                },
            );
        }
        let audit = audit_of(FakeAdvisories::new(Vec::new()), CannedRepos::active());

        run(&audit, lock).unwrap();

        assert_eq!(audit.advisories.calls(), 1);
    }

    #[test]
    #[expect(
        clippy::expect_used,
        clippy::indexing_slicing,
        reason = "tests inspect known report and JSON positions"
    )]
    fn every_check_runs_over_every_entry() {
        // One entry, both checks firing: the offline check and the networked one are
        // combined rather than one replacing the other.
        let report = report_for(branch_lock(), CannedRepos::archived("2021-04-14T00:00:00Z"));

        assert_eq!(report.diagnostics.len(), 2);
        assert_eq!(report.warning_count, 2);
        assert_eq!(report.error_count, 0);
        assert_eq!(report.exit_code(), 0);
        assert_eq!(report.diagnostics[0].rule, CheckName::MutableRef);
        assert_eq!(report.diagnostics[1].rule, CheckName::ArchivedAction);
        assert!(
            report.render().iter().any(|line| matches!(
                line,
                Line::LintDiag { rule, level, .. }
                    if rule == "archived-action" && *level == Level::Warn
            )),
            "human output must name the archived-action check"
        );
        let json: serde_json::Value =
            serde_json::from_str(&report.to_json()).expect("report must serialize");
        assert_eq!(json["findings"][1]["check"], "archived-action");
        assert_eq!(json["findings"][1]["level"], "warn");
    }

    #[test]
    fn a_failed_lookup_does_not_suppress_other_entries() {
        // Targets sort by action id, so changing which repository fails covers both
        // failure-before-warning and warning-before-failure without relying on map order.
        for (failed_repo, archived_repo) in [
            ("actions/checkout", "actions/setup-node"),
            ("actions/setup-node", "actions/checkout"),
        ] {
            let source = CannedRepos::archived("2021-04-14T00:00:00Z").with_failure(failed_repo);
            let audit = audit_of(FakeAdvisories::new(Vec::new()), source);
            let report = run(&audit, two_entry_lock()).unwrap();

            assert_eq!(report.error_count, 1, "failed repo: {failed_repo}");
            assert_eq!(report.warning_count, 2, "failed repo: {failed_repo}");
            assert_eq!(report.diagnostics.len(), 3);
            assert_eq!(report.exit_code(), 1);
            let archived_findings: Vec<_> = report
                .diagnostics
                .iter()
                .filter(|finding| finding.rule == CheckName::ArchivedAction)
                .collect();
            assert_eq!(archived_findings.len(), 2);
            assert!(archived_findings.iter().any(|finding| {
                finding.level == Level::Error
                    && finding.message.contains(failed_repo)
                    && finding.message.contains("could not determine")
            }));
            assert!(archived_findings.iter().any(|finding| {
                finding.level == Level::Warn
                    && finding.message.contains(archived_repo)
                    && finding.message.contains("archived repository")
            }));
            assert_eq!(
                audit.repos.seen.borrow().as_slice(),
                ["actions/checkout", "actions/setup-node"]
            );
        }
    }

    #[test]
    fn archived_advisory_and_mutable_findings_survive_a_metadata_failure() {
        let audit = audit_of(
            FakeAdvisories::new(vec![Advisory {
                package: "actions/setup-node".to_owned(),
                ghsa_id: "GHSA-test".to_owned(),
                summary: "test vulnerability".to_owned(),
                severity: AdvisorySeverity::High,
                permalink: "https://github.com/advisories/GHSA-test".to_owned(),
                vulnerable_range: "< 5.0.0".to_owned(),
                first_patched: Some("5.0.0".to_owned()),
            }]),
            CannedRepos::archived("2021-04-14T00:00:00Z").with_failure("actions/setup-node"),
        );
        let report = run(&audit, two_entry_lock()).unwrap();

        assert_eq!(report.diagnostics.len(), 4);
        assert_eq!(report.warning_count, 2);
        assert_eq!(report.error_count, 2);
        assert_eq!(report.exit_code(), 1);
        for (rule, level) in [
            (CheckName::MutableRef, Level::Warn),
            (CheckName::ArchivedAction, Level::Warn),
            (CheckName::ArchivedAction, Level::Error),
            (CheckName::KnownVulnerability, Level::Error),
        ] {
            assert!(
                report
                    .diagnostics
                    .iter()
                    .any(|finding| { finding.rule == rule && finding.level == level }),
                "missing {rule} at {level:?}"
            );
        }
        assert_eq!(audit.advisories.calls(), 1);
        assert_eq!(
            audit.repos.seen.borrow().as_slice(),
            ["actions/checkout", "actions/setup-node"]
        );
    }
}
