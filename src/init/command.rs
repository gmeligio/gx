use super::report::Report;
use crate::command::Command;
use crate::config::Config;
use crate::domain::file::scan::Error as WorkflowError;
use crate::infra::github::{Error as GithubError, Registry as GithubRegistry};
use crate::infra::lock::Error as LockFileError;
use crate::infra::manifest::Error as ManifestError;
use crate::infra::registry::caching_retrying;
use crate::infra::workflow_scan::FileScanner as FileWorkflowScanner;
use crate::infra::workflow_update::WorkflowWriter;
use crate::tidy::Error as TidyError;
use std::path::Path;
use thiserror::Error;

/// Errors that can occur during the init command.
#[derive(Debug, Error)]
pub enum Error {
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
    type Report = Report;
    type Error = Error;

    fn run(
        &self,
        repo_root: &Path,
        config: Config,
        on_progress: &mut dyn FnMut(&str),
    ) -> Result<Report, Error> {
        if config.manifest_path.exists() {
            return Err(Error::AlreadyInitialized);
        }
        on_progress("Reading actions from workflows into the manifest...");
        if config.settings.github_token.is_none() {
            on_progress(
                "Warning: No GITHUB_TOKEN set — using unauthenticated GitHub API (60 requests/hour limit).",
            );
        }
        let github = GithubRegistry::new(config.settings.github_token)?;
        // Cache outside retry, so a repeated query never reaches the retry layer
        // and a wait is only spent on a request that must reach GitHub. Each wait
        // is announced through the progress channel so a pause is never an
        // unexplained stall.
        let (registry, progress) = caching_retrying(github, &mut *on_progress);
        let scanner = FileWorkflowScanner::new(repo_root);
        let updater = WorkflowWriter::new(repo_root);

        let plan = crate::tidy::plan(
            &config.manifest,
            &config.lock,
            &registry,
            &scanner,
            progress,
        )?;

        if !plan.is_empty() {
            crate::infra::manifest::create(&config.manifest_path, &plan.manifest)?;
            let lock_store = crate::infra::lock::Store::new(&config.lock_path);
            lock_store.save(&plan.lock)?;
            crate::tidy::apply_workflow_patches(&updater, &plan.workflows)?;
        }

        let report = Report {
            actions_discovered: plan.manifest.added.len(),
            created: !plan.is_empty(),
        };

        Ok(report)
    }
}
