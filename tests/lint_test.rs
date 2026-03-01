#![allow(unused_crate_dependencies)]

use std::fs;

use gx::commands::lint;
use gx::config::{Level, LintConfig};
use gx::domain::{
    ActionId, CommitSha, Lock, Manifest, RefType, ResolvedAction, Version, WorkflowActionSet,
};
use gx::infrastructure::FileWorkflowScanner;

#[test]
fn lint_clean_repo_no_diagnostics() {
    let temp_dir = tempfile::tempdir().unwrap();
    let repo_root = temp_dir.path();
    let workflows_dir = repo_root.join(".github").join("workflows");
    fs::create_dir_all(&workflows_dir).unwrap();

    // Create a clean repo with no workflows
    let scanner = FileWorkflowScanner::new(repo_root);
    let workflows = scanner.scan_all_located().unwrap();
    let action_set = WorkflowActionSet::from_located(&workflows);

    let manifest = Manifest::default();
    let lock = Lock::default();
    let lint_config = LintConfig::default();

    let diagnostics =
        lint::run(&manifest, &lock, &workflows, &action_set, &lint_config).expect("Should succeed");

    assert!(
        diagnostics.is_empty(),
        "Empty repo should have no diagnostics"
    );
}

#[test]
fn lint_detects_unpinned_actions() {
    let temp_dir = tempfile::tempdir().unwrap();
    let repo_root = temp_dir.path();
    let workflows_dir = repo_root.join(".github").join("workflows");
    fs::create_dir_all(&workflows_dir).unwrap();

    // Create workflow with tag refs (unpinned)
    let workflow_content = "
name: CI
on: [push]
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v3
";
    fs::write(workflows_dir.join("ci.yml"), workflow_content).unwrap();

    let mut manifest = Manifest::default();
    manifest.set(ActionId::from("actions/checkout"), Version::from("v4"));
    manifest.set(ActionId::from("actions/setup-node"), Version::from("v3"));

    let lock = Lock::default();

    let scanner = FileWorkflowScanner::new(repo_root);
    let workflows = scanner.scan_all_located().unwrap();
    let action_set = WorkflowActionSet::from_located(&workflows);
    let lint_config = LintConfig::default();

    let diagnostics =
        lint::run(&manifest, &lock, &workflows, &action_set, &lint_config).expect("Should succeed");

    let unpinned_count = diagnostics.iter().filter(|d| d.rule == "unpinned").count();
    assert!(unpinned_count > 0, "Should detect unpinned actions");
    assert!(
        diagnostics.iter().any(|d| d.level == Level::Error),
        "Should have error-level diagnostics"
    );
}

#[test]
fn lint_detects_unsynced_manifest() {
    let temp_dir = tempfile::tempdir().unwrap();
    let repo_root = temp_dir.path();
    let workflows_dir = repo_root.join(".github").join("workflows");
    fs::create_dir_all(&workflows_dir).unwrap();

    // Workflow uses actions/cache
    let workflow_content = "
name: CI
on: [push]
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/cache@abc123 # v3
";
    fs::write(workflows_dir.join("ci.yml"), workflow_content).unwrap();

    // Manifest doesn't list actions/cache
    let manifest = Manifest::default();
    let lock = Lock::default();

    let scanner = FileWorkflowScanner::new(repo_root);
    let workflows = scanner.scan_all_located().unwrap();
    let action_set = WorkflowActionSet::from_located(&workflows);
    let lint_config = LintConfig::default();

    let diagnostics =
        lint::run(&manifest, &lock, &workflows, &action_set, &lint_config).expect("Should succeed");

    let unsynced_count = diagnostics
        .iter()
        .filter(|d| d.rule == "unsynced-manifest")
        .count();
    assert!(unsynced_count > 0, "Should detect unsynced manifest");
}

#[test]
fn lint_respects_disabled_rules() {
    let temp_dir = tempfile::tempdir().unwrap();
    let repo_root = temp_dir.path();
    let workflows_dir = repo_root.join(".github").join("workflows");
    fs::create_dir_all(&workflows_dir).unwrap();

    // Workflow has unpinned action
    let workflow_content = "
name: CI
on: [push]
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
";
    fs::write(workflows_dir.join("ci.yml"), workflow_content).unwrap();

    let mut manifest = Manifest::default();
    manifest.set(ActionId::from("actions/checkout"), Version::from("v4"));

    let lock = Lock::default();

    let scanner = FileWorkflowScanner::new(repo_root);
    let workflows = scanner.scan_all_located().unwrap();
    let action_set = WorkflowActionSet::from_located(&workflows);

    // Disable unpinned rule
    let mut lint_config = LintConfig::default();
    lint_config.rules.insert(
        "unpinned".to_string(),
        gx::config::RuleConfig {
            level: Level::Off,
            ignore: vec![],
        },
    );

    let diagnostics =
        lint::run(&manifest, &lock, &workflows, &action_set, &lint_config).expect("Should succeed");

    let unpinned_count = diagnostics.iter().filter(|d| d.rule == "unpinned").count();
    assert_eq!(
        unpinned_count, 0,
        "Disabled rule should not produce diagnostics"
    );
}

