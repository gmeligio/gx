use anyhow::Result;
use std::path::Path;

use crate::domain::Lock;
use crate::infrastructure::{
    FileLock, FileManifest, FileWorkflowScanner, FileWorkflowUpdater, GithubRegistry, LockStore,
    ManifestStore, MemoryLock, MemoryManifest,
};

use super::upgrade::UpgradeMode;

/// Run the tidy command with automatic store selection based on manifest existence.
///
/// # Errors
///
/// Returns an error if the registry cannot be created, stores cannot be loaded,
/// or the underlying tidy command fails.
pub fn tidy(repo_root: &Path, manifest_path: &Path, lock_path: &Path) -> Result<()> {
    let registry = GithubRegistry::from_env()?;
    let scanner = FileWorkflowScanner::new(repo_root);
    let updater = FileWorkflowUpdater::new(repo_root);

    if manifest_path.exists() {
        let manifest_store = FileManifest::new(manifest_path);
        let manifest = manifest_store.load()?;
        let lock_store = FileLock::new(lock_path);
        let lock = lock_store.load()?;
        super::tidy::run(
            repo_root,
            manifest,
            manifest_store,
            lock,
            lock_store,
            registry,
            &scanner,
            &updater,
        )
    } else {
        let action_set = FileWorkflowScanner::new(repo_root).scan_all()?;
        let manifest_store = MemoryManifest::from_workflows(&action_set);
        let manifest = manifest_store.load()?;
        let lock_store = MemoryLock;
        let lock = Lock::default();
        super::tidy::run(
            repo_root,
            manifest,
            manifest_store,
            lock,
            lock_store,
            registry,
            &scanner,
            &updater,
        )
    }
}

/// Run the init command: create manifest and lock files from current workflows.
///
/// # Errors
///
/// Returns an error if the manifest already exists, the registry cannot be created,
/// or the underlying tidy command fails.
pub fn init(repo_root: &Path, manifest_path: &Path, lock_path: &Path) -> Result<()> {
    if manifest_path.exists() {
        anyhow::bail!("Already initialized. Use `gx tidy` to update.");
    }
    log::info!("Reading actions from workflows into the manifest...");
    let registry = GithubRegistry::from_env()?;
    let manifest_store = FileManifest::new(manifest_path);
    let manifest = manifest_store.load()?;
    let lock_store = FileLock::new(lock_path);
    let lock = lock_store.load()?;
    let scanner = FileWorkflowScanner::new(repo_root);
    let updater = FileWorkflowUpdater::new(repo_root);
    super::tidy::run(
        repo_root,
        manifest,
        manifest_store,
        lock,
        lock_store,
        registry,
        &scanner,
        &updater,
    )
}

/// Run the upgrade command with automatic store selection based on manifest existence.
///
/// # Errors
///
/// Returns an error if the registry cannot be created, stores cannot be loaded,
/// or the underlying upgrade command fails.
pub fn upgrade(
    repo_root: &Path,
    manifest_path: &Path,
    lock_path: &Path,
    mode: &UpgradeMode,
) -> Result<()> {
    let registry = GithubRegistry::from_env()?;
    let scanner = FileWorkflowScanner::new(repo_root);
    let updater = FileWorkflowUpdater::new(repo_root);

    if manifest_path.exists() {
        let manifest_store = FileManifest::new(manifest_path);
        let manifest = manifest_store.load()?;
        let lock_store = FileLock::new(lock_path);
        let lock = lock_store.load()?;
        super::upgrade::run(
            repo_root,
            manifest,
            manifest_store,
            lock,
            lock_store,
            registry,
            &scanner,
            &updater,
            mode,
        )
    } else {
        let action_set = FileWorkflowScanner::new(repo_root).scan_all()?;
        let manifest_store = MemoryManifest::from_workflows(&action_set);
        let manifest = manifest_store.load()?;
        let lock_store = MemoryLock;
        let lock = Lock::default();
        super::upgrade::run(
            repo_root,
            manifest,
            manifest_store,
            lock,
            lock_store,
            registry,
            &scanner,
            &updater,
            mode,
        )
    }
}
