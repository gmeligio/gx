#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "tests use unwrap, indexing, and other patterns freely"
)]

use gx::config::{Level, Lint};
use gx::domain::action::identity::{ActionId, CommitDate, CommitSha, Repository, Version};
use gx::domain::action::resolved::{Commit, ResolvedRef};
use gx::domain::action::spec::Spec as ActionSpec;
use gx::domain::action::specifier::Specifier;
use gx::domain::action::uses_ref::RefType;
use gx::domain::lock::Lock;
use gx::domain::manifest::Manifest;
use gx::infra::workflow_scan::FileScanner as FileWorkflowScanner;
use gx::lint;
use std::fs;

#[test]
fn lint_clean_repo_no_diagnostics() {
    let temp_dir = tempfile::tempdir().unwrap();
    let repo_root = temp_dir.path();
    let workflows_dir = repo_root.join(".github").join("workflows");
    fs::create_dir_all(&workflows_dir).unwrap();

    let scanner = FileWorkflowScanner::new(repo_root);
    let manifest = Manifest::default();
    let lock = Lock::default();
    let lint_config = Lint::default();

    let diagnostics =
        lint::collect_diagnostics(&manifest, &lock, &scanner, &lint_config, &mut |_| {})
            .expect("Should succeed");

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
    manifest.set(ActionId::from("actions/checkout"), Specifier::from_v1("v4"));
    manifest.set(
        ActionId::from("actions/setup-node"),
        Specifier::from_v1("v3"),
    );

    let lock = Lock::default();
    let scanner = FileWorkflowScanner::new(repo_root);
    let lint_config = Lint::default();

    let diagnostics =
        lint::collect_diagnostics(&manifest, &lock, &scanner, &lint_config, &mut |_| {})
            .expect("Should succeed");

    let unpinned_count = diagnostics
        .iter()
        .filter(|d| d.rule == gx::lint::RuleName::Unpinned)
        .count();
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

    let manifest = Manifest::default();
    let lock = Lock::default();
    let scanner = FileWorkflowScanner::new(repo_root);
    let lint_config = Lint::default();

    let diagnostics =
        lint::collect_diagnostics(&manifest, &lock, &scanner, &lint_config, &mut |_| {})
            .expect("Should succeed");

    let unsynced_count = diagnostics
        .iter()
        .filter(|d| d.rule == gx::lint::RuleName::UnsyncedManifest)
        .count();
    assert!(unsynced_count > 0, "Should detect unsynced manifest");
}

#[test]
fn lint_respects_disabled_rules() {
    let temp_dir = tempfile::tempdir().unwrap();
    let repo_root = temp_dir.path();
    let workflows_dir = repo_root.join(".github").join("workflows");
    fs::create_dir_all(&workflows_dir).unwrap();

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
    manifest.set(ActionId::from("actions/checkout"), Specifier::from_v1("v4"));

    let lock = Lock::default();
    let scanner = FileWorkflowScanner::new(repo_root);

    let mut lint_config = Lint::default();
    lint_config.rules.insert(
        gx::lint::RuleName::Unpinned,
        gx::config::Rule {
            level: Level::Off,
            ignore: vec![],
        },
    );

    let diagnostics =
        lint::collect_diagnostics(&manifest, &lock, &scanner, &lint_config, &mut |_| {})
            .expect("Should succeed");

    let unpinned_count = diagnostics
        .iter()
        .filter(|d| d.rule == gx::lint::RuleName::Unpinned)
        .count();
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
    manifest.set(ActionId::from("actions/checkout"), Specifier::from_v1("v4"));

    let lock = Lock::default();
    let scanner = FileWorkflowScanner::new(repo_root);

    let mut lint_config = Lint::default();
    lint_config.rules.insert(
        gx::lint::RuleName::Unpinned,
        gx::config::Rule {
            level: Level::Error,
            ignore: vec![gx::config::IgnoreTarget {
                action: Some("actions/checkout".to_owned()),
                workflow: None,
                job: None,
            }],
        },
    );

    let diagnostics =
        lint::collect_diagnostics(&manifest, &lock, &scanner, &lint_config, &mut |_| {})
            .expect("Should succeed");

    let unpinned_count = diagnostics
        .iter()
        .filter(|d| d.rule == gx::lint::RuleName::Unpinned)
        .count();
    assert_eq!(
        unpinned_count, 0,
        "Ignored action should not produce diagnostics"
    );
}

