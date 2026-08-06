use std::path::Path;

use super::cli::Request as UpgradeRequest;
use super::plan::{self, UpgradeError};
use super::report::{Report as UpgradeReport, UpgradeEntry};
use crate::command::Command;
use crate::config::Config;
use crate::domain::action::spec::Spec as ActionSpec;
use crate::domain::action::upgrade::{Action, Candidate as UpgradeCandidate};
use crate::domain::lock::Lock;
use crate::infra::github::Registry;
use crate::infra::lock::Error as LockFileError;
use crate::infra::manifest::Error as ManifestError;
use crate::infra::registry::caching_retrying;
use crate::infra::workflow_update::WorkflowWriter;
use thiserror::Error;

/// JSON for an up-to-date upgrade report, used when there is nothing to do
/// (e.g. no `.github` folder) but a `--json` consumer still needs a valid document.
#[must_use]
pub fn empty_json_report() -> String {
    UpgradeReport {
        up_to_date: true,
        ..Default::default()
    }
    .to_json()
}

/// Build a report entry for one upgrade candidate against the pre-upgrade lock.
///
/// `from` is the version resolved in the lock *before* this run — the version a
/// reviewer is actually moving away from — not the manifest range. When both the
/// old and new sides are real tags, a GitHub compare link is attached so the PR
/// body (via `--json`) and the log can show *why* the pin moved.
fn build_entry(candidate: &UpgradeCandidate, lock_before: &Lock) -> UpgradeEntry {
    let to = candidate.candidate();
    let in_range = matches!(candidate.action, Action::InRange { .. });

    // The pre-upgrade lock is keyed by the ORIGINAL specifier, so a cross-range
    // bump still looks up its old entry under `current`, not the new specifier.
    let key = ActionSpec::new(candidate.id.clone(), candidate.current.clone());
    let from_tag = lock_before
        .get(&key)
        .and_then(|entry| entry.reference.tag().cloned());

    // A compare link only makes sense between two real version tags; a branch or
    // bare-commit pin (no tag) yields no meaningful diff view.
    let compare = from_tag
        .as_ref()
        .map(|from| candidate.id.compare_url(from, to));

    let from = from_tag.map_or_else(|| candidate.current.to_string(), |v| v.to_string());

    UpgradeEntry {
        action: candidate.id.to_string(),
        from,
        to: to.to_string(),
        in_range,
        compare,
    }
}

