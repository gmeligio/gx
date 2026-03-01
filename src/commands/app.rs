use std::path::Path;
use thiserror::Error;

use crate::config::Config;
use crate::infrastructure::{
    FileLock, FileManifest, FileWorkflowScanner, FileWorkflowUpdater, GithubError, GithubRegistry,
    LockFileError, ManifestError, WorkflowError,
};

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

/// Run the tidy command with automatic store selection based on manifest existence.
///
/// # Errors
///
/// Returns [`AppError::Workflow`] if workflows cannot be scanned.
/// Returns [`AppError::Manifest`] if the manifest cannot be loaded.
/// Returns [`AppError::Lock`] if the lock file cannot be loaded.
/// Returns [`AppError::Github`] if the registry cannot be created.
/// Returns [`AppError::Tidy`] if the tidy command fails.
pub fn tidy(repo_root: &Path, config: Config) -> Result<(), AppError> {
    let has_manifest = config.manifest_path.exists();
    let registry = GithubRegistry::new(config.settings.github_token)?;
    let scanner = FileWorkflowScanner::new(repo_root);
    let updater = FileWorkflowUpdater::new(repo_root);

    let (updated_manifest, updated_lock) = super::tidy::run(
        config.manifest,
        config.lock,
        &config.manifest_path,
        registry,
        &scanner,
        &updater,
    )?;

    if has_manifest {
        FileManifest::new(&config.manifest_path).save(&updated_manifest)?;
        FileLock::new(&config.lock_path).save(&updated_lock)?;
    }

    Ok(())
}

/// Run the init command: create manifest and lock files from current workflows.
///
/// # Errors
///
/// Returns [`AppError::AlreadyInitialized`] if the manifest file already exists.
/// Returns [`AppError::Manifest`] if the manifest cannot be loaded.
/// Returns [`AppError::Lock`] if the lock file cannot be loaded.
/// Returns [`AppError::Github`] if the registry cannot be created.
/// Returns [`AppError::Workflow`] if workflows cannot be scanned.
/// Returns [`AppError::Tidy`] if the tidy command fails.
pub fn init(repo_root: &Path, config: Config) -> Result<(), AppError> {
    if config.manifest_path.exists() {
        return Err(AppError::AlreadyInitialized);
    }
    log::info!("Reading actions from workflows into the manifest...");
    let registry = GithubRegistry::new(config.settings.github_token)?;
    let scanner = FileWorkflowScanner::new(repo_root);
    let updater = FileWorkflowUpdater::new(repo_root);

    let (updated_manifest, updated_lock) = super::tidy::run(
        config.manifest,
        config.lock,
        &config.manifest_path,
        registry,
        &scanner,
        &updater,
    )?;

    // Always save for init — this creates the files
    FileManifest::new(&config.manifest_path).save(&updated_manifest)?;
    FileLock::new(&config.lock_path).save(&updated_lock)?;

    Ok(())
}

/// Run the upgrade command with automatic store selection based on manifest existence.
///
/// # Errors
///
/// Returns [`AppError::Workflow`] if workflows cannot be scanned.
/// Returns [`AppError::Manifest`] if the manifest cannot be loaded.
/// Returns [`AppError::Lock`] if the lock file cannot be loaded.
/// Returns [`AppError::Github`] if the registry cannot be created.
/// Returns [`AppError::Upgrade`] if the upgrade command fails.
pub fn upgrade(repo_root: &Path, config: Config, request: &UpgradeRequest) -> Result<(), AppError> {
    let has_manifest = config.manifest_path.exists();
    let registry = GithubRegistry::new(config.settings.github_token)?;
    let updater = FileWorkflowUpdater::new(repo_root);

    let (updated_manifest, updated_lock) =
        super::upgrade::run(config.manifest, config.lock, registry, &updater, request)?;

    if has_manifest {
        FileManifest::new(&config.manifest_path).save(&updated_manifest)?;
        FileLock::new(&config.lock_path).save(&updated_lock)?;
    }

    Ok(())
}

/// Run the lint command: check workflows against manifest and lock without modifying anything.
///
/// # Errors
///
/// Returns [`AppError::Workflow`] if workflows cannot be scanned.
/// Returns [`AppError::Manifest`] if the manifest cannot be loaded.
/// Returns [`AppError::Lock`] if the lock file cannot be loaded.
/// Returns [`AppError::Lint`] if violations are found.
pub fn lint(repo_root: &Path, config: &Config) -> Result<(), AppError> {
    use log::info;

    let scanner = FileWorkflowScanner::new(repo_root);
    let action_set = scanner.scan_all()?;
    let workflows = scanner.scan_all_located()?;

    let diagnostics = super::lint::run(
        &config.manifest,
        &config.lock,
        &workflows,
        &action_set,
        &config.lint_config,
    )?;

    // Print diagnostics
    if diagnostics.is_empty() {
        info!("No lint issues found.");
        return Ok(());
    }

    for diag in &diagnostics {
        let level_str = match diag.level {
            crate::config::Level::Error => "[error]",
            crate::config::Level::Warn => "[warn]",
            crate::config::Level::Off => "[off]",
        };
        let location = diag
            .workflow
            .as_ref()
            .map(|w| format!("{w}: "))
            .unwrap_or_default();
        info!("{} {}{}: {}", level_str, location, diag.rule, diag.message);
    }

    let error_count = diagnostics
        .iter()
        .filter(|d| d.level == crate::config::Level::Error)
        .count();
    let warn_count = diagnostics
        .iter()
        .filter(|d| d.level == crate::config::Level::Warn)
        .count();
    info!(
        "{} issue(s) ({} error{}, {} warning{})",
        diagnostics.len(),
        error_count,
        if error_count == 1 { "" } else { "s" },
        warn_count,
        if warn_count == 1 { "" } else { "s" }
    );

    // Return error if there are violations
    if error_count > 0 {
        return Err(AppError::Lint(super::lint::LintError::ViolationsFound {
            errors: error_count,
            warnings: warn_count,
        }));
    }

    Ok(())
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
