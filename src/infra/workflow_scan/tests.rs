use super::FileScanner as FileWorkflowScanner;
use crate::domain::action::identity::ActionId;
use crate::domain::workflow::Scanner as _;
use crate::domain::workflow_actions::StepIndex;
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

fn create_test_workflow(dir: &Path, name: &str, content: &str) -> PathBuf {
    let workflows_dir = dir.join(".github").join("workflows");
    fs::create_dir_all(&workflows_dir).unwrap();
    let file_path = workflows_dir.join(name);
    let mut file = fs::File::create(&file_path).unwrap();
    file.write_all(content.as_bytes()).unwrap();
    file_path
}

#[test]
fn scan_all_located_includes_location() {
    let temp_dir = TempDir::new().unwrap();
    let content = "name: CI
jobs:
  build:
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v3
  test:
    steps:
      - uses: actions/checkout@v3
";
    create_test_workflow(temp_dir.path(), "ci.yml", content);

    let scanner = FileWorkflowScanner::new(temp_dir.path());
    let located = scanner.scan_all_located().unwrap();

    assert_eq!(located.len(), 3);

    // Find the build-job checkout entry
    let build_checkout = located.iter().find(|a| {
        a.action.id == ActionId::from("actions/checkout")
            && a.location
                .job
                .as_ref()
                .map(crate::domain::workflow_actions::JobId::as_str)
                == Some("build")
    });
    assert!(build_checkout.is_some());
    let bc = build_checkout.unwrap();
    assert_eq!(bc.action.version.as_str(), "v4");
    assert_eq!(bc.location.step, Some(StepIndex::from(0_u16)));

    let test_checkout = located.iter().find(|a| {
        a.action.id == ActionId::from("actions/checkout")
            && a.location
                .job
                .as_ref()
                .map(crate::domain::workflow_actions::JobId::as_str)
                == Some("test")
    });
    assert!(test_checkout.is_some());
    assert_eq!(test_checkout.unwrap().action.version.as_str(), "v3");
}

#[test]
fn find_workflows() {
    let temp_dir = TempDir::new().unwrap();
    create_test_workflow(temp_dir.path(), "ci.yml", "name: CI");
    create_test_workflow(temp_dir.path(), "deploy.yaml", "name: Deploy");

    let parser = FileWorkflowScanner::new(temp_dir.path());
    let workflows = parser.find_workflows().unwrap();

    assert_eq!(workflows.len(), 2);
}

#[test]
fn scan_single_workflow() {
    let temp_dir = TempDir::new().unwrap();
    let content = "name: CI
on: push
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v3
      - uses: docker/build-push-action@v5
";
    let workflow_path = create_test_workflow(temp_dir.path(), "ci.yml", content);

    let parser = FileWorkflowScanner::new(temp_dir.path());
    let action_set = parser.scan_file(&workflow_path).unwrap();

    let ids: Vec<_> = action_set.action_ids().collect();
    assert_eq!(ids.len(), 3);
    assert!(ids.contains(&&ActionId::from("actions/checkout")));
    assert!(ids.contains(&&ActionId::from("actions/setup-node")));
    assert!(ids.contains(&&ActionId::from("docker/build-push-action")));
}

#[test]
fn scan_skips_local() {
    let temp_dir = TempDir::new().unwrap();
    let content = "name: CI
jobs:
  build:
    steps:
      - uses: actions/checkout@v4
      - uses: ./local/action
      - uses: ./.github/actions/my-action
";
    let workflow_path = create_test_workflow(temp_dir.path(), "ci.yml", content);

    let parser = FileWorkflowScanner::new(temp_dir.path());
    let action_set = parser.scan_file(&workflow_path).unwrap();

    let ids: Vec<_> = action_set.action_ids().collect();
    assert_eq!(ids.len(), 1);
    assert!(ids.contains(&&ActionId::from("actions/checkout")));
}

#[test]
fn scan_multiple_jobs() {
    let temp_dir = TempDir::new().unwrap();
    let content = "name: CI
jobs:
  build:
    steps:
      - uses: actions/checkout@v4
  test:
    steps:
      - uses: actions/checkout@v3
      - uses: actions/setup-node@v3
";
    let workflow_path = create_test_workflow(temp_dir.path(), "ci.yml", content);

    let parser = FileWorkflowScanner::new(temp_dir.path());
    let action_set = parser.scan_file(&workflow_path).unwrap();

    // Two unique actions (checkout appears in both jobs with different versions)
    assert_eq!(action_set.action_ids().count(), 2);

    assert_eq!(
        action_set
            .versions_for(&ActionId::from("actions/checkout"))
            .count(),
        2
    );
}

#[test]
fn scan_all_located_derives_action_set() {
    let temp_dir = TempDir::new().unwrap();
    create_test_workflow(
        temp_dir.path(),
        "ci.yml",
        "jobs:\n  build:\n    steps:\n      - uses: actions/checkout@v4",
    );
    create_test_workflow(
        temp_dir.path(),
        "deploy.yml",
        "jobs:\n  deploy:\n    steps:\n      - uses: docker/build-push-action@v5",
    );

    let parser = FileWorkflowScanner::new(temp_dir.path());
    let located = parser.scan_all_located().unwrap();
    let action_set = crate::domain::workflow_actions::ActionSet::from_located(&located);

    assert_eq!(action_set.action_ids().count(), 2);
}

