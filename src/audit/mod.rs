#![expect(clippy::pub_use, reason = "reexport from extracted submodule")]

//! `gx audit` — checks `gx.lock` against knowledge that changes without the repository
//! changing: today whether a pin is mutable, next security advisories.
//!
//! That is the line against `gx lint`, which is offline and whose verdict changes only when
//! you edit a file. The same commit is clean today and critical tomorrow here, so audit
//! requires a token even for the check it ships, which needs no network.

/// The `archived-action` check.
mod archived;
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
use crate::infra::github::{Registry, RepoMetadata, RestRepoMetadata};
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
const OFFLINE_CHECKS: &[fn(&target::AuditTarget<'_>) -> Option<Finding>] = &[target::mutable_ref];

/// Run every check over the locked action set.
///
/// Takes the repository-metadata source rather than building one, so tests drive the whole
/// pipeline with canned state and no network.
#[must_use]
pub fn collect_findings(
    config: &Config,
    repos: &dyn RepoMetadata,
    on_progress: &mut dyn FnMut(&str),
) -> Vec<Finding> {
    on_progress("Auditing locked actions...");
    target::targets(&config.lock)
        .iter()
        .flat_map(|target| {
            OFFLINE_CHECKS
                .iter()
                .filter_map(move |check| check(target))
                // Chained, not short-circuited: one action's failed lookup must not
                // discard the findings every other action produced.
                .chain(archived::archived_action(target, repos))
        })
        .collect()
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
pub struct Audit;

impl Command for Audit {
    type Report = Report;
    type Error = Error;

    fn run(
        &self,
        _repo_root: &Path,
        config: Config,
        on_progress: &mut dyn FnMut(&str),
    ) -> Result<Report, Error> {
        require_token(&config.settings)?;
        let registry = Registry::new(config.settings.github_token.clone()).map_err(|source| {
            Error::Registry {
                forge: Forge::GitHub,
                source,
            }
        })?;
        let repos = RestRepoMetadata::new(registry);
        Ok(Report::from_diagnostics(collect_findings(
            &config,
            &repos,
            on_progress,
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::{Audit, Command as _, Error, Forge, Report, collect_findings};
    use crate::config::{Config, GitHubToken, Level, Settings};
    use crate::domain::action::identity::{ActionId, CommitDate, CommitSha, Repository, Version};
    use crate::domain::action::resolved::{Commit, ResolvedRef};
    use crate::domain::action::spec::Spec;
    use crate::domain::action::specifier::Specifier;
    use crate::domain::action::uses_ref::RefType;
    use crate::domain::lock::Lock;
    use crate::domain::manifest::Manifest;
    use crate::infra::github::CannedRepos;
    use std::path::{Path, PathBuf};

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
        let result = Audit.run(Path::new("/nonexistent"), config, &mut |_| {});

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
        let result = Audit.run(Path::new("/nonexistent"), config, &mut |_| {});
        assert!(matches!(result, Err(Error::MissingToken { .. })));
    }

    /// Drives the whole check pipeline against canned repository state, so these tests
    /// cover what `Audit::run` does without the network `Audit::run` would reach for.
    fn report_for(lock: Lock, repos: &CannedRepos) -> Report {
        let config = config_with(lock, Some("token"));
        Report::from_diagnostics(collect_findings(&config, repos, &mut |_| {}))
    }

    #[test]
    fn branch_entry_produces_a_finding() {
        let report = report_for(branch_lock(), &CannedRepos::active());

        assert_eq!(report.diagnostics.len(), 1);
        assert_eq!(report.warning_count, 1);
        assert_eq!(report.error_count, 0);
    }

    #[test]
    fn empty_lock_is_clean() {
        let report = report_for(Lock::default(), &CannedRepos::active());

        assert!(report.diagnostics.is_empty());
    }

    #[test]
    fn every_check_runs_over_every_entry() {
        // One entry, both checks firing: the offline check and the networked one are
        // combined rather than one replacing the other.
        let report = report_for(
            branch_lock(),
            &CannedRepos::archived("2021-04-14T00:00:00Z"),
        );

        assert_eq!(report.diagnostics.len(), 2);
        assert_eq!(report.warning_count, 2);
    }

    #[test]
    fn a_failed_lookup_does_not_suppress_other_entries() {
        // Two entries, the lookup failing for both: the run reports on each rather than
        // stopping at the first failure. Order-independent by construction — every entry
        // is checked, so neither position can be the lucky one.
        let report = report_for(two_entry_lock(), &CannedRepos::failing());

        let errors: Vec<_> = report
            .diagnostics
            .iter()
            .filter(|finding| finding.level == Level::Error)
            .collect();
        assert_eq!(errors.len(), 2, "both entries must report: {errors:?}");
        assert!(
            errors
                .iter()
                .any(|f| f.message.contains("actions/checkout"))
        );
        assert!(
            errors
                .iter()
                .any(|f| f.message.contains("actions/setup-node"))
        );
    }
}