#[test]
fn lint_sha_mismatch_rule_detects_workflow_sha_not_in_lock() {
    let temp_dir = tempfile::tempdir().unwrap();
    let repo_root = temp_dir.path();
    let workflows_dir = repo_root.join(".github").join("workflows");
    fs::create_dir_all(&workflows_dir).unwrap();

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
    let lock = Lock::default();
    let scanner = FileWorkflowScanner::new(repo_root);
    let lint_config = Lint::default();

    let diagnostics =
        lint::collect_diagnostics(&manifest, &lock, &scanner, &lint_config, &mut |_| {})
            .expect("Should succeed");

    let sha_mismatch = diagnostics
        .iter()
        .filter(|d| d.rule == gx::lint::RuleName::ShaMismatch)
        .count();
    assert!(
        sha_mismatch > 0,
        "Should detect sha-mismatch for unregistered SHA"
    );
}

#[test]
fn lint_stale_comment_rule_detects_mismatched_version_comment() {
    let temp_dir = tempfile::tempdir().unwrap();
    let repo_root = temp_dir.path();
    let workflows_dir = repo_root.join(".github").join("workflows");
    fs::create_dir_all(&workflows_dir).unwrap();

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
    manifest.set(ActionId::from("actions/checkout"), Specifier::from_v1("v4"));

    let mut lock = Lock::default();
    lock.set(
        &ActionSpec::new(ActionId::from("actions/checkout"), Specifier::from_v1("v4")),
        ResolvedRef::Tag(Version::from("v4")),
        Commit {
            sha: CommitSha::from("def456789012345678901234567890abcd123456"),
            repository: Repository::from("actions/checkout"),
            ref_type: Some(RefType::Tag),
            date: CommitDate::from("2026-01-01T00:00:00Z"),
        },
    );

    let scanner = FileWorkflowScanner::new(repo_root);
    let lint_config = Lint::default();

    let diagnostics =
        lint::collect_diagnostics(&manifest, &lock, &scanner, &lint_config, &mut |_| {})
            .expect("Should succeed");

    let stale_comment = diagnostics
        .iter()
        .filter(|d| d.rule == gx::lint::RuleName::StaleComment)
        .count();
    assert!(stale_comment > 0, "Should detect stale-comment");
}

#[test]
fn lint_mixed_severity_output_errors_and_warnings() {
    let temp_dir = tempfile::tempdir().unwrap();
    let repo_root = temp_dir.path();
    let workflows_dir = repo_root.join(".github").join("workflows");
    fs::create_dir_all(&workflows_dir).unwrap();

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
    manifest.set(ActionId::from("actions/checkout"), Specifier::from_v1("v4"));
    manifest.set(
        ActionId::from("actions/setup-node"),
        Specifier::from_v1("v3"),
    );

    let mut lock = Lock::default();
    lock.set(
        &ActionSpec::new(
            ActionId::from("actions/setup-node"),
            Specifier::from_v1("v3"),
        ),
        ResolvedRef::Tag(Version::from("v3")),
        Commit {
            sha: CommitSha::from("def456789012345678901234567890abcd123456"),
            repository: Repository::from("actions/setup-node"),
            ref_type: Some(RefType::Tag),
            date: CommitDate::from("2026-01-01T00:00:00Z"),
        },
    );

    let scanner = FileWorkflowScanner::new(repo_root);
    let lint_config = Lint::default();

    let diagnostics =
        lint::collect_diagnostics(&manifest, &lock, &scanner, &lint_config, &mut |_| {})
            .expect("Should succeed");

    let has_errors = diagnostics.iter().any(|d| d.level == Level::Error);
    let has_warnings = diagnostics.iter().any(|d| d.level == Level::Warn);
    assert!(has_errors, "Should have error-level diagnostics");
    assert!(has_warnings, "Should have warning-level diagnostics");
}

