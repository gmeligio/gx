#![expect(clippy::pub_use, reason = "reexport from extracted submodule")]

//! `gx audit` — checks `gx.lock` against knowledge that changes without the repository
//! changing, including published security advisories.
//!
//! That is the line against `gx lint`, which is offline and whose verdict changes only when
//! you edit a file. Audit requires a token for GitHub's advisory API.

/// The `known-vulnerability` check and package matching.
mod advisory_check;
/// Conservative local matching of advisory ranges.
mod advisory_range;
/// The audit check identity, built from `rule_ids!`.
mod check_name;
/// Findings, report rendering, and the `--json` contract.
mod report;
/// The per-lock-row view checks consume, and the `mutable-ref` check.
mod target;

pub use check_name::CheckName;
pub use report::{Finding, Report};

use crate::command::Command;
use crate::config::{Config, Settings};
use crate::domain::resolution::Forge;
use crate::infra::github::{AdvisoryQuery, Error as GitHubError, GraphQlAdvisories, Registry};
use std::path::Path;
use thiserror::Error;

/// Errors that can occur during the audit command.
#[derive(Debug, Error)]
pub enum Error {
    /// No GitHub token was available. The GraphQL endpoint rejects unauthenticated
    /// requests, so there is no degraded mode — only a false "clean".
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
}

/// Every offline check audit runs. Adding one is a line here.
const CHECKS: &[fn(&target::AuditTarget<'_>) -> Option<Finding>] = &[target::mutable_ref];

/// Run every check over the locked action set.
///
/// Takes the advisory source as a seam so tests drive the whole command with canned data
/// and no network.
///
/// # Errors
///
/// Returns [`Error::Advisories`] when the advisory lookup fails, so a run that could not
/// check never becomes a report that says it did.
fn collect_findings(
    config: &Config,
    advisories: &dyn AdvisoryQuery,
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
pub struct Audit<A> {
    /// Advisory source shared by every check in a run.
    advisories: A,
}

impl<A> Audit<A> {
    /// Construct a command with an advisory source.
    pub const fn new(advisories: A) -> Self {
        Self { advisories }
    }
}

impl Audit<GraphQlAdvisories> {
    /// Compose the production advisory adapter from configured credentials.
    ///
    /// # Errors
    ///
    /// Returns an error when credentials are absent or the HTTP client cannot be built.
    pub fn from_settings(settings: &Settings) -> Result<Self, Error> {
        require_token(settings)?;
        let registry = Registry::new(settings.github_token.clone())
            .map_err(|source| Error::Advisories { source })?;
        Ok(Self::new(GraphQlAdvisories::new(registry)))
    }
}

impl<A: AdvisoryQuery> Command for Audit<A> {
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
            on_progress,
        )?))
    }
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    reason = "tests use unwrap, indexing, and other patterns freely"
)]
mod tests {
    use super::{Audit, Error, Forge};
    use crate::command::Command as _;
    use crate::config::{Config, GitHubToken, Settings};
    use crate::domain::action::identity::{ActionId, CommitDate, CommitSha, Repository, Version};
    use crate::domain::action::resolved::{Commit, ResolvedRef};
    use crate::domain::action::spec::Spec;
    use crate::domain::action::specifier::Specifier;
    use crate::domain::action::uses_ref::RefType;
    use crate::domain::lock::Lock;
    use crate::domain::manifest::Manifest;
    use crate::infra::github::{Advisory, AdvisorySeverity, FakeAdvisories};
    use std::path::PathBuf;

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

    #[test]
    fn missing_token_is_an_error_not_a_clean_report() {
        let config = config_with(Lock::default(), None);
        let result = Audit::new(FakeAdvisories::new(Vec::new())).run(
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
        let audit = Audit::new(FakeAdvisories::failing());
        let result = audit.run(std::path::Path::new("."), config, &mut |_| {});
        assert!(matches!(result, Err(Error::MissingToken { .. })));
        // The guard ran before the lookup, so no query was even attempted.
        assert_eq!(audit.advisories.calls(), 0);
    }

    #[test]
    fn branch_entry_produces_a_finding() {
        let config = config_with(branch_lock(), Some("token"));
        let report = Audit::new(FakeAdvisories::new(Vec::new()))
            .run(std::path::Path::new("."), config, &mut |_| {})
            .unwrap();

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
            let audit = Audit::new(FakeAdvisories::new(vec![Advisory {
                package: "actions/checkout".to_owned(),
                ghsa_id: "GHSA-test".to_owned(),
                summary: "test vulnerability".to_owned(),
                severity: AdvisorySeverity::High,
                permalink: "https://github.com/advisories/GHSA-test".to_owned(),
                vulnerable_range: "< 5.0.0".to_owned(),
                first_patched: Some("5.0.0".to_owned()),
            }]));
            let report = audit
                .run(
                    std::path::Path::new("."),
                    config_with(lock, Some("token")),
                    &mut |_| {},
                )
                .unwrap();
            assert_eq!(report.exit_code(), expected_exit, "{version}");
            assert_eq!(audit.advisories.calls(), 1);
        }
    }

    #[test]
    fn empty_lock_is_clean_and_issues_no_query() {
        // Nothing to check must not fail for a network reason.
        let config = config_with(Lock::default(), Some("token"));
        let audit = Audit::new(FakeAdvisories::failing());

        let report = audit
            .run(std::path::Path::new("."), config, &mut |_| {})
            .unwrap();

        assert!(report.diagnostics.is_empty());
        assert_eq!(audit.advisories.calls(), 0);
    }

    #[test]
    fn a_failed_lookup_is_an_error_not_a_clean_report() {
        // The distinction the whole command turns on: "could not check" is a different
        // type from "checked and clean", so it can never be rendered or serialized.
        let config = config_with(branch_lock(), Some("token"));

        let result = Audit::new(FakeAdvisories::failing()).run(
            std::path::Path::new("."),
            config,
            &mut |_| {},
        );

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
        let audit = Audit::new(FakeAdvisories::new(Vec::new()));

        audit
            .run(
                std::path::Path::new("."),
                config_with(lock, Some("token")),
                &mut |_| {},
            )
            .unwrap();

        assert_eq!(audit.advisories.calls(), 1);
    }
}
