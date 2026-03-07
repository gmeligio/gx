pub mod report;

use std::path::Path;

use self::report::InitReport;
use crate::config::Config;
use crate::infra::{
    FileWorkflowScanner, FileWorkflowUpdater, GithubRegistry, create_lock, create_manifest,
};

use crate::domain::AppError;
use crate::domain::Command;

/// The init command struct.
pub struct Init;

impl Command for Init {
    type Report = InitReport;

    fn run(
        &self,
        repo_root: &Path,
        config: Config,
        on_progress: &mut dyn FnMut(&str),
    ) -> Result<InitReport, AppError> {
        if config.manifest_path.exists() {
            return Err(AppError::AlreadyInitialized);
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
            registry,
            &scanner,
            on_progress,
        )?;

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