#[test]
fn lint_warning_only_output_with_error_rules_disabled() {
    let temp_dir = tempfile::tempdir().unwrap();
    let repo_root = temp_dir.path();
    let workflows_dir = repo_root.join(".github").join("workflows");
    fs::create_dir_all(&workflows_dir).unwrap();

    // permissions + concurrency added so the new workflow-security rules
    // stay silent — this test is scoped to action-hygiene rules.
    let workflow_content = "
name: CI
on: [push]
permissions:
  contents: read
concurrency:
  group: ci
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@abc123def456789012345678901234567890abcd # v3
";
    fs::write(workflows_dir.join("ci.yml"), workflow_content).unwrap();

    let mut manifest = Manifest::default();
    manifest.set(ActionId::from("actions/checkout"), Specifier::from_v1("v4"));
    manifest.set(
        ActionId::from("actions/setup-node"),
        Specifier::from_v1("v3"),
    );

    let mut lock = Lock::default();
    lock.set(
        &ActionSpec::new(
            ActionId::from("actions/setup-node"),
            Specifier::from_v1("v3"),
        ),
        ResolvedRef::Tag(Version::from("v3")),
        Commit {
            sha: CommitSha::from("def456789012345678901234567890abcd123456"),
            repository: Repository::from("actions/setup-node"),
            ref_type: Some(RefType::Tag),
            date: CommitDate::from("2026-01-01T00:00:00Z"),
        },
    );

    let scanner = FileWorkflowScanner::new(repo_root);

    let mut lint_config = Lint::default();
    lint_config.rules.insert(
        gx::lint::RuleName::Unpinned,
        gx::config::Rule {
            level: Level::Off,
            ignore: vec![],
        },
    );
    lint_config.rules.insert(
        gx::lint::RuleName::ShaMismatch,
        gx::config::Rule {
            level: Level::Off,
            ignore: vec![],
        },
    );
    lint_config.rules.insert(
        gx::lint::RuleName::UnsyncedManifest,
        gx::config::Rule {
            level: Level::Off,
            ignore: vec![],
        },
    );

    let diagnostics =
        lint::collect_diagnostics(&manifest, &lock, &scanner, &lint_config, &mut |_| {})
            .expect("Should succeed");

    let has_errors = diagnostics.iter().any(|d| d.level == Level::Error);
    let has_warnings = diagnostics.iter().any(|d| d.level == Level::Warn);
    assert!(!has_errors, "Should have no error-level diagnostics");
    assert!(has_warnings, "Should have warning-level diagnostics");
}

#[test]
fn lint_local_actions_produce_no_diagnostics() {
    let temp_dir = tempfile::tempdir().unwrap();
    let repo_root = temp_dir.path();
    let workflows_dir = repo_root.join(".github").join("workflows");
    fs::create_dir_all(&workflows_dir).unwrap();

    // permissions + concurrency added so the workflow-security rules stay
    // silent — this test is scoped to action-hygiene rules.
    let workflow_content = "
name: CI
on: [push]
permissions:
  contents: read
concurrency:
  group: ci
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
    let lint_config = Lint::default();

    let diagnostics =
        lint::collect_diagnostics(&manifest, &lock, &scanner, &lint_config, &mut |_| {})
            .expect("Should succeed");

    assert!(
        diagnostics.is_empty(),
        "Local actions should produce no diagnostics, got {diagnostics:?}",
    );
}

#[test]
fn lint_rule_severity_override_promote_warn_to_error() {
    let temp_dir = tempfile::tempdir().unwrap();
    let repo_root = temp_dir.path();
    let workflows_dir = repo_root.join(".github").join("workflows");
    fs::create_dir_all(&workflows_dir).unwrap();

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
    manifest.set(ActionId::from("actions/checkout"), Specifier::from_v1("v4"));

    let mut lock = Lock::default();
    lock.set(
        &ActionSpec::new(ActionId::from("actions/checkout"), Specifier::from_v1("v4")),
        ResolvedRef::Tag(Version::from("v4")),
        Commit {
            sha: CommitSha::from("def456789012345678901234567890abcd123456"),
            repository: Repository::from("actions/checkout"),
            ref_type: Some(RefType::Tag),
            date: CommitDate::from("2026-01-01T00:00:00Z"),
        },
    );

    let scanner = FileWorkflowScanner::new(repo_root);

    let mut lint_config = Lint::default();
    lint_config.rules.insert(
        gx::lint::RuleName::StaleComment,
        gx::config::Rule {
            level: Level::Error,
            ignore: vec![],
        },
    );

    let diagnostics =
        lint::collect_diagnostics(&manifest, &lock, &scanner, &lint_config, &mut |_| {})
            .expect("Should succeed");

    let stale_comment_errors = diagnostics
        .iter()
        .filter(|d| d.rule == gx::lint::RuleName::StaleComment && d.level == Level::Error)
        .count();
    assert!(
        stale_comment_errors > 0,
        "Stale-comment should be promoted to Error"
    );
}

