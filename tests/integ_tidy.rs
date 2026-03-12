#![allow(unused_crate_dependencies)]

mod common;

use common::registries::{AuthRequiredRegistry, FakeRegistry};
use common::setup::{create_empty_manifest, create_test_repo};
use gx::domain::manifest::Manifest;
use gx::domain::resolution::VersionRegistry;
use gx::infra::lock::{self, apply_lock_diff};
use gx::infra::manifest::patch::apply_manifest_diff;
use gx::infra::manifest::{self};
use gx::infra::workflow_scan::FileScanner as FileWorkflowScanner;
use gx::infra::workflow_update::FileUpdater as FileWorkflowUpdater;
use gx::tidy;
use std::fs;
use std::io::Write;
use std::path::Path;
use tempfile::TempDir;

/// Helper to run tidy with a mock registry (plan+apply path).
fn run_tidy_with_registry<R: VersionRegistry + Clone>(
    repo_root: &Path,
    registry: &R,
) -> Result<(), gx::tidy::RunError> {
    let manifest_path = repo_root.join(".github").join("gx.toml");
    let lock_path = repo_root.join(".github").join("gx.lock");
    let scanner = FileWorkflowScanner::new(repo_root);
    let updater = FileWorkflowUpdater::new(repo_root);
    let has_manifest = manifest_path.exists();

    let manifest = if has_manifest {
        manifest::parse(&manifest_path)?.value
    } else {
        Manifest::default()
    };
    let lock = lock::parse(&lock_path)?.value;

    let plan = tidy::plan(&manifest, &lock, registry, &scanner, |_| {})?;

    if !plan.is_empty() {
        if has_manifest {
            apply_manifest_diff(&manifest_path, &plan.manifest)?;
            if lock_path.exists() {
                apply_lock_diff(&lock_path, &plan.lock)?;
            } else {
                lock::create(&lock_path, &plan.lock)?;
            }
        }
        tidy::apply_workflow_patches(&updater, &plan.workflows, &plan.corrections)?;
    }

    Ok(())
}

fn run_tidy(repo_root: &Path) -> Result<(), gx::tidy::RunError> {
    run_tidy_with_registry(repo_root, &FakeRegistry::new())
}

#[test]
fn test_gx_tidy_memory_only_mode_no_manifest_created() {
    let temp_dir = TempDir::new().unwrap();
    let root = create_test_repo(&temp_dir);

    let workflow_content = "name: CI
on: push
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v3
";
    let workflow_path = root.join(".github").join("workflows").join("ci.yml");
    let mut workflow_file = fs::File::create(&workflow_path).unwrap();
    workflow_file
        .write_all(workflow_content.as_bytes())
        .unwrap();

    let result = run_tidy(&root);
    assert!(result.is_ok());

    let manifest_path = root.join(".github").join("gx.toml");
    assert!(
        !manifest_path.exists(),
        "Manifest should not be created in memory-only mode"
    );

    let lock_path = root.join(".github").join("gx.lock");
    assert!(
        !lock_path.exists(),
        "Lock file should not be created in memory-only mode"
    );

    let workflow_content_after = fs::read_to_string(&workflow_path).unwrap();
    let checkout_sha = FakeRegistry::fake_sha("actions/checkout", "v4");
    let node_sha = FakeRegistry::fake_sha("actions/setup-node", "v3");
    assert!(
        workflow_content_after.contains(&format!("actions/checkout@{checkout_sha} # v4")),
        "Expected checkout SHA in workflow, got:\n{workflow_content_after}"
    );
    assert!(
        workflow_content_after.contains(&format!("actions/setup-node@{node_sha} # v3")),
        "Expected setup-node SHA in workflow, got:\n{workflow_content_after}"
    );
}

#[test]
fn test_gx_tidy_file_mode_creates_manifest_from_workflows() {
    let temp_dir = TempDir::new().unwrap();
    let root = create_test_repo(&temp_dir);

    create_empty_manifest(&root);

    let workflow_content = "name: CI
on: push
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v3
";
    let workflow_path = root.join(".github").join("workflows").join("ci.yml");
    let mut workflow_file = fs::File::create(&workflow_path).unwrap();
    workflow_file
        .write_all(workflow_content.as_bytes())
        .unwrap();

    let result = run_tidy(&root);
    assert!(result.is_ok(), "tidy failed: {result:?}");

    let manifest_path = root.join(".github").join("gx.toml");
    assert!(manifest_path.exists());

    let manifest_content = fs::read_to_string(&manifest_path).unwrap();
    assert!(manifest_content.contains("actions/checkout"));
    assert!(manifest_content.contains("^4"));
    assert!(manifest_content.contains("actions/setup-node"));
    assert!(manifest_content.contains("^3"));
}

