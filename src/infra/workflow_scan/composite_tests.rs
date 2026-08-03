//! Discovery and extraction tests for composite action definitions
//! (`.github/actions/**/action.yml`).

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

/// Write a file under `.github/actions/{name}`, creating parent directories.
/// `name` carries the whole relative path, so callers choose the file name
/// (`setup/action.yml`, `setup/config.yml`, `a/b/action.yaml`).
fn create_test_action_file(dir: &Path, name: &str, content: &str) -> PathBuf {
    let file_path = dir.join(".github").join("actions").join(name);
    fs::create_dir_all(file_path.parent().unwrap()).unwrap();
    let mut file = fs::File::create(&file_path).unwrap();
    file.write_all(content.as_bytes()).unwrap();
    file_path
}

/// A minimal composite action wrapping the given steps block.
fn composite(steps: &str) -> String {
    format!("name: Setup\nruns:\n  using: composite\n  steps:\n{steps}")
}

#[test]
fn scan_finds_composite_action() {
    let temp_dir = TempDir::new().unwrap();
    create_test_action_file(
        temp_dir.path(),
        "setup/action.yml",
        &composite("      - uses: actions/setup-node@v4\n"),
    );

    let scanner = FileWorkflowScanner::new(temp_dir.path());
    let located = scanner.scan_all_located().unwrap();

    assert_eq!(located.len(), 1);
    assert_eq!(located[0].action.id, ActionId::from("actions/setup-node"));
    assert_eq!(
        located[0].location.workflow.as_str(),
        ".github/actions/setup/action.yml"
    );
}

#[test]
fn scan_finds_nested_composite_action() {
    let temp_dir = TempDir::new().unwrap();
    create_test_action_file(
        temp_dir.path(),
        "ci/setup/action.yml",
        &composite("      - uses: actions/checkout@v4\n"),
    );

    let scanner = FileWorkflowScanner::new(temp_dir.path());
    let located = scanner.scan_all_located().unwrap();

    assert_eq!(located.len(), 1);
    assert_eq!(
        located[0].location.workflow.as_str(),
        ".github/actions/ci/setup/action.yml"
    );
}

#[test]
fn scan_finds_composite_action_yaml_extension() {
    let temp_dir = TempDir::new().unwrap();
    create_test_action_file(
        temp_dir.path(),
        "setup/action.yaml",
        &composite("      - uses: actions/checkout@v4\n"),
    );

    let scanner = FileWorkflowScanner::new(temp_dir.path());
    let located = scanner.scan_all_located().unwrap();

    assert_eq!(located.len(), 1);
    assert_eq!(
        located[0].location.workflow.as_str(),
        ".github/actions/setup/action.yaml"
    );
}

#[test]
fn scan_ignores_non_action_yaml_beside_definition() {
    let temp_dir = TempDir::new().unwrap();
    create_test_action_file(
        temp_dir.path(),
        "setup/action.yml",
        &composite("      - uses: actions/checkout@v4\n"),
    );
    // Read as an action definition this would contribute an action or fail to parse.
    create_test_action_file(
        temp_dir.path(),
        "setup/config.yml",
        &composite("      - uses: actions/setup-go@v5\n"),
    );

    let scanner = FileWorkflowScanner::new(temp_dir.path());
    let results: Vec<_> = scanner.scan().collect();

    assert!(results.iter().all(Result::is_ok), "no errors expected");
    let located = scanner.scan_all_located().unwrap();
    assert_eq!(located.len(), 1);
    assert_eq!(located[0].action.id, ActionId::from("actions/checkout"));
}

#[test]
fn scan_skips_non_composite_action_without_error() {
    let temp_dir = TempDir::new().unwrap();
    create_test_action_file(
        temp_dir.path(),
        "tool/action.yml",
        "name: Tool\nruns:\n  using: node20\n  main: index.js\n",
    );

    let scanner = FileWorkflowScanner::new(temp_dir.path());
    let results: Vec<_> = scanner.scan().collect();

    assert!(results.iter().all(Result::is_ok), "no errors expected");
    assert!(scanner.scan_all_located().unwrap().is_empty());
}

#[test]
fn scan_skips_action_without_using_key() {
    let temp_dir = TempDir::new().unwrap();
    create_test_action_file(
        temp_dir.path(),
        "tool/action.yml",
        "name: Tool\nruns:\n  steps:\n      - uses: actions/checkout@v4\n",
    );

    let scanner = FileWorkflowScanner::new(temp_dir.path());
    let results: Vec<_> = scanner.scan().collect();

    assert!(results.iter().all(Result::is_ok), "no errors expected");
    assert!(scanner.scan_all_located().unwrap().is_empty());
}

#[test]
fn scan_yields_error_for_malformed_composite_without_aborting() {
    let temp_dir = TempDir::new().unwrap();
    create_test_action_file(
        temp_dir.path(),
        "good/action.yml",
        &composite("      - uses: actions/checkout@v4\n"),
    );
    create_test_action_file(
        temp_dir.path(),
        "broken/action.yml",
        ":\n  :\n    - [invalid yaml{{{",
    );

    let scanner = FileWorkflowScanner::new(temp_dir.path());
    let results: Vec<_> = scanner.scan().collect();

    assert!(results.iter().filter(|r| r.is_ok()).count() >= 1);
    assert!(results.iter().filter(|r| r.is_err()).count() >= 1);
}

