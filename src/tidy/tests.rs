// Integration tests for the tidy module — exercises plan() and apply_workflow_patches()

use super::{TidyError, apply_workflow_patches, plan};
use crate::domain::RefType;
use crate::domain::{
    ActionId, CommitSha, Lock, LockKey, Manifest, ResolvedAction, Specifier, Version,
};
use crate::infra::{FileWorkflowScanner, FileWorkflowUpdater, parse_lock, parse_manifest};
use std::fs;

#[test]
fn tidy_error_resolution_failed_displays_specs() {
    let err = TidyError::ResolutionFailed {
        count: 2,
        specs: "actions/checkout: token required\n  actions/setup-node: timeout".to_string(),
    };
    assert_eq!(
        err.to_string(),
        "failed to resolve 2 action(s):\n  actions/checkout: token required\n  actions/setup-node: timeout"
    );
}

#[derive(Clone, Copy)]
struct NoopRegistry;
impl crate::domain::VersionRegistry for NoopRegistry {
    fn lookup_sha(
        &self,
        _id: &ActionId,
        _version: &Version,
    ) -> Result<crate::domain::ResolvedRef, crate::domain::ResolutionError> {
        Err(crate::domain::ResolutionError::AuthRequired)
    }
    fn tags_for_sha(
        &self,
        _id: &ActionId,
        _sha: &CommitSha,
    ) -> Result<Vec<Version>, crate::domain::ResolutionError> {
        Err(crate::domain::ResolutionError::AuthRequired)
    }
    fn all_tags(&self, _id: &ActionId) -> Result<Vec<Version>, crate::domain::ResolutionError> {
        Err(crate::domain::ResolutionError::AuthRequired)
    }
    fn describe_sha(
        &self,
        _id: &ActionId,
        _sha: &CommitSha,
    ) -> Result<crate::domain::ShaDescription, crate::domain::ResolutionError> {
        Err(crate::domain::ResolutionError::AuthRequired)
    }
}