#[test]
fn test_gx_tidy_updates_workflows_from_manifest() {
    let temp_dir = TempDir::new().unwrap();
    let root = create_test_repo(&temp_dir);

    let manifest_content = r#"
[actions]
"actions/checkout" = "v4"
"actions/setup-node" = "v4"
"#;
    let manifest_path = root.join(".github").join("gx.toml");
    let mut manifest_file = fs::File::create(&manifest_path).unwrap();
    manifest_file
        .write_all(manifest_content.as_bytes())
        .unwrap();

    let workflow_content = "name: CI
on: push
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      - uses: actions/setup-node@v3
";
    let workflow_path = root.join(".github").join("workflows").join("ci.yml");
    let mut workflow_file = fs::File::create(&workflow_path).unwrap();
    workflow_file
        .write_all(workflow_content.as_bytes())
        .unwrap();

    let result = run_tidy(&root);
    assert!(result.is_ok());

    let updated_content = fs::read_to_string(&workflow_path).unwrap();
    let checkout_sha = FakeRegistry::fake_sha("actions/checkout", "v4");
    let node_sha = FakeRegistry::fake_sha("actions/setup-node", "v4");
    assert!(
        updated_content.contains(&format!("actions/checkout@{checkout_sha} # v4")),
        "Expected checkout with SHA, got:\n{updated_content}"
    );
    assert!(
        updated_content.contains(&format!("actions/setup-node@{node_sha} # v4")),
        "Expected setup-node with SHA, got:\n{updated_content}"
    );
    assert!(!updated_content.contains("@v3"));
}

#[test]
fn test_gx_tidy_removes_unused_actions() {
    let temp_dir = TempDir::new().unwrap();
    let root = create_test_repo(&temp_dir);

    let manifest_content = r#"
[actions]
"actions/checkout" = "v4"
"actions/unused-action" = "v1"
"#;
    let manifest_path = root.join(".github").join("gx.toml");
    let mut manifest_file = fs::File::create(&manifest_path).unwrap();
    manifest_file
        .write_all(manifest_content.as_bytes())
        .unwrap();

    let workflow_content = "name: CI
jobs:
  build:
    steps:
      - uses: actions/checkout@v4
";
    let workflow_path = root.join(".github").join("workflows").join("ci.yml");
    let mut workflow_file = fs::File::create(&workflow_path).unwrap();
    workflow_file
        .write_all(workflow_content.as_bytes())
        .unwrap();

    let result = run_tidy(&root);
    assert!(result.is_ok());

    let updated_manifest = fs::read_to_string(&manifest_path).unwrap();
    assert!(updated_manifest.contains("actions/checkout"));
    assert!(!updated_manifest.contains("actions/unused-action"));
}

#[test]
fn test_gx_tidy_adds_missing_actions() {
    let temp_dir = TempDir::new().unwrap();
    let root = create_test_repo(&temp_dir);

    let manifest_content = r#"
[actions]
"actions/checkout" = "v4"
"#;
    let manifest_path = root.join(".github").join("gx.toml");
    let mut manifest_file = fs::File::create(&manifest_path).unwrap();
    manifest_file
        .write_all(manifest_content.as_bytes())
        .unwrap();

    let workflow_content = "name: CI
jobs:
  build:
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v3
      - uses: docker/build-push-action@v5
";
    let workflow_path = root.join(".github").join("workflows").join("ci.yml");
    let mut workflow_file = fs::File::create(&workflow_path).unwrap();
    workflow_file
        .write_all(workflow_content.as_bytes())
        .unwrap();

    let result = run_tidy(&root);
    assert!(result.is_ok());

    let updated_manifest = fs::read_to_string(&manifest_path).unwrap();
    assert!(updated_manifest.contains("actions/checkout"));
    assert!(updated_manifest.contains("actions/setup-node"));
    assert!(updated_manifest.contains("docker/build-push-action"));
}

