pub mod cli;
pub mod plan;
pub mod report;
pub mod types;

use std::path::Path;

use crate::command::Command;
use crate::config::Config;
use crate::domain::UpgradeAction;
use crate::infra::{
    FileWorkflowUpdater, GithubError, GithubRegistry, LockFileError, ManifestError,
    apply_lock_diff, apply_manifest_diff,
};
use report::UpgradeReport;
use thiserror::Error;

pub use cli::{ResolveError, resolve_upgrade_mode};
pub use plan::{apply_upgrade_workflows, plan};
pub use types::{UpgradeError, UpgradeMode, UpgradePlan, UpgradeRequest, UpgradeScope};

/// Errors that can occur during the upgrade command's run phase (I/O + domain)
#[derive(Debug, Error)]
pub enum UpgradeRunError {
    #[error(transparent)]
    Github(#[from] GithubError),
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
    type Error = UpgradeRunError;

    fn run(
        &self,
        repo_root: &Path,
        config: Config,
        on_progress: &mut dyn FnMut(&str),
    ) -> Result<UpgradeReport, UpgradeRunError> {
        let has_manifest = config.manifest_path.exists();
        let registry = GithubRegistry::new(config.settings.github_token)?;
        let updater = FileWorkflowUpdater::new(repo_root);

        let upgrade_plan = plan(
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
            apply_manifest_diff(&config.manifest_path, &upgrade_plan.manifest)?;
            apply_lock_diff(&config.lock_path, &upgrade_plan.lock)?;
        }

        let workflows_updated =
            apply_upgrade_workflows(&updater, &upgrade_plan.lock, &upgrade_plan.upgrades)?;

        if config.manifest_migrated {
            on_progress("migrated gx.toml → semver specifiers");
        }
        if config.lock_migrated {
            on_progress("migrated gx.lock → v1.4");
        }

        let upgrades = upgrade_plan
            .upgrades
            .iter()
            .map(|u| {
                let from = u.current.to_string();
                let to = match &u.action {
                    UpgradeAction::InRange { candidate } => candidate.to_string(),
                    UpgradeAction::CrossRange { new_specifier, .. } => new_specifier.to_string(),
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
