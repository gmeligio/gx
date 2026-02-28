use std::path::Path;
use thiserror::Error;

use crate::config::Config;
use crate::infrastructure::{
    FileLock, FileManifest, FileWorkflowScanner, FileWorkflowUpdater, GithubError, GithubRegistry,
    LockFileError, ManifestError, ManifestStore, MemoryLock, MemoryManifest, WorkflowError,
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
    let registry = GithubRegistry::new(config.settings.github_token)?;
    let scanner = FileWorkflowScanner::new(repo_root);
    let updater = FileWorkflowUpdater::new(repo_root);

    if config.manifest_path.exists() {
        let manifest_store = FileManifest::new(&config.manifest_path);
        let lock_store = FileLock::new(&config.lock_path);
        super::tidy::run(
            repo_root,
            config.manifest,
            manifest_store,
            config.lock,
            lock_store,
            registry,
            &scanner,
            &updater,
        )?;
    } else {
        let action_set = FileWorkflowScanner::new(repo_root).scan_all()?;
        let manifest_store = MemoryManifest::from_workflows(&action_set);
        let manifest = manifest_store.load()?;
        let lock_store = MemoryLock;
        super::tidy::run(
            repo_root,
            manifest,
            manifest_store,
            config.lock,
            lock_store,
            registry,
            &scanner,
            &updater,
        )?;
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
    let manifest_store = FileManifest::new(&config.manifest_path);
    let lock_store = FileLock::new(&config.lock_path);
    let scanner = FileWorkflowScanner::new(repo_root);
    let updater = FileWorkflowUpdater::new(repo_root);
    super::tidy::run(
        repo_root,
        config.manifest,
        manifest_store,
        config.lock,
        lock_store,
        registry,
        &scanner,
        &updater,
    )?;
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
    let registry = GithubRegistry::new(config.settings.github_token)?;
    let updater = FileWorkflowUpdater::new(repo_root);

    if config.manifest_path.exists() {
        let manifest_store = FileManifest::new(&config.manifest_path);
        let lock_store = FileLock::new(&config.lock_path);
        super::upgrade::run(
            repo_root,
            config.manifest,
            manifest_store,
            config.lock,
            lock_store,
            registry,
            &updater,
            request,
        )?;
    } else {
        let action_set = FileWorkflowScanner::new(repo_root).scan_all()?;
        let manifest_store = MemoryManifest::from_workflows(&action_set);
        let manifest = manifest_store.load()?;
        let lock_store = MemoryLock;
        super::upgrade::run(
            repo_root,
            manifest,
            manifest_store,
            config.lock,
            lock_store,
            registry,
            &updater,
            request,
        )?;
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