#[test]
fn lint_ignores_matching_targets() {
    let temp_dir = tempfile::tempdir().unwrap();
    let repo_root = temp_dir.path();
    let workflows_dir = repo_root.join(".github").join("workflows");
    fs::create_dir_all(&workflows_dir).unwrap();

    // Workflow has unpinned action
    let workflow_content = "
name: CI
on: [push]
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
";
    fs::write(workflows_dir.join("ci.yml"), workflow_content).unwrap();

    let mut manifest = Manifest::default();
    manifest.set(ActionId::from("actions/checkout"), Version::from("v4"));

    let lock = Lock::default();

    let scanner = FileWorkflowScanner::new(repo_root);
    let workflows = scanner.scan_all_located().unwrap();
    let action_set = WorkflowActionSet::from_located(&workflows);

    // Ignore actions/checkout
    let mut lint_config = LintConfig::default();
    lint_config.rules.insert(
        "unpinned".to_string(),
        gx::config::RuleConfig {
            level: Level::Error,
            ignore: vec![gx::config::IgnoreTarget {
                action: Some("actions/checkout".to_string()),
                workflow: None,
                job: None,
            }],
        },
    );

    let diagnostics =
        lint::run(&manifest, &lock, &workflows, &action_set, &lint_config).expect("Should succeed");

    let unpinned_count = diagnostics.iter().filter(|d| d.rule == "unpinned").count();
    assert_eq!(
        unpinned_count, 0,
        "Ignored action should not produce diagnostics"
    );
}

// Task 3.1: Add test for sha-mismatch rule
#[test]
fn lint_sha_mismatch_rule_detects_workflow_sha_not_in_lock() {
    let temp_dir = tempfile::tempdir().unwrap();
    let repo_root = temp_dir.path();
    let workflows_dir = repo_root.join(".github").join("workflows");
    fs::create_dir_all(&workflows_dir).unwrap();

    // Workflow with a bare SHA reference (no version comment)
    let workflow_content = "
name: CI
on: [push]
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@abc123def456789012345678901234567890abcd
";
    fs::write(workflows_dir.join("ci.yml"), workflow_content).unwrap();

    let manifest = Manifest::default();
    // Lock doesn't have this SHA registered as a version
    let lock = Lock::default();

    let scanner = FileWorkflowScanner::new(repo_root);
    let workflows = scanner.scan_all_located().unwrap();
    let action_set = WorkflowActionSet::from_located(&workflows);
    let lint_config = LintConfig::default();

    let diagnostics =
        lint::run(&manifest, &lock, &workflows, &action_set, &lint_config).expect("Should succeed");

    let sha_mismatch = diagnostics
        .iter()
        .filter(|d| d.rule == "sha-mismatch")
        .count();
    assert!(
        sha_mismatch > 0,
        "Should detect sha-mismatch for unregistered SHA"
    );
}

// Task 3.2: Add test for stale-comment rule
#[test]
fn lint_stale_comment_rule_detects_mismatched_version_comment() {
    let temp_dir = tempfile::tempdir().unwrap();
    let repo_root = temp_dir.path();
    let workflows_dir = repo_root.join(".github").join("workflows");
    fs::create_dir_all(&workflows_dir).unwrap();

    // Workflow with a version comment that doesn't match the lock
    let workflow_content = "
name: CI
on: [push]
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@abc123def456789012345678901234567890abcd # v4
";
    fs::write(workflows_dir.join("ci.yml"), workflow_content).unwrap();

    let mut manifest = Manifest::default();
    manifest.set(ActionId::from("actions/checkout"), Version::from("v4"));

    // Lock says the SHA for v4 is different
    let mut lock = Lock::default();
    lock.set(&ResolvedAction::new(
        ActionId::from("actions/checkout"),
        Version::from("v4"),
        CommitSha::from("def456789012345678901234567890abcd123456"),
        "actions/checkout".to_string(),
        RefType::Tag,
        "2026-01-01T00:00:00Z".to_string(),
    ));

    let scanner = FileWorkflowScanner::new(repo_root);
    let workflows = scanner.scan_all_located().unwrap();
    let action_set = WorkflowActionSet::from_located(&workflows);
    let lint_config = LintConfig::default();

    let diagnostics =
        lint::run(&manifest, &lock, &workflows, &action_set, &lint_config).expect("Should succeed");

    let stale_comment = diagnostics
        .iter()
        .filter(|d| d.rule == "stale-comment")
        .count();
    assert!(stale_comment > 0, "Should detect stale-comment");
}

