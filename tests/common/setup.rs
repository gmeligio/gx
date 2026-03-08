#![allow(dead_code)]
use std::fs;
use std::io::Write as IoWrite;
use std::path::{Path, PathBuf};

use gx::config::LintConfig;
use gx::domain::{Lock, Manifest, VersionRegistry};
use gx::infra::{
    FileWorkflowScanner, FileWorkflowUpdater, apply_lock_diff, apply_manifest_diff, create_lock,
    create_manifest, parse_lock, parse_manifest,
};
use gx::upgrade::UpgradeRequest;
use gx::{lint, tidy, upgrade};
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
        create_manifest(&mp, &plan.manifest).unwrap();
        create_lock(&lp, &plan.lock).unwrap();
        tidy::apply_workflow_patches(&updater, &plan.workflows, &plan.corrections).unwrap();
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
        parse_manifest(&mp).unwrap()
    } else {
        Manifest::default()
    };
    let lock = parse_lock(&lp).unwrap();

    let plan = tidy::plan(&manifest, &lock, registry, &scanner, |_| {}).unwrap();
    if !plan.is_empty() {
        if has_manifest {
            apply_manifest_diff(&mp, &plan.manifest).unwrap();
            if lp.exists() {
                apply_lock_diff(&lp, &plan.lock).unwrap();
            } else {
                create_lock(&lp, &plan.lock).unwrap();
            }
        }
        tidy::apply_workflow_patches(&updater, &plan.workflows, &plan.corrections).unwrap();
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
    let manifest = parse_manifest(&mp).unwrap();
    let lock = parse_lock(&lp).unwrap();
    let updater = FileWorkflowUpdater::new(root);

    let plan = upgrade::plan(&manifest, &lock, registry, request, |_| {}).unwrap();
    if !plan.is_empty() {
        apply_manifest_diff(&mp, &plan.manifest).unwrap();
        apply_lock_diff(&lp, &plan.lock).unwrap();
        upgrade::apply_upgrade_workflows(&updater, &plan.lock, &plan.upgrades).unwrap();
    }
}

/// Run lint and return the diagnostics.
pub fn run_lint(root: &Path) -> Vec<lint::Diagnostic> {
    let mp = manifest_path(root);
    let lp = lock_path(root);
    let manifest = parse_manifest(&mp).unwrap();
    let lock = parse_lock(&lp).unwrap();
    let scanner = FileWorkflowScanner::new(root);
    let lint_config = LintConfig::default();
    lint::collect_diagnostics(&manifest, &lock, &scanner, &lint_config, &mut |_| {}).unwrap()
}
