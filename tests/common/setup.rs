#![expect(
    dead_code,
    reason = "shared test helpers: not every integration test crate uses every item"
)]
use gx::config::Lint;
use gx::domain::lock::Lock;
use gx::domain::manifest::Manifest;
use gx::domain::resolution::VersionRegistry;
use gx::infra::lock::Store as LockStore;
use gx::infra::manifest::patch::apply_manifest_diff;
use gx::infra::manifest::{self};
use gx::infra::workflow_scan::FileScanner as FileWorkflowScanner;
use gx::infra::workflow_update::FileUpdater as FileWorkflowUpdater;
use gx::upgrade::types::Request as UpgradeRequest;
use gx::{lint, tidy, upgrade};
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// Create a repo directory structure with `.github/workflows/` directory.
pub fn create_test_repo(temp_dir: &TempDir) -> PathBuf {
    let root = temp_dir.path();
    fs::create_dir_all(root.join(".github").join("workflows")).unwrap();
    root.to_path_buf()
}

/// Write a workflow file to `.github/workflows/{name}`.
pub fn write_workflow(root: &Path, name: &str, content: &str) {
    let path = root.join(".github").join("workflows").join(name);
    let mut f = fs::File::create(&path).unwrap();
    f.write_all(content.as_bytes()).unwrap();
}

/// Path to the manifest file: `.github/gx.toml`.
pub fn manifest_path(root: &Path) -> PathBuf {
    root.join(".github").join("gx.toml")
}

/// Path to the lock file: `.github/gx.lock`.
pub fn lock_path(root: &Path) -> PathBuf {
    root.join(".github").join("gx.lock")
}

/// Create an empty manifest file (triggers file-backed mode).
pub fn create_empty_manifest(root: &Path) {
    fs::write(manifest_path(root), "[actions]\n").unwrap();
}

/// Write arbitrary content to the manifest file.
pub fn write_manifest(root: &Path, content: &str) {
    fs::write(manifest_path(root), content).unwrap();
}

/// Write arbitrary content to the lock file.
pub fn write_lock(root: &Path, content: &str) {
    fs::write(lock_path(root), content).unwrap();
}

/// Run the init pipeline: plan + create files.
pub fn run_init<R: VersionRegistry + Clone>(root: &Path, registry: &R) {
    let mp = manifest_path(root);
    let lp = lock_path(root);
    assert!(!mp.exists(), "init requires no existing manifest");

    let manifest = Manifest::default();
    let lock = Lock::default();
    let scanner = FileWorkflowScanner::new(root);
    let updater = FileWorkflowUpdater::new(root);

    let plan = tidy::plan(&manifest, &lock, registry, &scanner, |_| {}).unwrap();
    if !plan.is_empty() {
        manifest::create(&mp, &plan.manifest).unwrap();
        let lock_store = LockStore::new(&lp);
        lock_store.save(&plan.lock).unwrap();
        tidy::apply_workflow_patches(&updater, &plan.workflows).unwrap();
    }
}

/// Run the tidy pipeline: plan + apply diffs.
pub fn run_tidy<R: VersionRegistry + Clone>(root: &Path, registry: &R) {
    let mp = manifest_path(root);
    let lp = lock_path(root);
    let scanner = FileWorkflowScanner::new(root);
    let updater = FileWorkflowUpdater::new(root);
    let has_manifest = mp.exists();

    let manifest = if has_manifest {
        manifest::parse(&mp).unwrap().value
    } else {
        Manifest::default()
    };
    let lock_store = LockStore::new(&lp);
    let lock = lock_store.load().unwrap();

    let plan = tidy::plan(&manifest, &lock, registry, &scanner, |_| {}).unwrap();
    if !plan.is_empty() {
        if has_manifest {
            apply_manifest_diff(&mp, &plan.manifest).unwrap();
            lock_store.save(&plan.lock).unwrap();
        }
        tidy::apply_workflow_patches(&updater, &plan.workflows).unwrap();
    }
}

/// Run the upgrade pipeline: plan + apply diffs.
pub fn run_upgrade<R: VersionRegistry + Clone>(
    root: &Path,
    registry: &R,
    request: &UpgradeRequest,
) {
    let mp = manifest_path(root);
    let lp = lock_path(root);
    let manifest = manifest::parse(&mp).unwrap();
    let lock_store = LockStore::new(&lp);
    let lock = lock_store.load().unwrap();
    let updater = FileWorkflowUpdater::new(root);

    let plan = upgrade::plan::plan(&manifest.value, &lock, registry, request, |_| {}).unwrap();
    if !plan.is_empty() {
        apply_manifest_diff(&mp, &plan.manifest).unwrap();
        lock_store.save(&plan.lock).unwrap();
        upgrade::plan::apply_upgrade_workflows(&updater, &plan.lock_changes, &plan.upgrades)
            .unwrap();
    }
}

/// Run lint and return the diagnostics.
pub fn run_lint(root: &Path) -> Vec<lint::Diagnostic> {
    let mp = manifest_path(root);
    let lp = lock_path(root);
    let manifest = manifest::parse(&mp).unwrap();
    let lock_store = LockStore::new(&lp);
    let lock = lock_store.load().unwrap();
    let scanner = FileWorkflowScanner::new(root);
    let lint_config = Lint::default();
    lint::collect_diagnostics(&manifest.value, &lock, &scanner, &lint_config, &mut |_| {}).unwrap()
}
