use crate::config::Level;
use crate::domain::workflow_actions::{JobId, StepIndex};
use crate::domain::workflow_parsed::{Job, Parsed, Step, Trigger};
use crate::lint::{Context, Diagnostic, Rule, RuleName};
use crate::regex::static_regex;

/// `unprotected-secrets` rule: errors when a `pull_request`-triggered workflow references
/// a user-managed secret from a step that isn't guarded by a fork-PR `if:` gate.
///
/// `secrets.GITHUB_TOKEN` is excluded because GitHub auto-scopes it to read-only on fork
/// PRs regardless of declared permissions. Workflows that have widened it via top-level
/// `permissions:` are caught by `excessive-permissions`.
pub struct UnprotectedSecretsRule;

/// Substrings the rule treats as a valid fork-PR gate. Matched against the step's
/// effective `if:` (job-level + step-level concatenated). A textual contains-check is
/// intentional — implementing a full expression parser is out of scope and would not
/// reduce false-positives meaningfully for the canonical pattern.
const FORK_GATE_FRAGMENTS: &[&str] = &[
    "github.event.pull_request.head.repo.full_name == github.repository",
    "github.repository_owner ==",
];

// Regex matching `secrets.NAME` references and capturing the secret name.
static_regex!(SECRET_RE, r"secrets\.([A-Za-z_][A-Za-z0-9_]*)");

impl UnprotectedSecretsRule {
    /// Diagnoses each step in a PR-triggered workflow that references an ungated user secret.
    pub fn check_workflow(workflow: &Parsed) -> Vec<Diagnostic> {
        // Only PR-triggered workflows are in scope. pull_request_target and workflow_run
        // get a single dangerous-trigger diagnostic instead.
        if !workflow.has_trigger(&Trigger::PullRequest) {
            return Vec::new();
        }
        if workflow.has_trigger(&Trigger::PullRequestTarget)
            || workflow.has_trigger(&Trigger::WorkflowRun)
        {
            return Vec::new();
        }

        let mut out = Vec::new();
        for job in &workflow.jobs {
            for (idx, step) in job.steps.iter().enumerate() {
                let secrets = unguarded_secrets(job, step);
                if secrets.is_empty() {
                    continue;
                }
                let msg = format!(
                    "{}: job `{}` step {} references {} without a fork-PR `if:` gate — guard the step with `if: github.event.pull_request.head.repo.full_name == github.repository` so the secret never reaches fork PR code",
                    workflow.path,
                    job.id,
                    idx,
                    format_secret_list(&secrets),
                );
                let mut diag = Diagnostic::new(RuleName::UnprotectedSecrets, Level::Error, msg)
                    .with_workflow(workflow.path.clone())
                    .with_job(JobId::from(job.id.clone()));
                if let Ok(si) = StepIndex::try_from(idx) {
                    diag = diag.with_step(si);
                }
                out.push(diag);
            }
        }
        out
    }
}

/// User-managed secret names referenced by this step that are NOT covered by a fork gate.
/// Returns an empty vec when the step is safe (gated, or only references `GITHUB_TOKEN`).
fn unguarded_secrets(job: &Job, step: &Step) -> Vec<String> {
    let names = step_secret_names(step);
    if names.is_empty() {
        return names;
    }
    if has_fork_gate(job, step) {
        return Vec::new();
    }
    names
}

/// Collects distinct user-managed secret names referenced in a step's `with`, `env`, and `run`.
fn step_secret_names(step: &Step) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    let mut visit = |text: &str| {
        for cap in SECRET_RE.captures_iter(text) {
            let name = cap[1].to_string();
            if name == "GITHUB_TOKEN" {
                continue;
            }
            if !found.contains(&name) {
                found.push(name);
            }
        }
    };
    for v in step.with.values() {
        visit(v.as_str());
    }
    for v in step.env.values() {
        visit(v.as_str());
    }
    if let Some(run) = &step.run {
        visit(run);
    }
    found
}

/// Returns whether the step's effective `if:` (job + step) contains a fork-PR gate fragment.
fn has_fork_gate(job: &Job, step: &Step) -> bool {
    let mut combined = String::new();
    if let Some(job_if) = &job.if_cond {
        combined.push_str(job_if);
        combined.push('\n');
    }
    if let Some(step_if) = &step.if_cond {
        combined.push_str(step_if);
    }
    FORK_GATE_FRAGMENTS
        .iter()
        .any(|frag| combined.contains(frag))
}

/// Formats secret names as a comma-separated list of backtick-quoted `secrets.NAME` entries.
fn format_secret_list(secrets: &[String]) -> String {
    match secrets {
        [single] => format!("`secrets.{single}`"),
        many => many
            .iter()
            .map(|s| format!("`secrets.{s}`"))
            .collect::<Vec<_>>()
            .join(", "),
    }
}

impl Rule for UnprotectedSecretsRule {
    fn name(&self) -> RuleName {
        RuleName::UnprotectedSecrets
    }

    fn default_level(&self) -> Level {
        Level::Error
    }