#[test]
fn lint_ignore_scoped_to_specific_workflow() {
    let temp_dir = tempfile::tempdir().unwrap();
    let repo_root = temp_dir.path();
    let workflows_dir = repo_root.join(".github").join("workflows");
    fs::create_dir_all(&workflows_dir).unwrap();

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
    manifest.set(ActionId::from("actions/checkout"), Specifier::from_v1("v4"));
    manifest.set(
        ActionId::from("actions/setup-node"),
        Specifier::from_v1("v3"),
    );

    let lock = Lock::default();
    let scanner = FileWorkflowScanner::new(repo_root);

    let mut lint_config = Lint::default();
    lint_config.rules.insert(
        gx::lint::RuleName::Unpinned,
        gx::config::Rule {
            level: Level::Error,
            ignore: vec![gx::config::IgnoreTarget {
                action: None,
                workflow: Some("ci.yml".to_owned()),
                job: None,
            }],
        },
    );

    let diagnostics =
        lint::collect_diagnostics(&manifest, &lock, &scanner, &lint_config, &mut |_| {})
            .expect("Should succeed");

    let ci_unpinned = diagnostics
        .iter()
        .filter(|d| {
            d.rule == gx::lint::RuleName::Unpinned
                && d.workflow
                    .as_ref()
                    .is_none_or(|w| w.as_str().contains("ci.yml"))
        })
        .count();
    let test_unpinned = diagnostics
        .iter()
        .filter(|d| {
            d.rule == gx::lint::RuleName::Unpinned
                && d.workflow
                    .as_ref()
                    .is_none_or(|w| w.as_str().contains("test.yml"))
        })
        .count();

    assert_eq!(ci_unpinned, 0, "ci.yml unpinned should be ignored");
    assert!(test_unpinned > 0, "test.yml unpinned should not be ignored");
}

// ---------------------------------------------------------------------------
// Workflow-security rules: per-rule `level = "off"` smoke tests.
//
// Each rule has a workflow content that triggers it. Running with the default
// `Lint::default()` config must produce at least one diagnostic for that rule,
// and setting `level = "off"` for the rule must suppress it. Per-rule fixture
// content is intentionally minimal — exhaustive coverage lives in each rule's
// unit tests.
// ---------------------------------------------------------------------------

/// Run lint on a single-workflow repo, returning the diagnostic count for the
/// named rule under both the supplied config and a copy of it with `rule` forced
/// to `Level::Off`. Returns `(default_count, off_count)`.
fn run_off_toggle(
    workflow_content: &str,
    rule: gx::lint::RuleName,
    base_config: Lint,
) -> (usize, usize) {
    let temp_dir = tempfile::tempdir().unwrap();
    let repo_root = temp_dir.path();
    let workflows_dir = repo_root.join(".github").join("workflows");
    fs::create_dir_all(&workflows_dir).unwrap();
    fs::write(workflows_dir.join("ci.yml"), workflow_content).unwrap();

    let manifest = Manifest::default();
    let lock = Lock::default();
    let scanner = FileWorkflowScanner::new(repo_root);

    let on_diags =
        lint::collect_diagnostics(&manifest, &lock, &scanner, &base_config, &mut |_| {}).unwrap();
    let on_count = on_diags.iter().filter(|d| d.rule == rule).count();

    let mut off_config = base_config;
    off_config.rules.insert(
        rule,
        gx::config::Rule {
            level: Level::Off,
            ignore: vec![],
        },
    );
    let off_diags =
        lint::collect_diagnostics(&manifest, &lock, &scanner, &off_config, &mut |_| {}).unwrap();
    let off_count = off_diags.iter().filter(|d| d.rule == rule).count();

    (on_count, off_count)
}

#[test]
fn missing_permissions_can_be_disabled() {
    let workflow = "
name: CI
on: push
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - run: echo hi
";
    let (on, off) = run_off_toggle(
        workflow,
        gx::lint::RuleName::MissingPermissions,
        Lint::default(),
    );
    assert!(on > 0, "missing-permissions should fire by default");
    assert_eq!(off, 0, "level = off must suppress missing-permissions");
}