#[test]
fn scan_with_sha_and_comment() {
    let temp_dir = TempDir::new().unwrap();
    let content = "name: CI
jobs:
  build:
    steps:
      - uses: actions/checkout@abc123def456 # v4
      - uses: actions/setup-node@xyz789 #v3
";
    let workflow_path = create_test_workflow(temp_dir.path(), "ci.yml", content);

    let parser = FileWorkflowScanner::new(temp_dir.path());
    let action_set = parser.scan_file(&workflow_path).unwrap();

    let checkout_version = action_set
        .versions_for(&ActionId::from("actions/checkout"))
        .next()
        .unwrap();
    assert_eq!(checkout_version.as_str(), "v4");

    let node_version = action_set
        .versions_for(&ActionId::from("actions/setup-node"))
        .next()
        .unwrap();
    assert_eq!(node_version.as_str(), "v3");
}

#[test]
fn scan_comment_without_v_prefix() {
    let temp_dir = TempDir::new().unwrap();
    let content = "name: CI
jobs:
  build:
    steps:
      - uses: actions/checkout@abc123 # 4
";
    let workflow_path = create_test_workflow(temp_dir.path(), "ci.yml", content);

    let parser = FileWorkflowScanner::new(temp_dir.path());
    let action_set = parser.scan_file(&workflow_path).unwrap();

    let version = action_set
        .versions_for(&ActionId::from("actions/checkout"))
        .next()
        .unwrap();
    // Should normalize to v4
    assert_eq!(version.as_str(), "v4");
}

#[test]
fn scan_tag_without_comment() {
    let temp_dir = TempDir::new().unwrap();
    let content = "name: CI
jobs:
  build:
    steps:
      - uses: actions/checkout@v4
";
    let workflow_path = create_test_workflow(temp_dir.path(), "ci.yml", content);

    let parser = FileWorkflowScanner::new(temp_dir.path());
    let action_set = parser.scan_file(&workflow_path).unwrap();

    let version = action_set
        .versions_for(&ActionId::from("actions/checkout"))
        .next()
        .unwrap();
    assert_eq!(version.as_str(), "v4");
}

#[test]
fn scan_sha_without_comment() {
    let temp_dir = TempDir::new().unwrap();
    let content = "name: CI
jobs:
  build:
    steps:
      - uses: actions/checkout@abc123def456
";
    let workflow_path = create_test_workflow(temp_dir.path(), "ci.yml", content);

    let parser = FileWorkflowScanner::new(temp_dir.path());
    let action_set = parser.scan_file(&workflow_path).unwrap();

    let version = action_set
        .versions_for(&ActionId::from("actions/checkout"))
        .next()
        .unwrap();
    // Should use SHA as version when no comment
    assert_eq!(version.as_str(), "abc123def456");
}

#[test]
fn scan_real_world_format() {
    let temp_dir = TempDir::new().unwrap();
    let content = "on:
  pull_request:

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - name: Checkout repository
        uses: actions/checkout@8e8c483db84b4bee98b60c0593521ed34d9990e8 # v6.0.1

      - name: Login
        uses: docker/login-action@5e57cd118135c172c3672efd75eb46360885c0ef # v3.6.0
";
    let workflow_path = create_test_workflow(temp_dir.path(), "test.yml", content);

    let parser = FileWorkflowScanner::new(temp_dir.path());
    let action_set = parser.scan_file(&workflow_path).unwrap();

    let checkout_id = ActionId::from("actions/checkout");
    let checkout_action_version = action_set.versions_for(&checkout_id).next().unwrap();
    assert_eq!(checkout_action_version.as_str(), "v6.0.1");

    let login_id = ActionId::from("docker/login-action");
    let login_action_version = action_set.versions_for(&login_id).next().unwrap();
    assert_eq!(login_action_version.as_str(), "v3.6.0");
}

#[test]
fn scan_iterator_matches_scan_all_located() {
    let temp_dir = TempDir::new().unwrap();
    create_test_workflow(
        temp_dir.path(),
        "ci.yml",
        "jobs:\n  build:\n    steps:\n      - uses: actions/checkout@v4\n      - uses: actions/setup-node@v3",
    );
    create_test_workflow(
        temp_dir.path(),
        "deploy.yml",
        "jobs:\n  deploy:\n    steps:\n      - uses: docker/build-push-action@v5",
    );

    let scanner = FileWorkflowScanner::new(temp_dir.path());

    // Collect via the iterator-based scan()
    let via_iter: Vec<_> = scanner.scan().collect::<Result<Vec<_>, _>>().unwrap();
    // Collect via the default scan_all_located()
    let via_collect = scanner.scan_all_located().unwrap();

    assert_eq!(via_iter.len(), via_collect.len());

    // Same action IDs appear in both
    let mut iter_ids: Vec<String> = via_iter
        .iter()
        .map(|a| a.action.id.as_str().to_owned())
        .collect();
    let mut collect_ids: Vec<String> = via_collect
        .iter()
        .map(|a| a.action.id.as_str().to_owned())
        .collect();
    iter_ids.sort();
    collect_ids.sort();
    assert_eq!(iter_ids, collect_ids);
}