// Task 3.3: Add test for mixed severity output
#[test]
fn lint_mixed_severity_output_errors_and_warnings() {
    let temp_dir = tempfile::tempdir().unwrap();
    let repo_root = temp_dir.path();
    let workflows_dir = repo_root.join(".github").join("workflows");
    fs::create_dir_all(&workflows_dir).unwrap();

    // Workflow with both unpinned action and stale comment
    let workflow_content = "
name: CI
on: [push]
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@abc123def456789012345678901234567890abcd # v3
";
    fs::write(workflows_dir.join("ci.yml"), workflow_content).unwrap();

    let mut manifest = Manifest::default();
    manifest.set(ActionId::from("actions/checkout"), Version::from("v4"));
    manifest.set(ActionId::from("actions/setup-node"), Version::from("v3"));

    // Lock has different SHA for setup-node
    let mut lock = Lock::default();
    lock.set(&ResolvedAction::new(
        ActionId::from("actions/setup-node"),
        Version::from("v3"),
        CommitSha::from("def456789012345678901234567890abcd123456"),
        "actions/setup-node".to_string(),
        RefType::Tag,
        "2026-01-01T00:00:00Z".to_string(),
    ));

    let scanner = FileWorkflowScanner::new(repo_root);
    let workflows = scanner.scan_all_located().unwrap();
    let action_set = WorkflowActionSet::from_located(&workflows);
    let lint_config = LintConfig::default();

    let diagnostics =
        lint::run(&manifest, &lock, &workflows, &action_set, &lint_config).expect("Should succeed");

    let has_errors = diagnostics.iter().any(|d| d.level == Level::Error);
    let has_warnings = diagnostics.iter().any(|d| d.level == Level::Warn);
    assert!(has_errors, "Should have error-level diagnostics");
    assert!(has_warnings, "Should have warning-level diagnostics");
}

// Task 3.4: Add test for warning-only output
#[test]
fn lint_warning_only_output_with_error_rules_disabled() {
    let temp_dir = tempfile::tempdir().unwrap();
    let repo_root = temp_dir.path();
    let workflows_dir = repo_root.join(".github").join("workflows");
    fs::create_dir_all(&workflows_dir).unwrap();

    // Workflow with unpinned action and stale comment
    let workflow_content = "
name: CI
on: [push]
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@abc123def456789012345678901234567890abcd # v3
";
    fs::write(workflows_dir.join("ci.yml"), workflow_content).unwrap();

    let mut manifest = Manifest::default();
    manifest.set(ActionId::from("actions/checkout"), Version::from("v4"));
    manifest.set(ActionId::from("actions/setup-node"), Version::from("v3"));

    // Lock has different SHA for setup-node
    let mut lock = Lock::default();
    lock.set(&ResolvedAction::new(
        ActionId::from("actions/setup-node"),
        Version::from("v3"),
        CommitSha::from("def456789012345678901234567890abcd123456"),
        "actions/setup-node".to_string(),
        RefType::Tag,
        "2026-01-01T00:00:00Z".to_string(),
    ));

    let scanner = FileWorkflowScanner::new(repo_root);
    let workflows = scanner.scan_all_located().unwrap();
    let action_set = WorkflowActionSet::from_located(&workflows);

    // Disable all error rules
    let mut lint_config = LintConfig::default();
    lint_config.rules.insert(
        "unpinned".to_string(),
        gx::config::RuleConfig {
            level: Level::Off,
            ignore: vec![],
        },
    );
    lint_config.rules.insert(
        "sha-mismatch".to_string(),
        gx::config::RuleConfig {
            level: Level::Off,
            ignore: vec![],
        },
    );
    lint_config.rules.insert(
        "unsynced-manifest".to_string(),
        gx::config::RuleConfig {
            level: Level::Off,
            ignore: vec![],
        },
    );

    let diagnostics =
        lint::run(&manifest, &lock, &workflows, &action_set, &lint_config).expect("Should succeed");

    let has_errors = diagnostics.iter().any(|d| d.level == Level::Error);
    let has_warnings = diagnostics.iter().any(|d| d.level == Level::Warn);
    assert!(!has_errors, "Should have no error-level diagnostics");
    assert!(has_warnings, "Should have warning-level diagnostics");
}

