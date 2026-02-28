#![allow(unused_crate_dependencies)]

use std::fs;

use gx_lib::commands::lint;
use gx_lib::config::{Level, LintConfig};
use gx_lib::domain::{ActionId, Lock, Manifest, Version};
use gx_lib::infrastructure::FileWorkflowScanner;

#[test]
fn lint_clean_repo_no_diagnostics() {
    let temp_dir = tempfile::tempdir().unwrap();
    let repo_root = temp_dir.path();
    let workflows_dir = repo_root.join(".github").join("workflows");
    fs::create_dir_all(&workflows_dir).unwrap();

    // Create a clean repo with no workflows
    // This is a minimal test showing lint passes with no workflows to check
    let scanner = FileWorkflowScanner::new(repo_root);
    let workflows = scanner.scan_all_located().unwrap();
    let action_set = scanner.scan_all().unwrap();

    let manifest = Manifest::default();
    let lock = Lock::default();
    let lint_config = LintConfig::default();

    let (diagnostics, has_errors) =
        lint::run(&manifest, &lock, &workflows, &action_set, &lint_config);

    assert!(
        diagnostics.is_empty(),
        "Empty repo should have no diagnostics"
    );
    assert!(!has_errors, "Empty repo should not have errors");
}

#[test]
fn lint_detects_unpinned_actions() {
    let temp_dir = tempfile::tempdir().unwrap();
    let repo_root = temp_dir.path();
    let workflows_dir = repo_root.join(".github").join("workflows");
    fs::create_dir_all(&workflows_dir).unwrap();

    // Create workflow with tag refs (unpinned)
    let workflow_content = r#"
name: CI
on: [push]
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v3
"#;
    fs::write(workflows_dir.join("ci.yml"), workflow_content).unwrap();

    let mut manifest = Manifest::default();
    manifest.set(ActionId::from("actions/checkout"), Version::from("v4"));
    manifest.set(ActionId::from("actions/setup-node"), Version::from("v3"));

    let lock = Lock::default();

    let scanner = FileWorkflowScanner::new(repo_root);
    let workflows = scanner.scan_all_located().unwrap();
    let action_set = scanner.scan_all().unwrap();
    let lint_config = LintConfig::default();

    let (diagnostics, has_errors) =
        lint::run(&manifest, &lock, &workflows, &action_set, &lint_config);

    let unpinned_count = diagnostics.iter().filter(|d| d.rule == "unpinned").count();
    assert!(unpinned_count > 0, "Should detect unpinned actions");
    assert!(has_errors, "Should have errors for unpinned actions");
    assert!(
        diagnostics.iter().all(|d| d.level == Level::Error),
        "Unpinned should be errors"
    );
}

#[test]
fn lint_detects_unsynced_manifest() {
    let temp_dir = tempfile::tempdir().unwrap();
    let repo_root = temp_dir.path();
    let workflows_dir = repo_root.join(".github").join("workflows");
    fs::create_dir_all(&workflows_dir).unwrap();

    // Workflow uses actions/cache
    let workflow_content = r#"
name: CI
on: [push]
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/cache@abc123 # v3
"#;
    fs::write(workflows_dir.join("ci.yml"), workflow_content).unwrap();

    // Manifest doesn't list actions/cache
    let manifest = Manifest::default();
    let lock = Lock::default();

    let scanner = FileWorkflowScanner::new(repo_root);
    let workflows = scanner.scan_all_located().unwrap();
    let action_set = scanner.scan_all().unwrap();
    let lint_config = LintConfig::default();

    let (diagnostics, has_errors) =
        lint::run(&manifest, &lock, &workflows, &action_set, &lint_config);

    let unsynced_count = diagnostics
        .iter()
        .filter(|d| d.rule == "unsynced-manifest")
        .count();
    assert!(unsynced_count > 0, "Should detect unsynced manifest");
    assert!(has_errors, "Should have errors for unsynced manifest");
}

#[test]
fn lint_respects_disabled_rules() {
    let temp_dir = tempfile::tempdir().unwrap();
    let repo_root = temp_dir.path();
    let workflows_dir = repo_root.join(".github").join("workflows");
    fs::create_dir_all(&workflows_dir).unwrap();

    // Workflow has unpinned action
    let workflow_content = r#"
name: CI
on: [push]
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
"#;
    fs::write(workflows_dir.join("ci.yml"), workflow_content).unwrap();

    let mut manifest = Manifest::default();
    manifest.set(ActionId::from("actions/checkout"), Version::from("v4"));

    let lock = Lock::default();

    let scanner = FileWorkflowScanner::new(repo_root);
    let workflows = scanner.scan_all_located().unwrap();
    let action_set = scanner.scan_all().unwrap();

    // Disable unpinned rule
    let mut lint_config = LintConfig::default();
    lint_config.rules.insert(
        "unpinned".to_string(),
        gx_lib::config::RuleConfig {
            level: Level::Off,
            ignore: vec![],
        },
    );

    let (diagnostics, has_errors) =
        lint::run(&manifest, &lock, &workflows, &action_set, &lint_config);

    let unpinned_count = diagnostics.iter().filter(|d| d.rule == "unpinned").count();
    assert_eq!(
        unpinned_count, 0,
        "Disabled rule should not produce diagnostics"
    );
    assert!(!has_errors, "No errors when rules are disabled");
}

#[test]
fn lint_ignores_matching_targets() {
    let temp_dir = tempfile::tempdir().unwrap();
    let repo_root = temp_dir.path();
    let workflows_dir = repo_root.join(".github").join("workflows");
    fs::create_dir_all(&workflows_dir).unwrap();

    // Workflow has unpinned action
    let workflow_content = r#"
name: CI
on: [push]
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
"#;
    fs::write(workflows_dir.join("ci.yml"), workflow_content).unwrap();

    let mut manifest = Manifest::default();
    manifest.set(ActionId::from("actions/checkout"), Version::from("v4"));

    let lock = Lock::default();

    let scanner = FileWorkflowScanner::new(repo_root);
    let workflows = scanner.scan_all_located().unwrap();
    let action_set = scanner.scan_all().unwrap();

    // Ignore actions/checkout
    let mut lint_config = LintConfig::default();
    lint_config.rules.insert(
        "unpinned".to_string(),
        gx_lib::config::RuleConfig {
            level: Level::Error,
            ignore: vec![gx_lib::config::IgnoreTarget {
                action: Some("actions/checkout".to_string()),
                workflow: None,
                job: None,
            }],
        },
    );

    let (diagnostics, has_errors) =
        lint::run(&manifest, &lock, &workflows, &action_set, &lint_config);

    let unpinned_count = diagnostics.iter().filter(|d| d.rule == "unpinned").count();
    assert_eq!(
        unpinned_count, 0,
        "Ignored action should not produce diagnostics"
    );
    assert!(!has_errors, "No errors when action is ignored");
}