/// Bug #1 + #2: when workflows have a minority version (e.g. windows.yml uses
/// `actions/checkout@v5` while all others use SHA-pinned `v6.0.1`), tidy must:
///   1. Record the minority version as an override in the manifest (Bug #1 / init)
///   2. Not overwrite windows.yml with the v6.0.1 SHA (Bug #2 / tidy)
#[test]
fn test_tidy_records_minority_version_as_override_and_does_not_overwrite_file() {
    // ---- Setup temp repo ----
    let temp_dir = tempfile::TempDir::new().unwrap();
    let repo_root = temp_dir.path();
    let workflows_dir = repo_root.join(".github").join("workflows");
    fs::create_dir_all(&workflows_dir).unwrap();
    let github_dir = repo_root.join(".github");

    // Most workflows: actions/checkout pinned to SHA with v6.0.1 comment
    let sha_workflow = "on: pull_request
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@8e8c483db84b4bee98b60c0593521ed34d9990e8 # v6.0.1
";
    fs::write(workflows_dir.join("ci.yml"), sha_workflow).unwrap();
    fs::write(workflows_dir.join("build.yml"), sha_workflow).unwrap();
    fs::write(workflows_dir.join("release.yml"), sha_workflow).unwrap();

    // windows.yml: plain tag @v5 (minority)
    let windows_workflow = "on: pull_request
jobs:
  test_windows:
    runs-on: windows-2025
    steps:
      - uses: actions/checkout@v5
";
    fs::write(workflows_dir.join("windows.yml"), windows_workflow).unwrap();

    // ---- Run tidy with empty manifest (simulates `gx init`) ----
    let manifest_path = github_dir.join("gx.toml");
    let lock_path = github_dir.join("gx.lock");

    // Pre-seed lock with both versions already resolved (simulates a pre-existing lock)
    let seed_diff = crate::domain::LockDiff {
        added: vec![
            (
                LockKey::new(
                    ActionId::from("actions/checkout"),
                    Specifier::from_v1("v6.0.1"),
                ),
                crate::domain::LockEntry::with_version_and_comment(
                    CommitSha::from("8e8c483db84b4bee98b60c0593521ed34d9990e8"),
                    Some("v6.0.1".to_string()),
                    String::new(),
                    "actions/checkout".to_string(),
                    Some(RefType::Tag),
                    "2026-01-01T00:00:00Z".to_string(),
                ),
            ),
            (
                LockKey::new(ActionId::from("actions/checkout"), Specifier::from_v1("v5")),
                crate::domain::LockEntry::with_version_and_comment(
                    CommitSha::from("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
                    Some("v5".to_string()),
                    String::new(),
                    "actions/checkout".to_string(),
                    Some(RefType::Tag),
                    "2026-01-01T00:00:00Z".to_string(),
                ),
            ),
        ],
        ..Default::default()
    };
    crate::infra::create_lock(&lock_path, &seed_diff).unwrap();

    // Load manifest and lock via free functions
    let manifest = parse_manifest(&manifest_path).unwrap().value; // empty on first run
    let lock = parse_lock(&lock_path).unwrap().value;
    let scanner = FileWorkflowScanner::new(repo_root);
    let updater = FileWorkflowUpdater::new(repo_root);

    let tidy_plan = plan(&manifest, &lock, &NoopRegistry, &scanner, |_| {}).unwrap();

    // Apply the plan — manifest doesn't exist yet so use create, lock exists so use apply
    crate::infra::create_manifest(&manifest_path, &tidy_plan.manifest).unwrap();
    crate::infra::apply_lock_diff(&lock_path, &tidy_plan.lock).unwrap();
    apply_workflow_patches(&updater, &tidy_plan.workflows, &tidy_plan.corrections).unwrap();

    // ---- Assert: manifest has global v6.0.1 + override for windows.yml v5 ----
    let saved_manifest = parse_manifest(&manifest_path).unwrap().value;

    assert_eq!(
        saved_manifest.get(&ActionId::from("actions/checkout")),
        Some(&Specifier::from_v1("v6.0.1")),
        "Global version should be v6.0.1 (dominant)"
    );

    let overrides = saved_manifest.overrides_for(&ActionId::from("actions/checkout"));
    assert!(
        !overrides.is_empty(),
        "Bug #1: Expected an override for actions/checkout v5 in windows.yml, got none"
    );

    let windows_override = overrides
        .iter()
        .find(|o| o.workflow.ends_with("windows.yml"));
    assert!(
        windows_override.is_some(),
        "Override must be scoped to windows.yml"
    );
    assert_eq!(
        windows_override.unwrap().version,
        Specifier::from_v1("v5"),
        "Override version must be v5"
    );

    // ---- Assert: windows.yml was NOT overwritten with the v6.0.1 SHA ----
    let windows_content = fs::read_to_string(workflows_dir.join("windows.yml")).unwrap();
    assert!(
        windows_content.contains("actions/checkout@"),
        "windows.yml should still reference actions/checkout"
    );
    assert!(
        !windows_content.contains("8e8c483db84b4bee98b60c0593521ed34d9990e8"),
        "Bug #2: windows.yml was overwritten with the v6.0.1 SHA — it must use the v5 ref, not v6.0.1.\nGot:\n{windows_content}"
    );
}

/// Task 2.5: Manifest authority — manifest v4 must survive even when workflows
/// have a stale SHA pointing at v3.  The manifest is the source of truth for
/// existing actions; tidy must never downgrade it from workflow state.
#[test]
fn test_manifest_authority_not_overwritten_by_workflow_sha() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let repo_root = temp_dir.path();
    let workflows_dir = repo_root.join(".github").join("workflows");
    fs::create_dir_all(&workflows_dir).unwrap();

    // Workflow pins to a SHA that actually belongs to v3
    let workflow = "on: push
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa # v3
";
    fs::write(workflows_dir.join("ci.yml"), workflow).unwrap();

    // Manifest already tracks actions/checkout at v4 (user's intent)
    let mut manifest = Manifest::default();
    manifest.set(ActionId::from("actions/checkout"), Specifier::from_v1("v4"));

    // Pre-seed lock so tidy doesn't need to resolve
    let mut lock = Lock::default();
    lock.set(&ResolvedAction::new(
        ActionId::from("actions/checkout"),
        Specifier::from_v1("v4"),
        CommitSha::from("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"),
        "actions/checkout".to_string(),
        Some(RefType::Tag),
        "2026-01-01T00:00:00Z".to_string(),
    ));

    let scanner = FileWorkflowScanner::new(repo_root);

    let tidy_plan = plan(&manifest, &lock, &NoopRegistry, &scanner, |_| {}).unwrap();

    // Manifest diff must NOT change checkout's version — v4 is preserved
    assert!(
        !tidy_plan
            .manifest
            .updated
            .iter()
            .any(|(id, _)| id == &ActionId::from("actions/checkout")),
        "Manifest v4 must not be overwritten by workflow SHA pointing to v3"
    );
    assert!(
        !tidy_plan
            .manifest
            .removed
            .contains(&ActionId::from("actions/checkout")),
        "Manifest should not remove actions/checkout"
    );
}

// ========== Step 8: tidy::plan() tests ==========

#[test]
fn test_plan_empty_workflows_returns_empty_plan() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let repo_root = temp_dir.path();
    // Create .github/workflows dir but no workflow files
    let workflows_dir = repo_root.join(".github").join("workflows");
    fs::create_dir_all(&workflows_dir).unwrap();

    let manifest = Manifest::default();
    let lock = Lock::default();
    let scanner = FileWorkflowScanner::new(repo_root);

    let result = plan(&manifest, &lock, &NoopRegistry, &scanner, |_| {}).unwrap();
    assert!(result.is_empty(), "Plan for empty workflows must be empty");
}

