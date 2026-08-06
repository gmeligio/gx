#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "tests use unwrap, indexing, and other patterns freely"
)]

//! `gx audit` end to end: a real `gx.lock` on disk through the lock parser, the command,
//! and out to findings, exit code, and `--json`.
//!
//! The token is injected via a constructed `Config` rather than by setting `GITHUB_TOKEN`,
//! because process environment is global and would make these flaky under the parallel
//! test runner.

use gx::audit::{Audit, Error as AuditError, Report};
use gx::command::{Command as _, CommandReport};
use gx::config::{Config, GitHubToken, Level, Lint};
use gx::domain::lock::Lock;
use gx::domain::manifest::Manifest;
use gx::infra::lock::Store as LockStore;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

/// A lock recording one action pinned to a moving branch.
const BRANCH_LOCK: &str = r#"
[resolutions."actions/checkout"."main"]
version = "main"

[actions."actions/checkout"."main"]
sha = "abc123def456789012345678901234567890abcd"
repository = "actions/checkout"
ref_type = "branch"
date = "2026-01-01T00:00:00Z"
"#;

/// A lock whose only entry is a proper tag pin.
const TAG_LOCK: &str = r#"
[resolutions."actions/checkout"."^4"]
version = "v4.2.1"

[actions."actions/checkout"."v4.2.1"]
sha = "abc123def456789012345678901234567890abcd"
repository = "actions/checkout"
ref_type = "tag"
date = "2026-01-01T00:00:00Z"
"#;

/// Create `.github/` under a temp dir, optionally writing `gx.lock`.
fn repo_with_lock(temp: &TempDir, lock_toml: Option<&str>) -> std::path::PathBuf {
    let root = temp.path().to_path_buf();
    let github = root.join(".github");
    fs::create_dir_all(&github).unwrap();
    if let Some(contents) = lock_toml {
        fs::write(github.join("gx.lock"), contents).unwrap();
    }
    root
}

/// Load the lock from disk — exercising the real parser — and build a `Config` around it.
fn config_at(root: &Path, token: Option<&str>) -> Config {
    let lock_path = root.join(".github").join("gx.lock");
    let lock: Lock = LockStore::new(&lock_path)
        .load()
        .expect("lock should parse");
    Config {
        settings: gx::config::Settings {
            github_token: token.map(|t| GitHubToken::from(t.to_owned())),
        },
        manifest: Manifest::default(),
        lock,
        lint_config: Lint::default(),
        manifest_path: root.join(".github").join("gx.toml"),
        lock_path,
        manifest_migrated: false,
    }
}

fn audit(root: &Path, token: Option<&str>) -> Result<Report, AuditError> {
    Audit.run(root, config_at(root, token), &mut |_| {})
}

#[test]
fn branch_entry_is_reported_as_a_warning_and_exits_zero() {
    let temp = TempDir::new().unwrap();
    let root = repo_with_lock(&temp, Some(BRANCH_LOCK));

    let report = audit(&root, Some("token")).expect("audit should run with a token");

    assert_eq!(report.diagnostics.len(), 1, "branch pin must be reported");
    assert_eq!(report.diagnostics[0].rule.as_str(), "mutable-ref");
    assert_eq!(report.diagnostics[0].level, Level::Warn);
    assert!(
        report.diagnostics[0].message.contains("actions/checkout"),
        "finding must name the action: {}",
        report.diagnostics[0].message
    );
    // A warning alone does not fail the build.
    assert_eq!(CommandReport::exit_code(&report), 0);
}

#[test]
fn tag_only_lock_is_clean() {
    let temp = TempDir::new().unwrap();
    let root = repo_with_lock(&temp, Some(TAG_LOCK));

    let report = audit(&root, Some("token")).expect("audit should run with a token");

    assert!(report.diagnostics.is_empty());
    assert_eq!(CommandReport::exit_code(&report), 0);
    assert!(
        report.render().iter().any(|line| matches!(
            line,
            gx::output::lines::Line::Summary { text } if text == "No audit findings"
        )),
        "a clean run must say so explicitly"
    );
}