    fn check(&self, ctx: &Context) -> Vec<Diagnostic> {
        ctx.workflows_full
            .iter()
            .flat_map(Self::check_workflow)
            .collect()
    }
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "tests use unwrap and indexing freely"
)]
mod tests {
    use super::*;
    use crate::domain::workflow_actions::WorkflowPath;

    fn parse(content: &str) -> Parsed {
        Parsed::from_yaml(WorkflowPath::new(".github/workflows/x.yml"), content).unwrap()
    }

    #[test]
    fn rule_metadata() {
        let r = UnprotectedSecretsRule;
        assert_eq!(r.name(), RuleName::UnprotectedSecrets);
        assert_eq!(r.default_level(), Level::Error);
    }

    #[test]
    fn pr_workflow_user_secret_no_gate_errors() {
        let p = parse(
            "on: pull_request
jobs:
  build:
    steps:
      - uses: docker/login-action@v3
        with:
          username: foo
          password: ${{ secrets.DOCKER_HUB_TOKEN }}
",
        );
        let diags = UnprotectedSecretsRule::check_workflow(&p);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].level, Level::Error);
        assert!(diags[0].message.contains("DOCKER_HUB_TOKEN"));
        assert_eq!(diags[0].job.as_ref().unwrap().as_str(), "build");
        assert_eq!(diags[0].step.unwrap().as_u16(), 0);
    }

    #[test]
    fn pr_workflow_user_secret_with_canonical_gate_is_clean() {
        let p = parse(
            "on: pull_request
jobs:
  build:
    steps:
      - if: github.event.pull_request.head.repo.full_name == github.repository
        uses: docker/login-action@v3
        with:
          password: ${{ secrets.DOCKER_HUB_TOKEN }}
",
        );
        assert!(UnprotectedSecretsRule::check_workflow(&p).is_empty());
    }

    #[test]
    fn pr_workflow_only_github_token_is_clean() {
        // GitHub auto-scopes GITHUB_TOKEN to read-only on fork PRs — excluded.
        let p = parse(
            "on: pull_request
jobs:
  build:
    steps:
      - uses: actions/checkout@v4
        with:
          token: ${{ secrets.GITHUB_TOKEN }}
",
        );
        assert!(UnprotectedSecretsRule::check_workflow(&p).is_empty());
    }

    #[test]
    fn pull_request_target_workflow_with_secret_is_clean() {
        // Covered by dangerous-trigger; reporting both would be noise.
        let p = parse(
            "on: pull_request_target
jobs:
  build:
    steps:
      - uses: docker/login-action@v3
        with:
          password: ${{ secrets.DOCKER_HUB_TOKEN }}
",
        );
        assert!(UnprotectedSecretsRule::check_workflow(&p).is_empty());
    }

    #[test]
    fn workflow_run_workflow_with_secret_is_clean() {
        let p = parse(
            "on:
  workflow_run:
    workflows: [CI]
    types: [completed]
jobs:
  publish:
    steps:
      - run: npm publish --token ${{ secrets.NPM_TOKEN }}
",
        );
        assert!(UnprotectedSecretsRule::check_workflow(&p).is_empty());
    }

    #[test]
    fn non_pr_workflow_with_secret_is_clean() {
        let p = parse(
            "on: push
jobs:
  publish:
    steps:
      - uses: docker/login-action@v3
        with:
          password: ${{ secrets.DOCKER_HUB_TOKEN }}
",
        );
        assert!(UnprotectedSecretsRule::check_workflow(&p).is_empty());
    }

    #[test]
    fn pr_workflow_with_non_canonical_custom_gate_still_errors() {
        // Custom expression doesn't match either canonical fragment; user opts out via ignore.
        let p = parse(
            "on: pull_request
jobs:
  build:
    steps:
      - if: env.IS_FORK != 'true'
        uses: docker/login-action@v3
        with:
          password: ${{ secrets.DOCKER_HUB_TOKEN }}
",
        );
        let diags = UnprotectedSecretsRule::check_workflow(&p);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn job_level_gate_propagates_to_steps() {
        let p = parse(
            "on: pull_request
jobs:
  build:
    if: github.event.pull_request.head.repo.full_name == github.repository
    steps:
      - uses: docker/login-action@v3
        with:
          password: ${{ secrets.DOCKER_HUB_TOKEN }}
",
        );
        assert!(UnprotectedSecretsRule::check_workflow(&p).is_empty());
    }

    #[test]
    fn run_command_reference_is_detected() {
        let p = parse(
            "on: pull_request
jobs:
  build:
    steps:
      - run: echo ${{ secrets.NPM_TOKEN }}
",
        );
        let diags = UnprotectedSecretsRule::check_workflow(&p);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("NPM_TOKEN"));
    }

    #[test]
    fn env_block_reference_is_detected() {
        let p = parse(
            "on: pull_request
jobs:
  build:
    steps:
      - run: ./deploy.sh
        env:
          API_KEY: ${{ secrets.PROD_API_KEY }}
",
        );
        let diags = UnprotectedSecretsRule::check_workflow(&p);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("PROD_API_KEY"));
    }
}