#[test]
fn test_plan_one_new_action_produces_added_entries() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let repo_root = temp_dir.path();
    let workflows_dir = repo_root.join(".github").join("workflows");
    fs::create_dir_all(&workflows_dir).unwrap();

    let sha = "abc123def456789012345678901234567890abcd";
    let workflow = format!(
        "on: push\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: actions/checkout@{sha} # v4\n"
    );
    fs::write(workflows_dir.join("ci.yml"), &workflow).unwrap();

    // Pre-seed lock so plan doesn't need to resolve via registry
    let mut lock = Lock::default();
    lock.set(&ResolvedAction::new(
        ActionId::from("actions/checkout"),
        Specifier::from_v1("v4"),
        CommitSha::from(sha),
        "actions/checkout".to_string(),
        Some(RefType::Tag),
        "2026-01-01T00:00:00Z".to_string(),
    ));

    let manifest = Manifest::default(); // empty — action is "new"
    let scanner = FileWorkflowScanner::new(repo_root);

    let result = plan(&manifest, &lock, &NoopRegistry, &scanner, |_| {}).unwrap();

    // Manifest should have added action
    assert!(
        result.manifest.added.iter().any(|(id, v)| {
            id == &ActionId::from("actions/checkout") && v == &Specifier::from_v1("v4")
        }),
        "Plan must include actions/checkout@v4 in manifest.added, got: {:?}",
        result.manifest.added
    );
    assert!(result.manifest.removed.is_empty());
}

#[test]
fn test_plan_removed_action_produces_removed_entries() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let repo_root = temp_dir.path();
    let workflows_dir = repo_root.join(".github").join("workflows");
    fs::create_dir_all(&workflows_dir).unwrap();

    // Workflow only has setup-node, not checkout
    let workflow = "on: push\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: actions/setup-node@aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa # v3\n";
    fs::write(workflows_dir.join("ci.yml"), workflow).unwrap();

    // Manifest has both checkout and setup-node
    let mut manifest = Manifest::default();
    manifest.set(ActionId::from("actions/checkout"), Specifier::from_v1("v4"));
    manifest.set(
        ActionId::from("actions/setup-node"),
        Specifier::from_v1("v3"),
    );

    // Pre-seed lock for both
    let mut lock = Lock::default();
    lock.set(&ResolvedAction::new(
        ActionId::from("actions/checkout"),
        Specifier::from_v1("v4"),
        CommitSha::from("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"),
        "actions/checkout".to_string(),
        Some(RefType::Tag),
        "2026-01-01T00:00:00Z".to_string(),
    ));
    lock.set(&ResolvedAction::new(
        ActionId::from("actions/setup-node"),
        Specifier::from_v1("v3"),
        CommitSha::from("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
        "actions/setup-node".to_string(),
        Some(RefType::Tag),
        "2026-01-01T00:00:00Z".to_string(),
    ));

    let scanner = FileWorkflowScanner::new(repo_root);

    let result = plan(&manifest, &lock, &NoopRegistry, &scanner, |_| {}).unwrap();

    // checkout should be removed from manifest
    assert!(
        result
            .manifest
            .removed
            .contains(&ActionId::from("actions/checkout")),
        "Plan must include actions/checkout in manifest.removed, got: {:?}",
        result.manifest.removed
    );
    // setup-node should NOT be removed
    assert!(
        !result
            .manifest
            .removed
            .contains(&ActionId::from("actions/setup-node")),
    );
    // Lock should also have checkout removed
    assert!(
        result
            .lock
            .removed
            .iter()
            .any(|k| k.id == ActionId::from("actions/checkout")),
        "Plan must include actions/checkout in lock.removed, got: {:?}",
        result.lock.removed
    );
}

#[test]
fn test_plan_everything_in_sync_returns_empty_plan() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let repo_root = temp_dir.path();
    let workflows_dir = repo_root.join(".github").join("workflows");
    fs::create_dir_all(&workflows_dir).unwrap();

    let sha = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    let workflow = format!(
        "on: push\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: actions/checkout@{sha} # v4\n"
    );
    fs::write(workflows_dir.join("ci.yml"), &workflow).unwrap();

    // Manifest already has checkout@v4
    let mut manifest = Manifest::default();
    manifest.set(ActionId::from("actions/checkout"), Specifier::from_v1("v4"));

    // Lock already has the entry fully populated
    let mut lock = Lock::default();
    lock.set(&ResolvedAction::new(
        ActionId::from("actions/checkout"),
        Specifier::from_v1("v4"),
        CommitSha::from(sha),
        "actions/checkout".to_string(),
        Some(RefType::Tag),
        "2026-01-01T00:00:00Z".to_string(),
    ));

    let scanner = FileWorkflowScanner::new(repo_root);

    let result = plan(&manifest, &lock, &NoopRegistry, &scanner, |_| {}).unwrap();

    // Everything is in sync — plan should have no manifest/lock changes
    assert!(
        result.manifest.added.is_empty(),
        "No manifest additions expected, got: {:?}",
        result.manifest.added
    );
    assert!(
        result.manifest.removed.is_empty(),
        "No manifest removals expected, got: {:?}",
        result.manifest.removed
    );
    assert!(
        result.lock.added.is_empty(),
        "No lock additions expected, got: {:?}",
        result.lock.added
    );
    assert!(
        result.lock.removed.is_empty(),
        "No lock removals expected, got: {:?}",
        result.lock.removed
    );
}