#[test]
fn test_gx_tidy_preserves_existing_versions() {
    let temp_dir = TempDir::new().unwrap();
    let root = create_test_repo(&temp_dir);

    let manifest_content = r#"
[actions]
"actions/checkout" = "v4"
"#;
    let manifest_path = root.join(".github").join("gx.toml");
    let mut manifest_file = fs::File::create(&manifest_path).unwrap();
    manifest_file
        .write_all(manifest_content.as_bytes())
        .unwrap();

    let workflow_content = "name: CI
jobs:
  build:
    steps:
      - uses: actions/checkout@v3
";
    let workflow_path = root.join(".github").join("workflows").join("ci.yml");
    let mut workflow_file = fs::File::create(&workflow_path).unwrap();
    workflow_file
        .write_all(workflow_content.as_bytes())
        .unwrap();

    let result = run_tidy(&root);
    assert!(result.is_ok());

    let updated_manifest = fs::read_to_string(&manifest_path).unwrap();
    assert!(updated_manifest.contains("\"actions/checkout\" = \"v4\""));

    let updated_workflow = fs::read_to_string(&workflow_path).unwrap();
    let checkout_sha = FakeRegistry::fake_sha("actions/checkout", "v4");
    assert!(
        updated_workflow.contains(&format!("actions/checkout@{checkout_sha} # v4")),
        "Expected checkout with SHA, got:\n{updated_workflow}"
    );
}

#[test]
fn test_gx_tidy_multiple_workflows() {
    let temp_dir = TempDir::new().unwrap();
    let root = create_test_repo(&temp_dir);

    create_empty_manifest(&root);

    let ci_content = "name: CI
jobs:
  build:
    steps:
      - uses: actions/checkout@v4
";
    let ci_path = root.join(".github").join("workflows").join("ci.yml");
    let mut ci_file = fs::File::create(&ci_path).unwrap();
    ci_file.write_all(ci_content.as_bytes()).unwrap();

    let deploy_content = "name: Deploy
jobs:
  deploy:
    steps:
      - uses: docker/build-push-action@v5
";
    let deploy_path = root.join(".github").join("workflows").join("deploy.yml");
    let mut deploy_file = fs::File::create(&deploy_path).unwrap();
    deploy_file.write_all(deploy_content.as_bytes()).unwrap();

    let result = run_tidy(&root);
    assert!(result.is_ok());

    let manifest_path = root.join(".github").join("gx.toml");
    let manifest_content = fs::read_to_string(&manifest_path).unwrap();
    assert!(manifest_content.contains("actions/checkout"));
    assert!(manifest_content.contains("docker/build-push-action"));
}

#[test]
fn test_gx_tidy_skips_local_actions() {
    let temp_dir = TempDir::new().unwrap();
    let root = create_test_repo(&temp_dir);

    create_empty_manifest(&root);

    let workflow_content = "name: CI
jobs:
  build:
    steps:
      - uses: actions/checkout@v4
      - uses: ./local/action
      - uses: ./.github/actions/my-action
";
    let workflow_path = root.join(".github").join("workflows").join("ci.yml");
    let mut workflow_file = fs::File::create(&workflow_path).unwrap();
    workflow_file
        .write_all(workflow_content.as_bytes())
        .unwrap();

    let result = run_tidy(&root);
    assert!(result.is_ok());

    let manifest_path = root.join(".github").join("gx.toml");
    let manifest_content = fs::read_to_string(&manifest_path).unwrap();
    assert!(manifest_content.contains("actions/checkout"));
    assert!(!manifest_content.contains("./local"));
    assert!(!manifest_content.contains(".github/actions"));
}

#[test]
fn test_gx_tidy_no_workflows() {
    let temp_dir = TempDir::new().unwrap();
    let root = create_test_repo(&temp_dir);

    let result = run_tidy(&root);
    assert!(result.is_ok());
}

#[test]
fn test_gx_tidy_workflow_without_actions() {
    let temp_dir = TempDir::new().unwrap();
    let root = create_test_repo(&temp_dir);

    let workflow_content = r#"name: CI
on: push
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - run: echo "Hello"
"#;
    let workflow_path = root.join(".github").join("workflows").join("ci.yml");
    let mut workflow_file = fs::File::create(&workflow_path).unwrap();
    workflow_file
        .write_all(workflow_content.as_bytes())
        .unwrap();

    let result = run_tidy(&root);
    assert!(result.is_ok());
}