#[test]
fn excessive_permissions_can_be_disabled() {
    let workflow = "
name: CI
on: push
permissions: write-all
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - run: echo hi
";
    let (on, off) = run_off_toggle(
        workflow,
        gx::lint::RuleName::ExcessivePermissions,
        Lint::default(),
    );
    assert!(on > 0, "excessive-permissions should fire by default");
    assert_eq!(off, 0, "level = off must suppress excessive-permissions");
}

#[test]
fn dangerous_trigger_can_be_disabled() {
    let workflow = "
name: PR-Target
on: pull_request_target
permissions:
  contents: read
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - run: echo hi
";
    let (on, off) = run_off_toggle(
        workflow,
        gx::lint::RuleName::DangerousTrigger,
        Lint::default(),
    );
    assert!(on > 0, "dangerous-trigger should fire by default");
    assert_eq!(off, 0, "level = off must suppress dangerous-trigger");
}

#[test]
fn pr_head_checkout_can_be_disabled() {
    let workflow = "
name: PR-checkout
on: pull_request
permissions:
  contents: read
jobs:
  build:
    runs-on: ubuntu-latest
    permissions:
      contents: write
    steps:
      - uses: actions/checkout@v4
        with:
          ref: ${{ github.event.pull_request.head.sha }}
";
    let (on, off) = run_off_toggle(
        workflow,
        gx::lint::RuleName::PrHeadCheckout,
        Lint::default(),
    );
    assert!(on > 0, "pr-head-checkout should fire by default");
    assert_eq!(off, 0, "level = off must suppress pr-head-checkout");
}

#[test]
fn missing_concurrency_can_be_disabled() {
    let workflow = "
name: CI
on: push
permissions:
  contents: read
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - run: echo hi
";
    let (on, off) = run_off_toggle(
        workflow,
        gx::lint::RuleName::MissingConcurrency,
        Lint::default(),
    );
    assert!(on > 0, "missing-concurrency should fire by default");
    assert_eq!(off, 0, "level = off must suppress missing-concurrency");
}

#[test]
fn unprotected_secrets_can_be_disabled() {
    let workflow = "
name: PR
on: pull_request
permissions:
  contents: read
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: docker/login-action@v3
        with:
          password: ${{ secrets.DOCKER_HUB_TOKEN }}
";
    let (on, off) = run_off_toggle(
        workflow,
        gx::lint::RuleName::UnprotectedSecrets,
        Lint::default(),
    );
    assert!(on > 0, "unprotected-secrets should fire by default");
    assert_eq!(off, 0, "level = off must suppress unprotected-secrets");
}

#[test]
fn dangling_reference_can_be_disabled() {
    let workflow = "
name: CI
on: push
permissions:
  contents: read
concurrency:
  group: ci
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - run: echo hi
  deploy:
    needs: [buld]
    runs-on: ubuntu-latest
    steps:
      - run: echo hi
";
    let (on, off) = run_off_toggle(
        workflow,
        gx::lint::RuleName::DanglingReference,
        Lint::default(),
    );
    assert!(on > 0, "dangling-reference should fire by default");
    assert_eq!(off, 0, "level = off must suppress dangling-reference");
}

#[test]
fn invalid_expression_can_be_disabled() {
    let workflow = "
name: CI
on: push
permissions:
  contents: read
concurrency:
  group: ci
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - run: echo ${{ steps.missing.outputs.x }}
";
    let (on, off) = run_off_toggle(
        workflow,
        gx::lint::RuleName::InvalidExpression,
        Lint::default(),
    );
    assert!(on > 0, "invalid-expression should fire by default");
    assert_eq!(off, 0, "level = off must suppress invalid-expression");
}