#[test]
fn absent_lock_file_has_nothing_to_audit() {
    let temp = TempDir::new().unwrap();
    let root = repo_with_lock(&temp, None);

    let report = audit(&root, Some("token")).expect("a missing lock is not an error");

    assert!(report.diagnostics.is_empty());
    assert_eq!(CommandReport::exit_code(&report), 0);
}

#[test]
fn empty_lock_file_has_nothing_to_audit() {
    let temp = TempDir::new().unwrap();
    let root = repo_with_lock(&temp, Some(""));

    let report = audit(&root, Some("token")).expect("an empty lock is not an error");

    assert!(report.diagnostics.is_empty());
    assert_eq!(CommandReport::exit_code(&report), 0);
}

#[test]
fn missing_token_fails_loudly_and_names_the_variable() {
    let temp = TempDir::new().unwrap();
    let root = repo_with_lock(&temp, Some(BRANCH_LOCK));

    let error = audit(&root, None).expect_err("audit must refuse to run without a token");

    let message = error.to_string();
    assert!(
        message.contains("GITHUB_TOKEN"),
        "error must name the variable to set: {message}"
    );
    assert!(
        message.contains("gh auth token"),
        "error must tell the user how to fix it: {message}"
    );
}

#[test]
fn missing_token_never_yields_a_report() {
    // The load-bearing property: "could not check" and "checked and clean" are different
    // types, so a token failure cannot be rendered or serialized as a clean result. A
    // consumer of `--json` reading `{"findings": []}` would otherwise conclude the audit
    // passed when it never ran.
    let temp = TempDir::new().unwrap();
    let root = repo_with_lock(&temp, Some(TAG_LOCK));

    let result = audit(&root, None);

    assert!(result.is_err());
    assert!(
        matches!(result, Err(AuditError::MissingToken)),
        "a token failure must not degrade into an empty report"
    );
}

#[test]
fn json_output_carries_findings_and_counts() {
    let temp = TempDir::new().unwrap();
    let root = repo_with_lock(&temp, Some(BRANCH_LOCK));

    let report = audit(&root, Some("token")).unwrap();
    let value: serde_json::Value =
        serde_json::from_str(&report.to_json()).expect("--json must emit one valid JSON document");

    assert_eq!(value["findings"][0]["check"], "mutable-ref");
    assert_eq!(value["findings"][0]["level"], "warn");
    assert_eq!(value["warning_count"], 1);
    assert_eq!(value["error_count"], 0);
}

#[test]
fn json_output_on_a_clean_repo_is_still_one_document() {
    let temp = TempDir::new().unwrap();
    let root = repo_with_lock(&temp, Some(TAG_LOCK));

    let report = audit(&root, Some("token")).unwrap();
    let value: serde_json::Value = serde_json::from_str(&report.to_json()).unwrap();

    assert_eq!(value["findings"], serde_json::json!([]));
}

#[test]
fn only_the_lock_decides_what_is_audited() {
    // The load-bearing design decision: audit reads gx.lock and never walks workflows.
    // The workflow below references a BRANCH pin that is absent from the lock — exactly
    // the kind of entry `mutable-ref` reports — so if a later change reintroduced
    // workflow traversal, this test fails rather than passing silently.
    let temp = TempDir::new().unwrap();
    let root = repo_with_lock(&temp, Some(TAG_LOCK));
    let workflows = root.join(".github").join("workflows");
    fs::create_dir_all(&workflows).unwrap();
    fs::write(
        workflows.join("ci.yml"),
        "name: CI\n\
         on: [push]\n\
         jobs:\n  \
           build:\n    \
             runs-on: ubuntu-latest\n    \
             steps:\n      \
               - uses: actions/setup-node@main\n",
    )
    .unwrap();

    let report = audit(&root, Some("token")).unwrap();

    assert!(
        report.diagnostics.is_empty(),
        "audit must ignore actions that appear only in workflows, got: {:?}",
        report.diagnostics
    );
}
