#![expect(clippy::pub_use, reason = "reexport from extracted submodule")]

//! `gx audit` — checks the actions recorded in `gx.lock` against knowledge that changes
//! without the repository changing: today, whether a pin is mutable; next, security
//! advisories and upstream repository state, which is why the command requires a token
//! even though the check it currently ships needs no network.
//!
//! Separate from `gx lint` because the two answer different questions. Lint judges *your
//! code* against rules you own: offline, deterministic, and its verdict changes only when
//! you edit a file. Audit judges *the world's knowledge about your dependencies*, so the
//! same commit is clean today and critical tomorrow. Folding networked checks into lint
//! would break a hermeticity users reasonably rely on.
//!
//! Audit reads `gx.lock` and never walks workflow files — see [`target`] for why.

/// The audit check identity, built from `rule_ids!`.
mod check_name;
/// Findings, report rendering, and the `--json` contract.
mod report;
/// The per-lock-row view checks consume, and the `mutable-ref` check.
mod target;

pub use check_name::CheckName;
pub use report::{Finding, Report};

use crate::command::Command;
use crate::config::Config;
use std::path::Path;
use thiserror::Error;

/// Errors that can occur during the audit command.
#[derive(Debug, Error)]
pub enum Error {
    /// No GitHub token was available.
    ///
    /// Unlike gx's REST paths, which fall back to unauthenticated requests, audit refuses
    /// to run: GitHub's GraphQL endpoint rejects unauthenticated requests, so the only
    /// reachable degraded behavior would be reporting "clean" without having checked
    /// anything. For a security command that is worse than not running at all.
    #[error(
        "gx audit requires a GitHub token, but GITHUB_TOKEN is not set.\n\
         Set it and run again, e.g. `GITHUB_TOKEN=$(gh auth token) gx audit`.\n\
         In GitHub Actions, pass `GITHUB_TOKEN: ${{{{ secrets.GITHUB_TOKEN }}}}` under `env:`.\n\
         Refusing to continue: an audit without a token could only report a false \"clean\"."
    )]
    MissingToken,
}

/// Run every check over the locked action set.
///
/// Reads only the lock — no network — so it is exercised in tests with a fixture lock and
/// a no-op progress callback.
#[must_use]
pub fn collect_findings(config: &Config, on_progress: &mut dyn FnMut(&str)) -> Vec<Finding> {
    on_progress("Auditing locked actions...");
    let targets = target::targets(&config.lock);
    // Each check is one line here and one function in `target.rs`, so checks developed in
    // parallel do not collide.
    targets.iter().filter_map(target::mutable_ref).collect()
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
        // Checked before anything else — before the lock is read and before any check
        // runs — so there is no path on which audit does partial work and returns a
        // report that reads as clean. The failure is an `Err`, structurally distinct
        // from a `Report` with zero findings.
        if config.settings.github_token.is_none() {
            return Err(Error::MissingToken);
        }

        Ok(Report::from_diagnostics(collect_findings(
            &config,
            on_progress,
        )))
    }
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    reason = "tests use unwrap, indexing, and other patterns freely"
)]
mod tests {
    use super::{Audit, Command as _, Error};
    use crate::config::{Config, GitHubToken, Settings};
    use crate::domain::action::identity::{ActionId, CommitDate, CommitSha, Repository, Version};
    use crate::domain::action::resolved::{Commit, ResolvedRef};
    use crate::domain::action::spec::Spec;
    use crate::domain::action::specifier::Specifier;
    use crate::domain::action::uses_ref::RefType;
    use crate::domain::lock::Lock;
    use crate::domain::manifest::Manifest;
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

    #[test]
    fn missing_token_is_an_error_not_a_clean_report() {
        let config = config_with(Lock::default(), None);
        let result = Audit.run(Path::new("/nonexistent"), config, &mut |_| {});

        // Structurally an Err, so it cannot be rendered or serialized as "clean".
        assert!(matches!(result, Err(Error::MissingToken)));
    }

    #[test]
    fn missing_token_message_names_the_variable_and_a_fix() {
        let message = Error::MissingToken.to_string();
        assert!(message.contains("GITHUB_TOKEN"), "got: {message}");
        assert!(message.contains("gh auth token"), "got: {message}");
    }

    #[test]
    fn token_guard_precedes_the_lock_read() {
        // A lock that would produce a finding still yields MissingToken, proving the
        // guard runs first rather than after a partial audit.
        let config = config_with(branch_lock(), None);
        let result = Audit.run(Path::new("/nonexistent"), config, &mut |_| {});
        assert!(matches!(result, Err(Error::MissingToken)));
    }

    #[test]
    fn branch_entry_produces_a_finding() {
        let config = config_with(branch_lock(), Some("token"));
        let report = Audit
            .run(Path::new("/nonexistent"), config, &mut |_| {})
            .unwrap();

        assert_eq!(report.diagnostics.len(), 1);
        assert_eq!(report.warning_count, 1);
        assert_eq!(report.error_count, 0);
    }

    #[test]
    fn empty_lock_is_clean() {
        let config = config_with(Lock::default(), Some("token"));
        let report = Audit
            .run(Path::new("/nonexistent"), config, &mut |_| {})
            .unwrap();

        assert!(report.diagnostics.is_empty());
    }
}