#[test]
fn diagnostics_are_stably_sorted_across_workflows_jobs_and_rules() {
    let temp_dir = tempfile::tempdir().unwrap();
    let repo_root = temp_dir.path();
    let workflows_dir = repo_root.join(".github").join("workflows");
    fs::create_dir_all(&workflows_dir).unwrap();

    // Two workflows, each missing both `permissions:` and `concurrency:` (on
    // push) — yields multiple diagnostics across rules per workflow.
    let bare = "
name: X
on: push
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - run: echo hi
";
    // Intentionally write `zebra.yml` first so insertion order != path order.
    fs::write(workflows_dir.join("zebra.yml"), bare).unwrap();
    fs::write(workflows_dir.join("alpha.yml"), bare).unwrap();

    let manifest = Manifest::default();
    let lock = Lock::default();
    let scanner = FileWorkflowScanner::new(repo_root);
    let lint_config = Lint::default();

    let diagnostics =
        lint::collect_diagnostics(&manifest, &lock, &scanner, &lint_config, &mut |_| {}).unwrap();
    assert!(
        diagnostics.len() >= 4,
        "expected ≥4 diagnostics, got {}",
        diagnostics.len()
    );

    // Verify the sort key tuple (workflow_path, job, step, rule) is monotonic.
    let keys: Vec<_> = diagnostics
        .iter()
        .map(|d| {
            (
                d.workflow
                    .as_ref()
                    .map(|w| w.as_str().to_owned())
                    .unwrap_or_default(),
                d.job
                    .as_ref()
                    .map(|j| j.as_str().to_owned())
                    .unwrap_or_default(),
                d.step
                    .map_or(u16::MAX, gx::domain::file::site::StepIndex::as_u16),
                d.rule,
            )
        })
        .collect();
    for pair in keys.windows(2) {
        assert!(
            pair[0] <= pair[1],
            "diagnostics not stably sorted: {pair:?}"
        );
    }

    // alpha.yml must precede zebra.yml in the output.
    let first_alpha = diagnostics.iter().position(|d| {
        d.workflow
            .as_ref()
            .is_some_and(|w| w.as_str().contains("alpha.yml"))
    });
    let first_zebra = diagnostics.iter().position(|d| {
        d.workflow
            .as_ref()
            .is_some_and(|w| w.as_str().contains("zebra.yml"))
    });
    assert!(matches!((first_alpha, first_zebra), (Some(a), Some(z)) if a < z));
}

#[test]
fn lint_config_parses_all_six_new_rule_names() {
    let toml_str = r#"
        [rules]
        missing-permissions = { level = "error" }
        excessive-permissions = { level = "warn" }
        dangerous-trigger = { level = "error", ignore = [{ workflow = ".github/workflows/release.yml" }] }
        pr-head-checkout = { level = "error" }
        missing-concurrency = { level = "off" }
        unprotected-secrets = { level = "error" }
    "#;
    let config: Lint = toml::from_str(toml_str).unwrap();
    assert_eq!(config.rules.len(), 6);
    assert_eq!(
        config.rules[&gx::lint::RuleName::MissingPermissions].level,
        Level::Error
    );
    assert_eq!(
        config.rules[&gx::lint::RuleName::ExcessivePermissions].level,
        Level::Warn
    );
    assert_eq!(
        config.rules[&gx::lint::RuleName::DangerousTrigger]
            .ignore
            .len(),
        1
    );
    assert_eq!(
        config.rules[&gx::lint::RuleName::MissingConcurrency].level,
        Level::Off
    );
}

/// Write a composite action definition at `.github/actions/{name}/action.yml`.
fn write_composite(repo_root: &std::path::Path, name: &str, steps: &str) -> std::path::PathBuf {
    let dir = repo_root.join(".github").join("actions").join(name);
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("action.yml");
    fs::write(
        &path,
        format!("name: Setup\nruns:\n  using: composite\n  steps:\n{steps}"),
    )
    .unwrap();
    path
}

/// A repo with one clean workflow and one composite action holding an unpinned ref.
fn composite_repo(repo_root: &std::path::Path) {
    let workflows_dir = repo_root.join(".github").join("workflows");
    fs::create_dir_all(&workflows_dir).unwrap();
    fs::write(
        workflows_dir.join("ci.yml"),
        "name: CI\non: [push]\npermissions:\n  contents: read\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: ./.github/actions/setup\n",
    )
    .unwrap();
    write_composite(repo_root, "setup", "    - uses: actions/checkout@v4\n");
}

