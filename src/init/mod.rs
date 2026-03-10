pub mod report;

use crate::command::Command;
use crate::config::Config;
use crate::domain::WorkflowError;
use crate::infra::{
    FileWorkflowScanner, FileWorkflowUpdater, GithubError, GithubRegistry, LockFileError,
    ManifestError, create_lock, create_manifest,
};
use crate::tidy::TidyError;
use report::InitReport;
use std::path::Path;
use thiserror::Error;

/// Errors that can occur during the init command
#[derive(Debug, Error)]
pub enum InitError {
    #[error("already initialized \u{2014} use `gx tidy` to update")]
    AlreadyInitialized,
    #[error(transparent)]
    Github(#[from] GithubError),
    #[error(transparent)]
    Manifest(#[from] ManifestError),
    #[error(transparent)]
    Lock(#[from] LockFileError),
    #[error(transparent)]
    Workflow(#[from] WorkflowError),
    #[error(transparent)]
    Tidy(#[from] TidyError),
}

/// The init command struct.
pub struct Init;

impl Command for Init {
    type Report = InitReport;
    type Error = InitError;

    fn run(
        &self,
        repo_root: &Path,
        config: Config,
        on_progress: &mut dyn FnMut(&str),
    ) -> Result<InitReport, InitError> {
        if config.manifest_path.exists() {
            return Err(InitError::AlreadyInitialized);
        }
        on_progress("Reading actions from workflows into the manifest...");
        if config.settings.github_token.is_none() {
            on_progress(
                "Warning: No GITHUB_TOKEN set — using unauthenticated GitHub API (60 requests/hour limit).",
            );
        }
        let registry = GithubRegistry::new(config.settings.github_token)?;
        let scanner = FileWorkflowScanner::new(repo_root);
        let updater = FileWorkflowUpdater::new(repo_root);

        let plan = crate::tidy::plan(
            &config.manifest,
            &config.lock,
            &registry,
            &scanner,
            &mut *on_progress,
        )?;

        if config.lock_migrated {
            on_progress("migrated gx.lock → v1.4");
        }

        if !plan.is_empty() {
            create_manifest(&config.manifest_path, &plan.manifest)?;
            create_lock(&config.lock_path, &plan.lock)?;
            crate::tidy::apply_workflow_patches(&updater, &plan.workflows, &plan.corrections)?;
        }

        let report = InitReport {
            actions_discovered: plan.manifest.added.len(),
            created: !plan.is_empty(),
        };

        Ok(report)
    }
}
