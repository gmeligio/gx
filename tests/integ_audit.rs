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
    // "Could not check" and "checked and clean" are different types. Otherwise a
    // `--json` consumer reading `{"findings": []}` concludes the audit passed when
    // it never ran.
    let temp = TempDir::new().unwrap();
    let root = repo_with_lock(&temp, Some(TAG_LOCK));

    let result = audit(&root, None);

    assert!(result.is_err());
    assert!(
        matches!(result, Err(AuditError::MissingToken { .. })),
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

/// Run the real `gx` binary in `root` with the given `GITHUB_TOKEN`, returning
/// `(stdout, stderr, exit code)`.
///
/// The stream-level `--json` guarantees cannot be observed in-process: they are about what
/// reaches stdout, which only a spawned process shows. `CARGO_BIN_EXE_gx` is set by cargo
/// for integration tests, so this needs no new dependency.
fn run_gx(root: &Path, token: Option<&str>, args: &[&str]) -> (String, String, Option<i32>) {
    let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_gx"));
    command.current_dir(root).args(args);
    match token {
        Some(value) => command.env("GITHUB_TOKEN", value),
        None => command.env_remove("GITHUB_TOKEN"),
    };
    // Keep the run deterministic: CI mode changes which lines are printed.
    command.env_remove("CI");
    let output = command.output().expect("gx binary should run");
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
        output.status.code(),
    )
}

/// Initialize a git repo so `repo::find_root` locates the fixture.
fn git_init(root: &Path) {
    let status = std::process::Command::new("git")
        .args(["init", "-q"])
        .current_dir(root)
        .status()
        .expect("git should be available");
    assert!(status.success(), "git init should succeed");
}

#[test]
fn json_mode_without_a_token_writes_nothing_to_stdout() {
    // The strongest claim the spec makes: a run that could not audit must not emit a
    // document a consumer would read as "clean". Asserting on real stdout, because that
    // is the surface the claim is about — an in-process check cannot see it.
    let temp = TempDir::new().unwrap();
    let root = repo_with_lock(&temp, Some(BRANCH_LOCK));
    git_init(&root);

    let (stdout, stderr, code) = run_gx(&root, None, &["audit", "--json"]);

    assert_eq!(code, Some(1), "a token failure must exit non-zero");
    assert!(
        stdout.trim().is_empty(),
        "stdout must carry no JSON document at all, got: {stdout}"
    );
    assert!(
        stderr.contains("GITHUB_TOKEN"),
        "the error must reach the user and name the variable, got: {stderr}"
    );
}

#[test]
fn a_repo_without_github_still_requires_a_token() {
    // The token guard lives on the command, but a missing `.github` short-circuits before
    // the command runs. Without an explicit guard on that path, `gx audit --json` outside a
    // gx repo emitted `{"findings": []}` and exited 0 — a CI consumer reads that as
    // "audited, clean" for a run that never audited anything.
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    git_init(root);

    let (stdout, stderr, code) = run_gx(root, None, &["audit", "--json"]);

    assert_eq!(code, Some(1), "a token failure must exit non-zero");
    assert!(
        stdout.trim().is_empty(),
        "no document may be emitted for a run that never audited, got: {stdout}"
    );
    assert!(
        stderr.contains("GITHUB_TOKEN"),
        "the error must name the variable, got: {stderr}"
    );
}

#[test]
fn a_repo_without_github_is_clean_once_a_token_is_present() {
    // The counterpart: the guard must gate on the token, not turn "nothing to audit" into
    // a failure. With a token, an empty document and exit 0 remain correct.
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    git_init(root);

    let (stdout, _stderr, code) = run_gx(root, Some("token"), &["audit", "--json"]);

    assert_eq!(code, Some(0));
    let value: serde_json::Value =
        serde_json::from_str(&stdout).expect("stdout must be exactly one JSON document");
    assert_eq!(value["findings"], serde_json::json!([]));
}

#[test]
fn json_mode_writes_one_document_and_no_progress_output() {
    let temp = TempDir::new().unwrap();
    let root = repo_with_lock(&temp, Some(BRANCH_LOCK));
    git_init(&root);

    let (stdout, _stderr, code) = run_gx(&root, Some("token"), &["audit", "--json"]);

    assert_eq!(code, Some(0), "a warning-only run exits zero");
    // Parses whole, so nothing was interleaved with it.
    let value: serde_json::Value =
        serde_json::from_str(&stdout).expect("stdout must be exactly one JSON document");
    assert_eq!(value["findings"][0]["check"], "mutable-ref");
    // The human-facing lines that would corrupt the document.
    assert!(!stdout.contains("📋"), "log path must be suppressed");
    assert!(!stdout.contains('✓'), "summary must be suppressed");
}

#[test]
fn json_mode_writes_no_local_log_file() {
    let temp = TempDir::new().unwrap();
    let root = repo_with_lock(&temp, Some(TAG_LOCK));
    git_init(&root);

    let (stdout, _stderr, _code) = run_gx(&root, Some("token"), &["audit", "--json"]);

    // The log path is printed as the last output line when a log file is written, so its
    // absence from an otherwise-complete document is the observable signal.
    assert!(
        !stdout.contains(".log"),
        "no log file path may appear in JSON mode, got: {stdout}"
    );
    serde_json::from_str::<serde_json::Value>(&stdout).expect("stdout must remain valid JSON");
}

#[test]
fn human_mode_still_prints_a_summary() {
    // The counterpart to the suppression tests: without --json the human lines DO appear,
    // so the assertions above are detecting suppression rather than absence.
    let temp = TempDir::new().unwrap();
    let root = repo_with_lock(&temp, Some(BRANCH_LOCK));
    git_init(&root);

    let (stdout, _stderr, code) = run_gx(&root, Some("token"), &["audit"]);

    assert_eq!(code, Some(0));
    assert!(
        stdout.contains("mutable-ref"),
        "human output must name the check, got: {stdout}"
    );
    assert!(
        serde_json::from_str::<serde_json::Value>(&stdout).is_err(),
        "human output is not JSON"
    );
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