#[test]
fn lint_unpinned_fires_on_composite_step() {
    let temp_dir = tempfile::tempdir().unwrap();
    let repo_root = temp_dir.path();
    composite_repo(repo_root);

    let mut manifest = Manifest::default();
    manifest.set(ActionId::from("actions/checkout"), Specifier::from_v1("v4"));
    let lock = Lock::default();
    let scanner = FileWorkflowScanner::new(repo_root);
    let lint_config = Lint::default();

    let diagnostics =
        lint::collect_diagnostics(&manifest, &lock, &scanner, &lint_config, &mut |_| {})
            .expect("Should succeed");

    let unpinned: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.rule == gx::lint::RuleName::Unpinned)
        .collect();

    assert_eq!(unpinned.len(), 1, "expected one unpinned diagnostic");
    let diag = unpinned[0];
    assert_eq!(
        diag.workflow
            .as_ref()
            .map(gx::domain::file::site::WorkflowPath::as_str),
        Some(".github/actions/setup/action.yml")
    );
    assert!(diag.line.is_some(), "diagnostic should carry a source line");
    assert!(diag.job.is_none(), "composite steps belong to no job");
}

#[test]
fn lint_unsynced_manifest_counts_composite_references() {
    let temp_dir = tempfile::tempdir().unwrap();
    let repo_root = temp_dir.path();
    composite_repo(repo_root);

    let mut manifest = Manifest::default();
    manifest.set(ActionId::from("actions/checkout"), Specifier::from_v1("v4"));
    let lock = Lock::default();
    let scanner = FileWorkflowScanner::new(repo_root);
    let lint_config = Lint::default();

    let diagnostics =
        lint::collect_diagnostics(&manifest, &lock, &scanner, &lint_config, &mut |_| {})
            .expect("Should succeed");

    let unsynced: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.rule == gx::lint::RuleName::UnsyncedManifest)
        .collect();

    assert!(
        unsynced.is_empty(),
        "action referenced from a composite is not orphaned: {unsynced:?}"
    );
}

#[test]
fn lint_workflow_schema_rules_skip_composite_files() {
    let temp_dir = tempfile::tempdir().unwrap();
    let repo_root = temp_dir.path();
    composite_repo(repo_root);

    let manifest = Manifest::default();
    let lock = Lock::default();
    let scanner = FileWorkflowScanner::new(repo_root);
    let lint_config = Lint::default();

    let diagnostics =
        lint::collect_diagnostics(&manifest, &lock, &scanner, &lint_config, &mut |_| {})
            .expect("Should succeed");

    let on_composite: Vec<_> = diagnostics
        .iter()
        .filter(|d| {
            d.workflow
                .as_ref()
                .is_some_and(|w| w.as_str().contains(".github/actions/"))
        })
        .filter(|d| {
            matches!(
                d.rule,
                gx::lint::RuleName::MissingPermissions
                    | gx::lint::RuleName::ExcessivePermissions
                    | gx::lint::RuleName::DangerousTrigger
                    | gx::lint::RuleName::MissingConcurrency
                    | gx::lint::RuleName::PrHeadCheckout
                    | gx::lint::RuleName::UnprotectedSecrets
                    | gx::lint::RuleName::DanglingReference
                    | gx::lint::RuleName::InvalidExpression
                    | gx::lint::RuleName::RunShellcheck
            )
        })
        .collect();

    assert!(
        on_composite.is_empty(),
        "workflow-schema rules must not fire on composite files: {on_composite:?}"
    );
}

#[test]
fn lint_run_shellcheck_skips_composite_run_bodies() {
    let temp_dir = tempfile::tempdir().unwrap();
    let repo_root = temp_dir.path();
    let workflows_dir = repo_root.join(".github").join("workflows");
    fs::create_dir_all(&workflows_dir).unwrap();
    // A composite whose run: body has an unquoted expansion shellcheck flags (SC2086).
    write_composite(
        repo_root,
        "setup",
        "    - shell: bash\n      run: echo $FOO\n",
    );

    let manifest = Manifest::default();
    let lock = Lock::default();
    let scanner = FileWorkflowScanner::new(repo_root);
    let lint_config = Lint::default();

    let diagnostics =
        lint::collect_diagnostics(&manifest, &lock, &scanner, &lint_config, &mut |_| {})
            .expect("Should succeed");

    assert!(
        !diagnostics
            .iter()
            .any(|d| d.rule == gx::lint::RuleName::RunShellcheck),
        "run-shellcheck is deferred for composite files: {diagnostics:?}"
    );
}