#[test]
fn test_gx_tidy_multiple_versions_picks_highest() {
    let temp_dir = TempDir::new().unwrap();
    let root = create_test_repo(&temp_dir);

    create_empty_manifest(&root);

    let workflow_content = "name: CI
jobs:
  build:
    steps:
      - uses: actions/checkout@v4
  test:
    steps:
      - uses: actions/checkout@v3
";
    let workflow_path = root.join(".github").join("workflows").join("ci.yml");
    let mut workflow_file = fs::File::create(&workflow_path).unwrap();
    workflow_file
        .write_all(workflow_content.as_bytes())
        .unwrap();

    let result = run_tidy(&root);
    assert!(result.is_ok());

    let manifest_path = root.join(".github").join("gx.toml");
    let manifest_content = fs::read_to_string(&manifest_path).unwrap();

    assert!(manifest_content.contains("[actions]"));
    assert!(manifest_content.contains("\"actions/checkout\" = \"^4\""));

    assert!(!manifest_content.contains("[workflows"));
}

#[test]
fn test_gx_tidy_multiple_workflows_unified_version() {
    let temp_dir = TempDir::new().unwrap();
    let root = create_test_repo(&temp_dir);

    create_empty_manifest(&root);

    let ci_content = "name: CI
jobs:
  build:
    steps:
      - uses: actions/checkout@v4
";
    let ci_path = root.join(".github").join("workflows").join("ci.yml");
    let mut ci_file = fs::File::create(&ci_path).unwrap();
    ci_file.write_all(ci_content.as_bytes()).unwrap();

    let deploy_content = "name: Deploy
jobs:
  deploy:
    steps:
      - uses: actions/checkout@v3
";
    let deploy_path = root.join(".github").join("workflows").join("deploy.yml");
    let mut deploy_file = fs::File::create(&deploy_path).unwrap();
    deploy_file.write_all(deploy_content.as_bytes()).unwrap();

    let result = run_tidy(&root);
    assert!(result.is_ok());

    let manifest_path = root.join(".github").join("gx.toml");
    let manifest_content = fs::read_to_string(&manifest_path).unwrap();

    assert!(manifest_content.contains("\"actions/checkout\" = \"^4\""));
    assert!(!manifest_content.contains("[workflows"));
}

#[test]
fn test_gx_tidy_idempotent() {
    let temp_dir = TempDir::new().unwrap();
    let root = create_test_repo(&temp_dir);

    create_empty_manifest(&root);

    let workflow_content = "name: CI
jobs:
  build:
    steps:
      - uses: actions/checkout@v4
";
    let workflow_path = root.join(".github").join("workflows").join("ci.yml");
    let mut workflow_file = fs::File::create(&workflow_path).unwrap();
    workflow_file
        .write_all(workflow_content.as_bytes())
        .unwrap();

    let result1 = run_tidy(&root);
    assert!(result1.is_ok());

    let manifest_after_first = fs::read_to_string(root.join(".github").join("gx.toml")).unwrap();
    let workflow_after_first = fs::read_to_string(&workflow_path).unwrap();

    let result2 = run_tidy(&root);
    assert!(result2.is_ok());

    let manifest_after_second = fs::read_to_string(root.join(".github").join("gx.toml")).unwrap();
    let workflow_after_second = fs::read_to_string(&workflow_path).unwrap();

    assert_eq!(manifest_after_first, manifest_after_second);
    assert_eq!(workflow_after_first, workflow_after_second);
}

#[test]
fn test_gx_tidy_with_sha_and_comment() {
    let temp_dir = TempDir::new().unwrap();
    let root = create_test_repo(&temp_dir);

    create_empty_manifest(&root);

    let workflow_content = "name: CI
jobs:
  build:
    steps:
      - uses: actions/checkout@abc123def456 # v4
      - uses: actions/setup-node@xyz789 #v3
";
    let workflow_path = root.join(".github").join("workflows").join("ci.yml");
    let mut workflow_file = fs::File::create(&workflow_path).unwrap();
    workflow_file
        .write_all(workflow_content.as_bytes())
        .unwrap();

    let result = run_tidy(&root);
    assert!(result.is_ok());

    let manifest_path = root.join(".github").join("gx.toml");
    let manifest_content = fs::read_to_string(&manifest_path).unwrap();

    assert!(manifest_content.contains("\"actions/checkout\" = \"^4\""));
    assert!(manifest_content.contains("\"actions/setup-node\" = \"^3\""));

    assert!(!manifest_content.contains("abc123def456"));
    assert!(!manifest_content.contains("xyz789"));
}