#[test]
fn composite_step_has_no_job_and_carries_step_and_line() {
    let temp_dir = TempDir::new().unwrap();
    create_test_action_file(
        temp_dir.path(),
        "setup/action.yml",
        &composite("      - uses: actions/checkout@v4\n      - uses: actions/setup-node@v3\n"),
    );

    let scanner = FileWorkflowScanner::new(temp_dir.path());
    let located = scanner.scan_all_located().unwrap();

    assert_eq!(located.len(), 2);
    for action in &located {
        assert!(
            action.location.job.is_none(),
            "composite steps belong to no job"
        );
        assert!(action.location.line.is_some(), "line should be recorded");
    }
    let node = located
        .iter()
        .find(|a| a.action.id == ActionId::from("actions/setup-node"))
        .unwrap();
    assert_eq!(node.location.step, Some(StepIndex::from(1_u16)));
}

#[test]
fn scan_skips_local_and_docker_inside_composite() {
    let temp_dir = TempDir::new().unwrap();
    create_test_action_file(
        temp_dir.path(),
        "build/action.yml",
        &composite(
            "      - uses: ./.github/actions/setup\n      - uses: docker://alpine:3\n      - uses: actions/checkout@v4\n",
        ),
    );

    let scanner = FileWorkflowScanner::new(temp_dir.path());
    let located = scanner.scan_all_located().unwrap();

    assert_eq!(located.len(), 1);
    assert_eq!(located[0].action.id, ActionId::from("actions/checkout"));
}

#[test]
fn composite_keeps_per_step_comment() {
    let temp_dir = TempDir::new().unwrap();
    let sha = "a".repeat(40);
    create_test_action_file(
        temp_dir.path(),
        "setup/action.yml",
        &composite(&format!(
            "      - uses: actions/checkout@{sha} # v4\n      - uses: actions/checkout@{sha} # v5\n"
        )),
    );

    let scanner = FileWorkflowScanner::new(temp_dir.path());
    let located = scanner.scan_all_located().unwrap();

    assert_eq!(located.len(), 2);
    let label_at = |step: u16| {
        located
            .iter()
            .find(|a| a.location.step == Some(StepIndex::from(step)))
            .unwrap()
            .action
            .reference
            .label()
            .to_owned()
    };
    assert_eq!(label_at(0), "v4");
    assert_eq!(label_at(1), "v5");
}

#[test]
fn discovery_order_is_deterministic_workflows_before_composites() {
    let temp_dir = TempDir::new().unwrap();
    create_test_workflow(temp_dir.path(), "zzz.yml", "name: Z");
    create_test_workflow(temp_dir.path(), "aaa.yaml", "name: A");
    create_test_action_file(temp_dir.path(), "zebra/action.yml", "name: Zebra");
    create_test_action_file(temp_dir.path(), "alpha/action.yml", "name: Alpha");

    let scanner = FileWorkflowScanner::new(temp_dir.path());
    let first = scanner.find_workflow_paths().unwrap();
    let second = scanner.find_workflow_paths().unwrap();

    assert_eq!(first, second, "order must be stable across runs");

    let rel: Vec<String> = first
        .iter()
        .map(|p| {
            p.strip_prefix(temp_dir.path())
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/")
        })
        .collect();
    assert_eq!(
        rel,
        vec![
            ".github/workflows/aaa.yaml",
            ".github/workflows/zzz.yml",
            ".github/actions/alpha/action.yml",
            ".github/actions/zebra/action.yml",
        ]
    );
}

#[test]
fn scan_file_reads_a_composite_under_the_right_schema() {
    let temp_dir = TempDir::new().unwrap();
    let path = create_test_action_file(
        temp_dir.path(),
        "setup/action.yml",
        &composite("      - uses: actions/checkout@v4\n"),
    );

    let scanner = FileWorkflowScanner::new(temp_dir.path());
    let action_set = scanner.scan_file(&path).unwrap();

    let ids: Vec<_> = action_set.action_ids().collect();
    assert_eq!(ids.len(), 1);
    assert!(ids.contains(&&ActionId::from("actions/checkout")));
}

#[test]
fn discovery_kind_agrees_with_of_path() {
    use crate::domain::workflow_parsed::FileKind;

    let temp_dir = TempDir::new().unwrap();
    create_test_workflow(temp_dir.path(), "ci.yml", "name: CI");
    // A workflow named action.yml — the case where a file-name rule would disagree.
    create_test_workflow(temp_dir.path(), "action.yml", "name: Odd");
    create_test_action_file(temp_dir.path(), "setup/action.yml", "name: Setup");
    create_test_action_file(temp_dir.path(), "a/b/action.yaml", "name: Nested");

    let files = super::discovery::managed_files(temp_dir.path()).unwrap();
    assert_eq!(files.len(), 4);

    // Discovery tags kind by root; `of_path` derives it from the path. Disagreement
    // means some layer re-derives the kind with a different rule.
    for file in &files {
        assert_eq!(
            file.kind,
            FileKind::of_path(&file.path),
            "discovery and FileKind::of_path disagree for {}",
            file.path.display()
        );
    }
}
