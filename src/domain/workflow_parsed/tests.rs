#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "tests use unwrap and indexing freely"
)]

use super::*;

fn parse(content: &str) -> Parsed {
    Parsed::from_yaml(WorkflowPath::new(".github/workflows/x.yml"), content).unwrap()
}

#[test]
fn parses_bare_string_trigger() {
    let p = parse("on: push\njobs: {}\n");
    assert_eq!(p.on, vec![Trigger::Push]);
}

#[test]
fn parses_list_triggers() {
    let p = parse("on: [push, pull_request]\njobs: {}\n");
    assert_eq!(p.on, vec![Trigger::Push, Trigger::PullRequest]);
}

#[test]
fn parses_map_triggers_with_filters() {
    let p = parse(
        "on:
  push:
    branches: [main]
  pull_request_target:
    types: [labeled]
jobs: {}
",
    );
    assert!(p.has_trigger(&Trigger::Push));
    assert!(p.has_trigger(&Trigger::PullRequestTarget));
}

#[test]
fn unknown_trigger_round_trips_as_other() {
    let p = parse("on: deployment_status\njobs: {}\n");
    assert_eq!(p.on, vec![Trigger::Other("deployment_status".to_owned())]);
}

#[test]
fn permissions_read_all_shorthand() {
    let p = parse("on: push\npermissions: read-all\njobs: {}\n");
    assert_eq!(p.permissions, Some(Permissions::ReadAll));
    assert!(p.permissions.as_ref().unwrap().is_excessive());
}

#[test]
fn permissions_write_all_is_excessive_and_writable() {
    let p = parse("on: push\npermissions: write-all\njobs: {}\n");
    assert_eq!(p.permissions, Some(Permissions::WriteAll));
    assert!(p.permissions.as_ref().unwrap().is_excessive());
    assert!(p.permissions.as_ref().unwrap().has_write());
}

#[test]
fn permissions_contents_read_only_is_not_excessive() {
    let p = parse("on: push\npermissions:\n  contents: read\njobs: {}\n");
    let perms = p.permissions.unwrap();
    assert!(!perms.is_excessive());
    assert!(!perms.has_write());
}

#[test]
fn permissions_with_packages_write_is_excessive() {
    let p = parse(
        "on: push
permissions:
  contents: read
  packages: write
jobs: {}
",
    );
    let perms = p.permissions.unwrap();
    assert!(perms.is_excessive());
    assert!(perms.has_write());
}

#[test]
fn empty_permissions_map_drops_defaults() {
    let p = parse("on: push\npermissions: {}\njobs: {}\n");
    assert_eq!(p.permissions, Some(Permissions::Empty));
    assert!(!p.permissions.as_ref().unwrap().is_excessive());
}

#[test]
fn concurrency_captures_group_and_cancel() {
    let p = parse(
        "on: push
concurrency:
  group: ci-${{ github.ref }}
  cancel-in-progress: true
jobs: {}
",
    );
    let c = p.concurrency.unwrap();
    assert_eq!(c.group.as_deref(), Some("ci-${{ github.ref }}"));
    assert_eq!(c.cancel_in_progress, Some(true));
}

#[test]
fn jobs_populate_id_from_map_key() {
    let p = parse(
        "on: push
jobs:
  build:
    steps:
      - uses: actions/checkout@v4
  test:
    steps: []
",
    );
    let mut ids: Vec<&str> = p.jobs.iter().map(|j| j.id.as_str()).collect();
    ids.sort_unstable();
    assert_eq!(ids, vec!["build", "test"]);
}

#[test]
fn step_captures_with_env_run_and_if() {
    let p = parse(
        "on: pull_request
jobs:
  build:
    steps:
      - uses: docker/login-action@v3
        if: github.event.pull_request.head.repo.full_name == github.repository
        with:
          username: foo
          password: ${{ secrets.DOCKER_HUB_TOKEN }}
        env:
          NODE_ENV: production
      - run: echo hello
",
    );
    let job = &p.jobs[0];
    let s0 = &job.steps[0];
    assert_eq!(s0.uses_ref(), Some("docker/login-action@v3"));
    assert!(
        s0.if_cond
            .as_deref()
            .unwrap()
            .contains("head.repo.full_name")
    );
    assert_eq!(s0.with.get("ref"), None);
    assert!(
        s0.with["password"]
            .as_str()
            .contains("secrets.DOCKER_HUB_TOKEN")
    );
    assert_eq!(s0.env["NODE_ENV"].as_str(), "production");
    let s1 = &job.steps[1];
    assert_eq!(s1.run.as_deref(), Some("echo hello"));
}

#[test]
fn scalar_text_concatenates_with_env_run() {
    let p = parse(
        "on: pull_request
jobs:
  build:
    steps:
      - with:
          password: ${{ secrets.MY_TOKEN }}
        env:
          OTHER: ${{ secrets.OTHER_TOKEN }}
        run: echo done
",
    );
    let text = p.jobs[0].steps[0].scalar_text();
    assert!(text.contains("secrets.MY_TOKEN"));
    assert!(text.contains("secrets.OTHER_TOKEN"));
    assert!(text.contains("echo done"));
}