#[test]
fn test_gx_tidy_real_world_workflow_format() {
    let temp_dir = TempDir::new().unwrap();
    let root = create_test_repo(&temp_dir);

    create_empty_manifest(&root);

    let workflow_content = "on:
  pull_request:

permissions:
  contents: read

jobs:
  test_windows:
    runs-on: windows-2025
    steps:
      - name: Checkout repository
        uses: actions/checkout@8e8c483db84b4bee98b60c0593521ed34d9990e8 # v6.0.1

      - name: Login to Docker Hub
        uses: docker/login-action@5e57cd118135c172c3672efd75eb46360885c0ef # v3.6.0
";
    let workflow_path = root.join(".github").join("workflows").join("windows.yml");
    let mut workflow_file = fs::File::create(&workflow_path).unwrap();
    workflow_file
        .write_all(workflow_content.as_bytes())
        .unwrap();

    let registry = FakeRegistry::new()
        .with_sha_tags(
            "actions/checkout",
            "8e8c483db84b4bee98b60c0593521ed34d9990e8",
            vec!["v6", "v6.0.1"],
        )
        .with_sha_tags(
            "docker/login-action",
            "5e57cd118135c172c3672efd75eb46360885c0ef",
            vec!["v3", "v3.6.0"],
        );
    let result = run_tidy_with_registry(&root, &registry);
    assert!(result.is_ok(), "tidy failed: {result:?}");
    let manifest_path = root.join(".github").join("gx.toml");

    let manifest_content = fs::read_to_string(&manifest_path).unwrap();

    assert!(
        manifest_content.contains("\"actions/checkout\" = \"~6.0.1\""),
        "Expected ~6.0.1 in manifest, got: {manifest_content}"
    );
    assert!(
        manifest_content.contains("\"docker/login-action\" = \"~3.6.0\""),
        "Expected ~3.6.0 in manifest, got: {manifest_content}"
    );

    assert!(!manifest_content.contains("8e8c483db84b4bee98b60c0593521ed34d9990e8"));
    assert!(!manifest_content.contains("5e57cd118135c172c3672efd75eb46360885c0ef"));
}

#[test]
fn test_gx_tidy_tag_not_resolved_without_token() {
    let temp_dir = TempDir::new().unwrap();
    let root = create_test_repo(&temp_dir);

    create_empty_manifest(&root);

    let workflow_content = "name: CI
on: push
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
";
    let workflow_path = root.join(".github").join("workflows").join("ci.yml");
    let mut workflow_file = fs::File::create(&workflow_path).unwrap();
    workflow_file
        .write_all(workflow_content.as_bytes())
        .unwrap();

    let result = run_tidy_with_registry(&root, &AuthRequiredRegistry);

    assert!(
        result.is_ok(),
        "tidy should succeed with recoverable errors (auth required), got: {result:?}"
    );
}

#[test]
fn test_gx_tidy_resolves_tag_to_sha() {
    let temp_dir = TempDir::new().unwrap();
    let root = create_test_repo(&temp_dir);

    create_empty_manifest(&root);

    let workflow_content = "name: CI
on: push
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
";
    let workflow_path = root.join(".github").join("workflows").join("ci.yml");
    let mut workflow_file = fs::File::create(&workflow_path).unwrap();
    workflow_file
        .write_all(workflow_content.as_bytes())
        .unwrap();

    let result = run_tidy(&root);
    assert!(result.is_ok(), "tidy failed: {result:?}");

    let lock_path = root.join(".github").join("gx.lock");

    let updated_workflow = fs::read_to_string(&workflow_path).unwrap();

    let checkout_sha = FakeRegistry::fake_sha("actions/checkout", "v4");
    let expected_checkout = format!("actions/checkout@{checkout_sha} # v4");
    assert!(
        updated_workflow.contains(&expected_checkout),
        "Expected workflow to contain '{expected_checkout}', got:\n{updated_workflow}"
    );

    let toolchain_sha = FakeRegistry::fake_sha("dtolnay/rust-toolchain", "stable");
    let expected_toolchain = format!("dtolnay/rust-toolchain@{toolchain_sha} # stable");
    assert!(
        updated_workflow.contains(&expected_toolchain),
        "Expected workflow to contain '{expected_toolchain}', got:\n{updated_workflow}"
    );

    let lock_content = fs::read_to_string(&lock_path).unwrap();
    assert!(
        lock_content.contains(&checkout_sha),
        "Expected lock to contain checkout SHA, got:\n{lock_content}"
    );
}

