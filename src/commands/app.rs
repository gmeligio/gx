use std::path::Path;
use thiserror::Error;

use crate::config::Config;
use crate::domain::{LockKey, WorkflowError};
use crate::infrastructure::{
    FileWorkflowScanner, FileWorkflowUpdater, GithubError, GithubRegistry, LockFileError,
    ManifestError, apply_lock_diff, apply_manifest_diff, create_lock, create_manifest,
};
use crate::output::{InitReport, LintReport, TidyReport, UpgradeReport};

use crate::domain::UpgradeAction;

use super::lint::LintError;
use super::tidy::TidyError;
use super::upgrade::{UpgradeError, UpgradeRequest};

/// Errors that can occur during command orchestration
#[derive(Debug, Error)]
pub enum AppError {
    /// The manifest file already exists when running init.
    #[error("already initialized \u{2014} use `gx tidy` to update")]
    AlreadyInitialized,

    /// The manifest store encountered an error.
    #[error(transparent)]
    Manifest(#[from] ManifestError),

    /// The lock store encountered an error.
    #[error(transparent)]
    Lock(#[from] LockFileError),

    /// Workflow scanning or updating failed.
    #[error(transparent)]
    Workflow(#[from] WorkflowError),

    /// The GitHub registry could not be initialized.
    #[error(transparent)]
    Github(#[from] GithubError),

    /// The tidy command failed.
    #[error(transparent)]
    Tidy(#[from] TidyError),

    /// The upgrade command failed.
    #[error(transparent)]
    Upgrade(#[from] UpgradeError),

    /// The lint command failed.
    #[error(transparent)]
    Lint(#[from] LintError),
}

/// Run the tidy command. Returns a `TidyReport` describing what changed.
///
/// # Errors
///
/// Returns [`AppError`] variants on failure.
pub fn tidy(
    repo_root: &Path,
    config: Config,
    mut on_progress: impl FnMut(&str),
) -> Result<TidyReport, AppError> {
    let has_manifest = config.manifest_path.exists();
    if config.settings.github_token.is_none() {
        on_progress(
            "Warning: No GITHUB_TOKEN set — using unauthenticated GitHub API (60 requests/hour limit).",
        );
    }
    let registry = GithubRegistry::new(config.settings.github_token)?;
    let scanner = FileWorkflowScanner::new(repo_root);
    let updater = FileWorkflowUpdater::new(repo_root);

    let original_manifest = config.manifest.clone();

    let plan = super::tidy::plan(
        &config.manifest,
        &config.lock,
        registry,
        &scanner,
        on_progress,
    )?;

    if plan.is_empty() {
        return Ok(TidyReport::default());
    }

    if has_manifest {
        apply_manifest_diff(&config.manifest_path, &plan.manifest)?;
        if config.lock_path.exists() {
            apply_lock_diff(&config.lock_path, &plan.lock)?;
        } else {
            create_lock(&config.lock_path, &plan.lock)?;
        }
    }

    let workflows_updated =
        super::tidy::apply_workflow_patches(&updater, &plan.workflows, &plan.corrections)?;

    let report = TidyReport {
        removed: plan
            .manifest
            .removed
            .iter()
            .map(std::string::ToString::to_string)
            .collect(),
        added: plan
            .manifest
            .added
            .iter()
            .map(|(id, v)| (id.to_string(), v.to_string()))
            .collect(),
        upgraded: plan
            .manifest
            .updated
            .iter()
            .map(|(id, new_v)| {
                let old_v = original_manifest
                    .get(id)
                    .map(std::string::ToString::to_string)
                    .unwrap_or_else(|| {
                        // Fallback: use new version as "from" if original not found
                        let _ = LockKey::new(id.clone(), new_v.clone());
                        "?".to_string()
                    });
                (id.to_string(), old_v, new_v.to_string())
            })
            .collect(),
        corrections: plan.corrections.len(),
        workflows_updated,
    };

    Ok(report)
}

/// Run the init command: create manifest and lock files from current workflows.
///
/// # Errors
///
/// Returns [`AppError::AlreadyInitialized`] if the manifest file already exists.
/// Returns other [`AppError`] variants on failure.
pub fn init(
    repo_root: &Path,
    config: Config,
    mut on_progress: impl FnMut(&str),
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

    let plan = super::tidy::plan(
        &config.manifest,
        &config.lock,
        registry,
        &scanner,
        on_progress,
    )?;

    if !plan.is_empty() {
        create_manifest(&config.manifest_path, &plan.manifest)?;
        create_lock(&config.lock_path, &plan.lock)?;
        super::tidy::apply_workflow_patches(&updater, &plan.workflows, &plan.corrections)?;
    }

    let report = InitReport {
        actions_discovered: plan.manifest.added.len(),
        created: !plan.is_empty(),
    };

    Ok(report)
}

/// Run the upgrade command. Returns an `UpgradeReport` describing what was upgraded.
///
/// # Errors
///
/// Returns [`AppError`] variants on failure.
pub fn upgrade(
    repo_root: &Path,
    config: Config,
    request: &UpgradeRequest,
    on_progress: impl FnMut(&str),
) -> Result<UpgradeReport, AppError> {
    let has_manifest = config.manifest_path.exists();
    let registry = GithubRegistry::new(config.settings.github_token)?;
    let updater = FileWorkflowUpdater::new(repo_root);

    let plan = super::upgrade::plan(
        &config.manifest,
        &config.lock,
        registry,
        request,
        on_progress,
    )?;

    if plan.is_empty() {
        return Ok(UpgradeReport {
            up_to_date: true,
            ..Default::default()
        });
    }

    if has_manifest {
        apply_manifest_diff(&config.manifest_path, &plan.manifest)?;
        apply_lock_diff(&config.lock_path, &plan.lock)?;
    }

    let workflows_updated =
        super::upgrade::apply_upgrade_workflows(&updater, &plan.lock, &plan.upgrades)?;

    let upgrades = plan
        .upgrades
        .iter()
        .map(|u| {
            let from = u.current.to_string();
            let to = match &u.action {
                UpgradeAction::InRange { candidate } => candidate.to_string(),
                UpgradeAction::CrossRange {
                    new_manifest_version,
                    ..
                } => new_manifest_version.to_string(),
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

/// Run the lint command: check workflows against manifest and lock without modifying anything.
///
/// # Errors
///
/// Returns [`AppError::Lint`] if violations are found.
/// Returns other [`AppError`] variants on failure.
pub fn lint(repo_root: &Path, config: &Config) -> Result<LintReport, AppError> {
    let scanner = FileWorkflowScanner::new(repo_root);

    let diagnostics = super::lint::run(
        &config.manifest,
        &config.lock,
        &scanner,
        &config.lint_config,
    )?;

    let report = super::lint::format_and_report(diagnostics)?;

    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_error_already_initialized_message() {
        let err = AppError::AlreadyInitialized;
        assert_eq!(
            err.to_string(),
            "already initialized \u{2014} use `gx tidy` to update"
        );
    }
}
