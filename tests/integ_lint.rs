#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "tests use unwrap, indexing, and other patterns freely"
)]

use gx::config::{Level, Lint};
use gx::domain::action::identity::{ActionId, CommitDate, CommitSha, Repository, Version};
use gx::domain::action::resolved::Commit;
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
        Version::from("v4"),
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
        Version::from("v3"),
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
        Version::from("v3"),
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
        Version::from("v4"),
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
    let (on, off) = run_off_toggle(workflow, gx::lint::RuleName::MissingPermissions, Lint::default());
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
    let (on, off) =
        run_off_toggle(workflow, gx::lint::RuleName::ExcessivePermissions, Lint::default());
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
    let (on, off) =
        run_off_toggle(workflow, gx::lint::RuleName::DangerousTrigger, Lint::default());
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
    let (on, off) =
        run_off_toggle(workflow, gx::lint::RuleName::PrHeadCheckout, Lint::default());
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
    let (on, off) =
        run_off_toggle(workflow, gx::lint::RuleName::MissingConcurrency, Lint::default());
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
    let (on, off) =
        run_off_toggle(workflow, gx::lint::RuleName::UnprotectedSecrets, Lint::default());
    assert!(on > 0, "unprotected-secrets should fire by default");
    assert_eq!(off, 0, "level = off must suppress unprotected-secrets");
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
