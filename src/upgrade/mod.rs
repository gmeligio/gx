pub mod cli;
pub mod plan;
pub mod report;
pub mod types;

use std::path::Path;

use crate::command::Command;
use crate::config::Config;
use crate::domain::action::upgrade::Action;
use crate::infra::github::Registry;
use crate::infra::lock::Error as LockFileError;
use crate::infra::manifest::Error as ManifestError;
use crate::infra::workflow_update::WorkflowWriter;
use report::Report as UpgradeReport;
use thiserror::Error;
use types::{Error as UpgradeError, Request as UpgradeRequest};

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
        let registry = Registry::new(config.settings.github_token)?;
        let updater = WorkflowWriter::new(repo_root);

        let upgrade_plan = plan::plan(
            &config.manifest,
            &config.lock,
            &registry,
            &self.request,
            &mut *on_progress,
        )?;

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

        let upgrades = upgrade_plan
            .upgrades
            .iter()
            .map(|u| {
                let from = u.current.to_string();
                let to = match &u.action {
                    Action::InRange { candidate } => candidate.to_string(),
                    Action::CrossRange { new_specifier, .. } => new_specifier.to_string(),
                };
                (u.id.to_string(), from, to)
            })
            .collect();

        let report = UpgradeReport {
            upgrades,
            workflows_updated,
            up_to_date: false,
            ..Default::default()
        };

        Ok(report)
    }
}