// Task 3.5: Add test for workflow with only local actions
#[test]
fn lint_local_actions_produce_no_diagnostics() {
    let temp_dir = tempfile::tempdir().unwrap();
    let repo_root = temp_dir.path();
    let workflows_dir = repo_root.join(".github").join("workflows");
    fs::create_dir_all(&workflows_dir).unwrap();

    // Workflow with only local actions (./path)
    let workflow_content = "
name: CI
on: [push]
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: ./actions/custom
";
    fs::write(workflows_dir.join("ci.yml"), workflow_content).unwrap();

    let manifest = Manifest::default();
    let lock = Lock::default();

    let scanner = FileWorkflowScanner::new(repo_root);
    let workflows = scanner.scan_all_located().unwrap();
    let action_set = WorkflowActionSet::from_located(&workflows);
    let lint_config = LintConfig::default();

    let diagnostics =
        lint::run(&manifest, &lock, &workflows, &action_set, &lint_config).expect("Should succeed");

    assert!(
        diagnostics.is_empty(),
        "Local actions should produce no diagnostics"
    );
}

// Task 3.6: Add test for rule severity override
#[test]
fn lint_rule_severity_override_promote_warn_to_error() {
    let temp_dir = tempfile::tempdir().unwrap();
    let repo_root = temp_dir.path();
    let workflows_dir = repo_root.join(".github").join("workflows");
    fs::create_dir_all(&workflows_dir).unwrap();

    // Workflow with stale comment (which is Warn by default)
    let workflow_content = "
name: CI
on: [push]
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@abc123def456789012345678901234567890abcd # v4
";
    fs::write(workflows_dir.join("ci.yml"), workflow_content).unwrap();

    let mut manifest = Manifest::default();
    manifest.set(ActionId::from("actions/checkout"), Version::from("v4"));

    // Lock has different SHA
    let mut lock = Lock::default();
    lock.set(&ResolvedAction::new(
        ActionId::from("actions/checkout"),
        Version::from("v4"),
        CommitSha::from("def456789012345678901234567890abcd123456"),
        "actions/checkout".to_string(),
        RefType::Tag,
        "2026-01-01T00:00:00Z".to_string(),
    ));

    let scanner = FileWorkflowScanner::new(repo_root);
    let workflows = scanner.scan_all_located().unwrap();
    let action_set = WorkflowActionSet::from_located(&workflows);

    // Promote stale-comment to Error
    let mut lint_config = LintConfig::default();
    lint_config.rules.insert(
        "stale-comment".to_string(),
        gx::config::RuleConfig {
            level: Level::Error,
            ignore: vec![],
        },
    );

    let diagnostics =
        lint::run(&manifest, &lock, &workflows, &action_set, &lint_config).expect("Should succeed");

    let stale_comment_errors = diagnostics
        .iter()
        .filter(|d| d.rule == "stale-comment" && d.level == Level::Error)
        .count();
    assert!(
        stale_comment_errors > 0,
        "Stale-comment should be promoted to Error"
    );
}

// Task 3.7: Add test for ignore scoped to specific workflow
#[test]
fn lint_ignore_scoped_to_specific_workflow() {
    let temp_dir = tempfile::tempdir().unwrap();
    let repo_root = temp_dir.path();
    let workflows_dir = repo_root.join(".github").join("workflows");
    fs::create_dir_all(&workflows_dir).unwrap();

    // Create two workflows with unpinned actions
    let ci_content = "
name: CI
on: [push]
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
";
    fs::write(workflows_dir.join("ci.yml"), ci_content).unwrap();

    let test_content = "
name: Test
on: [push]
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/setup-node@v3
";
    fs::write(workflows_dir.join("test.yml"), test_content).unwrap();

    let mut manifest = Manifest::default();
    manifest.set(ActionId::from("actions/checkout"), Version::from("v4"));
    manifest.set(ActionId::from("actions/setup-node"), Version::from("v3"));

    let lock = Lock::default();

    let scanner = FileWorkflowScanner::new(repo_root);
    let workflows = scanner.scan_all_located().unwrap();
    let action_set = WorkflowActionSet::from_located(&workflows);

    // Ignore unpinned in ci.yml only
    let mut lint_config = LintConfig::default();
    lint_config.rules.insert(
        "unpinned".to_string(),
        gx::config::RuleConfig {
            level: Level::Error,
            ignore: vec![gx::config::IgnoreTarget {
                action: None,
                workflow: Some("ci.yml".to_string()),
                job: None,
            }],
        },
    );

    let diagnostics =
        lint::run(&manifest, &lock, &workflows, &action_set, &lint_config).expect("Should succeed");

    let ci_unpinned = diagnostics
        .iter()
        .filter(|d| {
            d.rule == "unpinned" && d.workflow.as_ref().is_none_or(|w| w.contains("ci.yml"))
        })
        .count();
    let test_unpinned = diagnostics
        .iter()
        .filter(|d| {
            d.rule == "unpinned" && d.workflow.as_ref().is_none_or(|w| w.contains("test.yml"))
        })
        .count();

    assert_eq!(ci_unpinned, 0, "ci.yml unpinned should be ignored");
    assert!(test_unpinned > 0, "test.yml unpinned should not be ignored");
}