#[test]
fn lint_ignore_scoped_to_composite_file() {
    let temp_dir = tempfile::tempdir().unwrap();
    let repo_root = temp_dir.path();
    let workflows_dir = repo_root.join(".github").join("workflows");
    fs::create_dir_all(&workflows_dir).unwrap();
    fs::write(
        workflows_dir.join("ci.yml"),
        "name: CI\non: [push]\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: actions/setup-node@v3\n",
    )
    .unwrap();
    write_composite(repo_root, "setup", "    - uses: actions/checkout@v4\n");

    let mut manifest = Manifest::default();
    manifest.set(ActionId::from("actions/checkout"), Specifier::from_v1("v4"));
    manifest.set(
        ActionId::from("actions/setup-node"),
        Specifier::from_v1("v3"),
    );
    let lock = Lock::default();
    let scanner = FileWorkflowScanner::new(repo_root);

    let mut lint_config = Lint::default();
    lint_config.rules.insert(
        gx::lint::RuleName::Unpinned,
        gx::config::Rule {
            level: Level::Error,
            ignore: vec![gx::config::IgnoreTarget {
                action: None,
                workflow: Some(".github/actions/setup/action.yml".to_owned()),
                job: None,
            }],
        },
    );

    let diagnostics =
        lint::collect_diagnostics(&manifest, &lock, &scanner, &lint_config, &mut |_| {})
            .expect("Should succeed");

    let unpinned: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.rule == gx::lint::RuleName::Unpinned)
        .collect();

    assert_eq!(
        unpinned.len(),
        1,
        "composite ignored, workflow still reported: {unpinned:?}"
    );
    assert!(
        unpinned[0]
            .workflow
            .as_ref()
            .is_some_and(|w| w.as_str().contains("ci.yml"))
    );
}

#[test]
fn lint_ignore_with_job_does_not_match_composite() {
    let temp_dir = tempfile::tempdir().unwrap();
    let repo_root = temp_dir.path();
    composite_repo(repo_root);

    let mut manifest = Manifest::default();
    manifest.set(ActionId::from("actions/checkout"), Specifier::from_v1("v4"));
    let lock = Lock::default();
    let scanner = FileWorkflowScanner::new(repo_root);

    let mut lint_config = Lint::default();
    lint_config.rules.insert(
        gx::lint::RuleName::Unpinned,
        gx::config::Rule {
            level: Level::Error,
            ignore: vec![gx::config::IgnoreTarget {
                action: None,
                workflow: Some(".github/actions/setup/action.yml".to_owned()),
                job: Some("build".to_owned()),
            }],
        },
    );

    let diagnostics =
        lint::collect_diagnostics(&manifest, &lock, &scanner, &lint_config, &mut |_| {})
            .expect("Should succeed");

    assert_eq!(
        diagnostics
            .iter()
            .filter(|d| d.rule == gx::lint::RuleName::Unpinned)
            .count(),
        1,
        "a job-scoped ignore cannot match a composite step (no job)"
    );
}

#[test]
fn lint_diagnostic_order_is_stable_across_runs() {
    let temp_dir = tempfile::tempdir().unwrap();
    let repo_root = temp_dir.path();
    let workflows_dir = repo_root.join(".github").join("workflows");
    fs::create_dir_all(&workflows_dir).unwrap();
    fs::write(
        workflows_dir.join("ci.yml"),
        "name: CI\non: [push]\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: actions/setup-node@v3\n",
    )
    .unwrap();
    write_composite(repo_root, "setup", "    - uses: actions/checkout@v4\n");
    write_composite(repo_root, "build", "    - uses: actions/cache@v3\n");

    let manifest = Manifest::default();
    let lock = Lock::default();
    let scanner = FileWorkflowScanner::new(repo_root);
    let lint_config = Lint::default();

    let key = |ds: &[gx::lint::Diagnostic]| -> Vec<String> {
        ds.iter()
            .map(|d| {
                format!(
                    "{}|{}|{:?}",
                    d.rule,
                    d.workflow.as_ref().map_or("", |w| w.as_str()),
                    d.step
                )
            })
            .collect()
    };

    let first =
        lint::collect_diagnostics(&manifest, &lock, &scanner, &lint_config, &mut |_| {}).unwrap();
    let second =
        lint::collect_diagnostics(&manifest, &lock, &scanner, &lint_config, &mut |_| {}).unwrap();

    assert_eq!(key(&first), key(&second), "diagnostic order must be stable");
}