#[test]
fn job_secrets_inherit_is_distinguished_from_explicit() {
    let p = parse(
        "on: workflow_call
jobs:
  call:
    uses: ./.github/workflows/x.yml
    secrets: inherit
",
    );
    assert_eq!(p.jobs[0].secrets, Some(JobSecrets::Inherit));
}

#[test]
fn needs_scalar_form_parses_to_single_element_vec() {
    let p = parse(
        "on: push
jobs:
  build:
    steps: []
  test:
    needs: build
    steps: []
",
    );
    let test = p.jobs.iter().find(|j| j.id == "test").unwrap();
    assert_eq!(test.needs, vec!["build".to_owned()]);
}

#[test]
fn needs_sequence_form_parses_to_vec() {
    let p = parse(
        "on: push
jobs:
  build:
    steps: []
  lint:
    steps: []
  deploy:
    needs: [build, lint]
    steps: []
",
    );
    let deploy = p.jobs.iter().find(|j| j.id == "deploy").unwrap();
    assert_eq!(deploy.needs, vec!["build".to_owned(), "lint".to_owned()]);
}

#[test]
fn absent_needs_is_empty_vec() {
    let p = parse("on: push\njobs:\n  build:\n    steps: []\n");
    assert!(p.jobs[0].needs.is_empty());
}

#[test]
fn job_outputs_map_is_captured() {
    let p = parse(
        "on: push
jobs:
  build:
    outputs:
      sha: ${{ steps.x.outputs.sha }}
      tag: ${{ steps.x.outputs.tag }}
    steps: []
",
    );
    let outs = &p.jobs[0].outputs;
    assert_eq!(outs.len(), 2);
    assert!(outs.contains_key("sha"));
    assert!(outs.contains_key("tag"));
}

#[test]
fn absent_outputs_is_empty_map() {
    let p = parse("on: push\njobs:\n  build:\n    steps: []\n");
    assert!(p.jobs[0].outputs.is_empty());
}

#[test]
fn step_id_is_captured_when_present_and_none_when_absent() {
    let p = parse(
        "on: push
jobs:
  build:
    steps:
      - id: checkout
        uses: actions/checkout@v4
      - run: echo hi
",
    );
    let steps = &p.jobs[0].steps;
    assert_eq!(steps[0].id.as_deref(), Some("checkout"));
    assert_eq!(steps[1].id, None);
}

#[test]
fn any_scalar_accepts_numbers_and_bools() {
    let p = parse(
        "on: push
jobs:
  build:
    steps:
      - with:
          retries: 3
          verbose: true
",
    );
    let s = &p.jobs[0].steps[0];
    assert_eq!(s.with["retries"].as_str(), "3");
    assert_eq!(s.with["verbose"].as_str(), "true");
}

#[test]
fn parses_step_shell() {
    let p = parse(
        "on: push
jobs:
  build:
    steps:
      - run: echo hi
        shell: bash
      - run: Write-Host hi
        shell: pwsh
      - run: echo hi
",
    );
    let steps = &p.jobs[0].steps;
    assert_eq!(steps[0].shell.as_deref(), Some("bash"));
    assert_eq!(steps[1].shell.as_deref(), Some("pwsh"));
    assert_eq!(steps[2].shell, None);
}

#[test]
fn parses_defaults_run_shell_at_job_and_workflow_level() {
    let p = parse(
        "on: push
defaults:
  run:
    shell: bash
jobs:
  build:
    defaults:
      run:
        shell: sh
    steps:
      - run: echo hi
",
    );
    assert_eq!(p.defaults.as_ref().unwrap().run_shell(), Some("bash"));
    assert_eq!(p.jobs[0].defaults.as_ref().unwrap().run_shell(), Some("sh"));
}

#[test]
fn defaults_absent_deserializes_to_none() {
    let p = parse("on: push\njobs:\n  build:\n    steps:\n      - run: echo hi\n");
    assert!(p.defaults.is_none());
    assert!(p.jobs[0].defaults.is_none());
}

#[test]
fn effective_shell_prefers_step_over_job_over_workflow() {
    let wf = Defaults {
        run: Some(RunDefaults {
            shell: Some("sh".to_owned()),
        }),
    };
    let job = Defaults {
        run: Some(RunDefaults {
            shell: Some("pwsh".to_owned()),
        }),
    };
    // step wins
    assert_eq!(effective_shell(Some("bash"), Some(&job), Some(&wf)), "bash");
    // job wins over workflow when step absent
    assert_eq!(effective_shell(None, Some(&job), Some(&wf)), "pwsh");
    // workflow used when step + job absent
    assert_eq!(effective_shell(None, None, Some(&wf)), "sh");
}

#[test]
fn effective_shell_defaults_to_bash_when_absent_everywhere() {
    assert_eq!(effective_shell(None, None, None), "bash");
}

#[test]
fn effective_shell_normalizes_template_forms() {
    assert_eq!(effective_shell(Some("bash -e {0}"), None, None), "bash");
    assert_eq!(effective_shell(Some("sh -e {0}"), None, None), "sh");
    // empty value falls back to bash
    assert_eq!(effective_shell(Some("   "), None, None), "bash");
}