#[test]
fn test_gx_tidy_respects_override_for_specific_workflow() {
    let temp_dir = TempDir::new().unwrap();
    let root = create_test_repo(&temp_dir);

    let manifest_content = r#"
[actions]
"actions/checkout" = "v4"

[actions.overrides]
"actions/checkout" = [
  { workflow = ".github/workflows/deploy.yml", version = "v3" },
]
"#;
    let manifest_path = root.join(".github").join("gx.toml");
    fs::write(&manifest_path, manifest_content).unwrap();

    let ci_content = "name: CI\njobs:\n  build:\n    steps:\n      - uses: actions/checkout@v4\n";
    let deploy_content =
        "name: Deploy\njobs:\n  deploy:\n    steps:\n      - uses: actions/checkout@v3\n";
    fs::write(root.join(".github/workflows/ci.yml"), ci_content).unwrap();
    fs::write(root.join(".github/workflows/deploy.yml"), deploy_content).unwrap();

    let result = run_tidy(&root);
    assert!(result.is_ok(), "tidy failed: {:?}", result.err());

    let ci_updated = fs::read_to_string(root.join(".github/workflows/ci.yml")).unwrap();
    let checkout_v4_sha = FakeRegistry::fake_sha("actions/checkout", "v4");
    assert!(
        ci_updated.contains(&format!("actions/checkout@{checkout_v4_sha} # v4")),
        "ci.yml should use v4 SHA, got:\n{ci_updated}"
    );

    let deploy_updated = fs::read_to_string(root.join(".github/workflows/deploy.yml")).unwrap();
    let checkout_v3_sha = FakeRegistry::fake_sha("actions/checkout", "v3");
    assert!(
        deploy_updated.contains(&format!("actions/checkout@{checkout_v3_sha} # v3")),
        "deploy.yml should use v3 SHA from override, got:\n{deploy_updated}"
    );
}

#[test]
fn test_gx_tidy_override_job_level() {
    let temp_dir = TempDir::new().unwrap();
    let root = create_test_repo(&temp_dir);

    let manifest_content = r#"
[actions]
"actions/checkout" = "v4"

[actions.overrides]
"actions/checkout" = [
  { workflow = ".github/workflows/ci.yml", job = "legacy-build", version = "v3" },
]
"#;
    fs::write(root.join(".github/gx.toml"), manifest_content).unwrap();

    let ci_content = "name: CI
jobs:
  build:
    steps:
      - uses: actions/checkout@v4
  legacy-build:
    steps:
      - uses: actions/checkout@v3
";
    fs::write(root.join(".github/workflows/ci.yml"), ci_content).unwrap();

    let result = run_tidy(&root);
    assert!(result.is_ok(), "{:?}", result.err());

    let lock_content = fs::read_to_string(root.join(".github/gx.lock")).unwrap();
    let v4_sha = FakeRegistry::fake_sha("actions/checkout", "v4");
    let v3_sha = FakeRegistry::fake_sha("actions/checkout", "v3");
    assert!(lock_content.contains(&v4_sha), "Lock should have v4 SHA");
    assert!(lock_content.contains(&v3_sha), "Lock should have v3 SHA");
}

#[test]
fn test_gx_tidy_removes_stale_override() {
    let temp_dir = TempDir::new().unwrap();
    let root = create_test_repo(&temp_dir);

    let manifest_content = r#"
[actions]
"actions/checkout" = "v4"

[actions.overrides]
"actions/checkout" = [
  { workflow = ".github/workflows/old-workflow.yml", version = "v3" },
]
"#;
    fs::write(root.join(".github/gx.toml"), manifest_content).unwrap();
    fs::write(
        root.join(".github/workflows/ci.yml"),
        "name: CI\njobs:\n  build:\n    steps:\n      - uses: actions/checkout@v4\n",
    )
    .unwrap();

    let result = run_tidy(&root);
    assert!(result.is_ok(), "{:?}", result.err());

    let manifest_content = fs::read_to_string(root.join(".github/gx.toml")).unwrap();
    assert!(
        !manifest_content.contains("old-workflow.yml"),
        "Stale override should be removed, got:\n{manifest_content}"
    );
}