/// Errors that can occur during the upgrade command's run phase (I/O + domain).
#[derive(Debug, Error)]
pub enum RunError {
    #[error(transparent)]
    Github(#[from] crate::infra::github::Error),
    #[error(transparent)]
    Manifest(#[from] ManifestError),
    #[error(transparent)]
    Lock(#[from] LockFileError),
    #[error(transparent)]
    Upgrade(#[from] UpgradeError),
}

/// The upgrade command struct.
pub struct Upgrade {
    pub request: UpgradeRequest,
}

impl Command for Upgrade {
    type Report = UpgradeReport;
    type Error = RunError;

    fn run(
        &self,
        repo_root: &Path,
        config: Config,
        on_progress: &mut dyn FnMut(&str),
    ) -> Result<UpgradeReport, RunError> {
        let has_manifest = config.manifest_path.exists();
        let github = Registry::new(config.settings.github_token)?;
        let updater = WorkflowWriter::new(repo_root);

        // Scoped so the registry's borrow of `on_progress` ends with planning,
        // leaving the callback free for the reporting below.
        let upgrade_plan = {
            // Cache outside retry, so a repeated query never reaches the retry
            // layer. Each wait is announced through the progress channel; in
            // `--json` mode that channel is already suppressed, so the single
            // JSON document on stdout stays intact.
            let (registry, progress) =
                caching_retrying(github, &mut *on_progress);
            plan::plan(
                &config.manifest,
                &config.lock,
                &registry,
                &self.request,
                progress,
            )?
        };

        if upgrade_plan.is_empty() {
            return Ok(UpgradeReport {
                up_to_date: true,
                ..Default::default()
            });
        }

        if has_manifest {
            crate::infra::manifest::patch::apply_manifest_diff(
                &config.manifest_path,
                &upgrade_plan.manifest,
            )?;
            let lock_store = crate::infra::lock::Store::new(&config.lock_path);
            lock_store.save(&upgrade_plan.lock)?;
        }

        let workflows_updated = plan::apply_upgrade_workflows(
            &updater,
            &upgrade_plan.lock_changes,
            &upgrade_plan.upgrades,
        )?;

        if config.manifest_migrated {
            on_progress("migrated gx.toml → semver specifiers");
        }

        let upgrades: Vec<UpgradeEntry> = upgrade_plan
            .upgrades
            .iter()
            .map(|u| build_entry(u, &config.lock))
            .collect();

        // The compare link is verbose, so it stays out of the terminal summary —
        // but it is recorded in the log file (and CI verbose output) for anyone
        // who wants to see *why* a pin moved. The `--json` output carries it too.
        for entry in &upgrades {
            if let Some(url) = &entry.compare {
                on_progress(&format!(
                    "{} {} → {} ({url})",
                    entry.action, entry.from, entry.to
                ));
            }
        }

        let report = UpgradeReport {
            upgrades,
            workflows_updated,
            up_to_date: false,
            ..Default::default()
        };

        Ok(report)
    }
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "tests use unwrap, indexing, and other patterns freely"
)]
mod tests {
    use super::{Lock, build_entry, empty_json_report};
    use crate::domain::action::identity::{ActionId, CommitDate, CommitSha, Repository, Version};
    use crate::domain::action::resolved::{Commit, ResolvedRef};
    use crate::domain::action::spec::Spec as ActionSpec;
    use crate::domain::action::specifier::Specifier;
    use crate::domain::action::upgrade::{Action, Candidate as UpgradeCandidate};
    use crate::domain::action::uses_ref::RefType;

    fn lock_with_tag(id: &str, specifier: &str, tag: &str) -> Lock {
        let mut lock = Lock::default();
        let spec = ActionSpec::new(ActionId::from(id), Specifier::parse(specifier));
        lock.set(
            &spec,
            ResolvedRef::Tag(Version::from(tag)),
            Commit {
                sha: CommitSha::from("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
                repository: Repository::from(id),
                ref_type: Some(RefType::Tag),
                date: CommitDate::from("2026-01-01T00:00:00Z"),
            },
        );
        lock
    }

    #[test]
    fn from_is_old_resolved_version_not_specifier() {
        // The pre-upgrade lock resolved `^6` to `v6.0.1`. An in-range bump to
        // `v6.0.3` must report `from = v6.0.1` — the version the reviewer is
        // moving away from — NOT the `^6` range.
        let lock = lock_with_tag("actions/checkout", "^6", "v6.0.1");
        let candidate = UpgradeCandidate {
            id: ActionId::from("actions/checkout"),
            current: Specifier::parse("^6"),
            action: Action::InRange {
                candidate: Version::from("v6.0.3"),
            },
        };

        let entry = build_entry(&candidate, &lock);

        assert_eq!(entry.from, "v6.0.1");
        assert_eq!(entry.to, "v6.0.3");
        assert!(entry.in_range);
        assert_eq!(
            entry.compare.as_deref(),
            Some("https://github.com/actions/checkout/compare/v6.0.1...v6.0.3")
        );
    }

    #[test]
    fn cross_range_reports_resolved_versions_and_is_not_in_range() {
        // A major bump: lock had v6.0.1 under `^6`; candidate is v7.0.0. The old
        // entry is still keyed by the ORIGINAL `^6`, so `from` resolves to v6.0.1.
        let lock = lock_with_tag("actions/checkout", "^6", "v6.0.1");
        let candidate = UpgradeCandidate {
            id: ActionId::from("actions/checkout"),
            current: Specifier::parse("^6"),
            action: Action::CrossRange {
                candidate: Version::from("v7.0.0"),
                new_specifier: Specifier::parse("^7"),
            },
        };

        let entry = build_entry(&candidate, &lock);

        assert_eq!(entry.from, "v6.0.1");
        assert_eq!(entry.to, "v7.0.0");
        assert!(!entry.in_range);
        assert_eq!(
            entry.compare.as_deref(),
            Some("https://github.com/actions/checkout/compare/v6.0.1...v7.0.0")
        );
    }

    #[test]
    fn no_lock_entry_falls_back_to_specifier_and_omits_compare() {
        // First-time pin (no prior lock entry): there is no old version to
        // compare against, so `from` falls back to the specifier and no link.
        let candidate = UpgradeCandidate {
            id: ActionId::from("actions/setup-node"),
            current: Specifier::parse("^4"),
            action: Action::InRange {
                candidate: Version::from("v4.1.0"),
            },
        };

        let entry = build_entry(&candidate, &Lock::default());

        assert_eq!(entry.from, "^4");
        assert_eq!(entry.to, "v4.1.0");
        assert!(entry.compare.is_none());
    }

    #[test]
    fn branch_pin_has_no_compare_link() {
        // A branch pin exposes no tag, so a version diff is meaningless.
        let mut lock = Lock::default();
        let spec = ActionSpec::new(ActionId::from("actions/checkout"), Specifier::parse("^6"));
        lock.set(
            &spec,
            ResolvedRef::Branch(Version::from("main")),
            Commit {
                sha: CommitSha::from("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
                repository: Repository::from("actions/checkout"),
                ref_type: Some(RefType::Branch),
                date: CommitDate::from("2026-01-01T00:00:00Z"),
            },
        );
        let candidate = UpgradeCandidate {
            id: ActionId::from("actions/checkout"),
            current: Specifier::parse("^6"),
            action: Action::InRange {
                candidate: Version::from("v6.0.3"),
            },
        };

        let entry = build_entry(&candidate, &lock);

        assert!(entry.compare.is_none());
    }

    #[test]
    fn empty_json_report_is_up_to_date() {
        let value: serde_json::Value = serde_json::from_str(&empty_json_report()).unwrap();
        assert_eq!(value["up_to_date"], true);
        assert_eq!(value["upgrades"].as_array().unwrap().len(), 0);
    }
}