#[test]
fn scan_all_with_parsed_matches_scan_all_located() {
    // Task 1.3 regression test: the combined single-pass parse must produce
    // exactly the same WorkflowAction list as the legacy per-iterator path.
    let temp_dir = TempDir::new().unwrap();
    create_test_workflow(
        temp_dir.path(),
        "ci.yml",
        "on: pull_request
permissions:
  contents: read
jobs:
  build:
    steps:
      - uses: actions/checkout@8e8c483db84b4bee98b60c0593521ed34d9990e8 # v4
      - uses: actions/setup-node@v3
",
    );
    create_test_workflow(
        temp_dir.path(),
        "deploy.yml",
        "on: push
jobs:
  deploy:
    steps:
      - uses: docker/build-push-action@v5",
    );

    let scanner = FileWorkflowScanner::new(temp_dir.path());

    let via_legacy = scanner.scan_all_located().unwrap();
    let (via_combined, parsed) = scanner.scan_all_with_parsed().unwrap();

    assert_eq!(via_legacy.len(), via_combined.len());

    let mut legacy_keys: Vec<String> = via_legacy
        .iter()
        .map(|a| format!("{}@{}", a.action.id.as_str(), a.action.version.as_str()))
        .collect();
    let mut combined_keys: Vec<String> = via_combined
        .iter()
        .map(|a| format!("{}@{}", a.action.id.as_str(), a.action.version.as_str()))
        .collect();
    legacy_keys.sort();
    combined_keys.sort();
    assert_eq!(legacy_keys, combined_keys);

    // The Parsed output must carry the same workflow set and the structural
    // fields rules will consume.
    assert_eq!(parsed.len(), 2);
    let ci = parsed
        .iter()
        .find(|p| p.path.as_str().ends_with("ci.yml"))
        .unwrap();
    assert!(ci.permissions.is_some());
    assert!(
        ci.on
            .iter()
            .any(|t| matches!(t, crate::domain::workflow_parsed::Trigger::PullRequest))
    );
    let deploy = parsed
        .iter()
        .find(|p| p.path.as_str().ends_with("deploy.yml"))
        .unwrap();
    assert!(deploy.permissions.is_none());
    assert!(
        deploy
            .on
            .iter()
            .any(|t| matches!(t, crate::domain::workflow_parsed::Trigger::Push))
    );
}

#[test]
fn scan_same_uses_keeps_per_step_comment() {
    // Regression test for the dup-key bug in the old regex comment-scraper: it keyed a
    // `HashMap<uses-text, comment>` on the `uses:` string, so two steps pinning the *same*
    // `action@sha` with *different* version comments collapsed to one entry (last write
    // wins) and both steps were mislabeled. Reading the comment from each step's own parsed
    // `Commented<String>` value gives each step its own comment.
    let temp_dir = TempDir::new().unwrap();
    let content = "name: CI
jobs:
  build:
    steps:
      - uses: actions/checkout@8e8c483db84b4bee98b60c0593521ed34d9990e8 # v4
  test:
    steps:
      - uses: actions/checkout@8e8c483db84b4bee98b60c0593521ed34d9990e8 # v5
";
    create_test_workflow(temp_dir.path(), "ci.yml", content);

    let scanner = FileWorkflowScanner::new(temp_dir.path());
    let located = scanner.scan_all_located().unwrap();

    let version_for = |job: &str| {
        located
            .iter()
            .find(|a| {
                a.action.id == ActionId::from("actions/checkout")
                    && a.location
                        .job
                        .as_ref()
                        .map(crate::domain::workflow_actions::JobId::as_str)
                        == Some(job)
            })
            .unwrap()
            .action
            .version
            .as_str()
            .to_owned()
    };

    // Each step keeps the comment written on its own line, not a single shared one.
    assert_eq!(version_for("build"), "v4");
    assert_eq!(version_for("test"), "v5");
}

#[test]
fn scan_iterator_yields_error_for_malformed_file_without_aborting() {
    let temp_dir = TempDir::new().unwrap();
    // One valid workflow
    create_test_workflow(
        temp_dir.path(),
        "good.yml",
        "jobs:\n  build:\n    steps:\n      - uses: actions/checkout@v4",
    );
    // One malformed workflow (invalid YAML)
    create_test_workflow(temp_dir.path(), "bad.yml", ":\n  :\n    - [invalid yaml{{{");

    let scanner = FileWorkflowScanner::new(temp_dir.path());

    let results: Vec<_> = scanner.scan().collect();

    // We should get at least one Ok (from good.yml) and at least one Err (from bad.yml)
    let ok_count = results.iter().filter(|r| r.is_ok()).count();
    let err_count = results.iter().filter(|r| r.is_err()).count();

    assert!(
        ok_count >= 1,
        "Expected at least one Ok result from good.yml"
    );
    assert!(
        err_count >= 1,
        "Expected at least one Err result from bad.yml"
    );
}
