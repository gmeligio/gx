use std::path::Path;
use thiserror::Error;

use crate::config::Config;
use crate::infrastructure::{
    FileLock, FileManifest, FileWorkflowScanner, FileWorkflowUpdater, GithubError, GithubRegistry,
    LockFileError, ManifestError, WorkflowError,
};

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
