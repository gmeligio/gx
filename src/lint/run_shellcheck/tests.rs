use super::*;
use crate::domain::workflow_actions::WorkflowPath;
use crate::infra::shellcheck::FakeChecker;

fn parse(content: &str) -> Parsed {
    Parsed::from_yaml(WorkflowPath::new(".github/workflows/ci.yml"), content).unwrap()
}

fn finding(code: u16, severity: Severity) -> Finding {
    Finding {
        code,
        severity,
        line: 1,
        column: 1,
        message: "test finding".to_owned(),
    }
}

/// A workflow with a single bash `run:` step.
fn one_bash_step(body: &str) -> Parsed {
    parse(&format!(
        "on: push
jobs:
  build:
    steps:
      - run: {body}
        shell: bash
"
    ))
}

#[test]
fn rule_metadata_defaults_to_warn() {
    let rule = RunShellcheckRule::new();
    assert_eq!(rule.name(), RuleName::RunShellcheck);
    assert_eq!(rule.default_level(), Level::Warn);
}

#[test]
fn finding_is_reported_as_step_scoped_diagnostic() {
    // Scenario: shell issue in a run step is reported.
    let checker = FakeChecker::new(vec![finding(2086, Severity::Info)]);
    let wfs = [one_bash_step("rm $RUNNER_TEMP/file")];
    let diags = check_workflows(&checker, &wfs);
    assert_eq!(diags.len(), 1);
    assert_eq!(diags[0].rule, RuleName::RunShellcheck);
    assert!(diags[0].message.contains("SC2086"));
    assert_eq!(diags[0].job.as_ref().unwrap().as_str(), "build");
    assert_eq!(diags[0].step.unwrap().as_u16(), 0);
    assert_eq!(
        diags[0].workflow.as_ref().unwrap().as_str(),
        ".github/workflows/ci.yml"
    );
}

#[test]
fn clean_body_produces_no_diagnostic() {
    // Scenario: clean shell body produces no diagnostic.
    let checker = FakeChecker::clean();
    let wfs = [one_bash_step("echo hello")];
    assert!(check_workflows(&checker, &wfs).is_empty());
}

#[test]
fn expression_is_sanitized_before_checking() {
    // Scenario: GitHub Actions expression does not cause a false positive — the body the
    // checker sees has the ${{ }} blanked to underscores.
    let checker = FakeChecker::clean();
    let wfs = [one_bash_step("echo \"${{ github.sha }}\"")];
    let _diags: Vec<Diagnostic> = check_workflows(&checker, &wfs);
    let seen = checker.seen.borrow();
    assert_eq!(seen.len(), 1);
    assert!(!seen[0].contains("${{"));
    assert!(seen[0].contains("________"));
}

#[test]
fn non_shell_step_is_skipped() {
    // Scenario: non-shell run step is skipped (shell: pwsh).
    let checker = FakeChecker::new(vec![finding(2086, Severity::Info)]);
    let wf = parse(
        "on: push
jobs:
  build:
    steps:
      - run: Write-Host hi
        shell: pwsh
",
    );
    let diags = check_workflows(&checker, &[wf]);
    assert!(diags.is_empty());
    // The checker was never invoked for a skipped step.
    assert!(checker.seen.borrow().is_empty());
}

#[test]
fn defaults_run_shell_pwsh_skips_absent_shell_step() {
    // Scenario: defaults.run.shell selects a non-shell and skips the step.
    let checker = FakeChecker::new(vec![finding(2086, Severity::Info)]);
    let wf = parse(
        "on: push
defaults:
  run:
    shell: pwsh
jobs:
  build:
    steps:
      - run: Write-Host hi
",
    );
    let diags = check_workflows(&checker, &[wf]);
    assert!(diags.is_empty());
    assert!(checker.seen.borrow().is_empty());
}

#[test]
fn absent_shell_defaults_to_bash_and_is_analyzed() {
    // No shell anywhere → effective shell is bash, so the step IS analyzed.
    let checker = FakeChecker::new(vec![finding(2086, Severity::Info)]);
    let wf = parse(
        "on: push
jobs:
  build:
    steps:
      - run: rm $X/y
",
    );
    let diags = check_workflows(&checker, &[wf]);
    assert_eq!(diags.len(), 1);
}

#[test]
fn missing_binary_emits_single_skip_diagnostic() {
    // Scenario: shellcheck binary missing degrades gracefully.
    let rule = RunShellcheckRule {
        availability: Availability::Absent,
    };
    let manifest = crate::domain::manifest::Manifest::default();
    let lock = crate::domain::lock::Lock::default();
    let action_set = crate::domain::workflow_actions::ActionSet::new();
    let wfs = [one_bash_step("rm $X/y")];
    let ctx = Context {
        manifest: &manifest,
        lock: &lock,
        workflows: &[],
        workflows_full: &wfs,
        action_set: &action_set,
    };
    let diags = rule.check(&ctx);
    assert_eq!(diags.len(), 1);
    assert!(diags[0].message.contains("shellcheck"));
    assert!(diags[0].message.contains("skipped"));
    assert!(diags[0].workflow.is_none());
}

#[test]
fn message_includes_severity_and_script_line() {
    let checker = FakeChecker::new(vec![Finding {
        code: 2046,
        severity: Severity::Warning,
        line: 7,
        column: 3,
        message: "Quote this to prevent word splitting.".to_owned(),
    }]);
    let wfs = [one_bash_step("foo")];
    let diags = check_workflows(&checker, &wfs);
    assert!(diags[0].message.contains("SC2046"));
    assert!(diags[0].message.contains("warning"));
    assert!(diags[0].message.contains("line 7"));
}
