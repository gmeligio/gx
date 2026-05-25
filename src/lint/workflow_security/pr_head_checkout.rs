use crate::config::Level;
use crate::domain::workflow_actions::{JobId, StepIndex};
use crate::domain::workflow_parsed::{Job, Parsed, Step};
use crate::lint::{Context, Diagnostic, Rule, RuleName};

/// Expression fragments that pull HEAD code from an untrusted PR. We match textually
/// because GitHub Actions `with:` values are interpolated as strings and any of these
/// fragments anywhere in `with.ref` is a checkout of attacker-controlled code.
const PR_HEAD_REFS: &[&str] = &[
    "github.event.pull_request.head.sha",
    "github.event.pull_request.head.ref",
    "github.head_ref",
];

/// `pr-head-checkout` rule: errors when a privileged workflow checks out the PR head ref.
///
/// "Privileged" means any job declares a write scope in `permissions:` OR any step
/// references `secrets.*`. A checkout is "of PR HEAD" when a step's `with.ref` contains
/// any of `PR_HEAD_REFS`.
pub struct PrHeadCheckoutRule;

impl PrHeadCheckoutRule {
    pub fn check_workflow(workflow: &Parsed) -> Vec<Diagnostic> {
        if !is_privileged(workflow) {
            return Vec::new();
        }
        let mut out = Vec::new();
        for job in &workflow.jobs {
            for (idx, step) in job.steps.iter().enumerate() {
                if checks_out_pr_head(step) {
                    let msg = format!(
                        "{}: job `{}` step {} checks out PR HEAD in a privileged workflow — checking out attacker-controlled code while secrets/write tokens are in scope enables the 'pwn request' attack class",
                        workflow.path, job.id, idx
                    );
                    let mut diag = Diagnostic::new(RuleName::PrHeadCheckout, Level::Error, msg)
                        .with_workflow(workflow.path.clone())
                        .with_job(JobId::from(job.id.clone()));
                    if let Ok(si) = StepIndex::try_from(idx) {
                        diag = diag.with_step(si);
                    }
                    out.push(diag);
                }
            }
        }
        out
    }
}

fn is_privileged(workflow: &Parsed) -> bool {
    workflow.jobs.iter().any(job_has_write_perms)
        || workflow
            .jobs
            .iter()
            .any(|j| j.steps.iter().any(step_references_secrets))
}

fn job_has_write_perms(job: &Job) -> bool {
    job.permissions
        .as_ref()
        .is_some_and(crate::domain::workflow_parsed::Permissions::has_write)
}

fn step_references_secrets(step: &Step) -> bool {
    if let Some(run) = &step.run
        && run.contains("secrets.")
    {
        return true;
    }
    step.with.values().any(|v| v.as_str().contains("secrets."))
        || step.env.values().any(|v| v.as_str().contains("secrets."))
}

fn checks_out_pr_head(step: &Step) -> bool {
    let Some(uses) = step.uses.as_deref() else {
        return false;
    };
    if !uses.starts_with("actions/checkout") {
        return false;
    }
    let Some(ref_value) = step.with.get("ref") else {
        return false;
    };
    PR_HEAD_REFS
        .iter()
        .any(|frag| ref_value.as_str().contains(frag))
}

impl Rule for PrHeadCheckoutRule {
    fn name(&self) -> RuleName {
        RuleName::PrHeadCheckout
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
        let r = PrHeadCheckoutRule;
        assert_eq!(r.name(), RuleName::PrHeadCheckout);
        assert_eq!(r.default_level(), Level::Error);
    }

    #[test]
    fn privileged_workflow_checking_out_pr_head_errors() {
        let p = parse(
            "on: pull_request_target
jobs:
  release:
    permissions:
      contents: write
    steps:
      - uses: actions/checkout@v4
        with:
          ref: ${{ github.event.pull_request.head.sha }}
",
        );
        let diags = PrHeadCheckoutRule::check_workflow(&p);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].level, Level::Error);
        assert_eq!(diags[0].job.as_ref().unwrap().as_str(), "release");
        assert_eq!(diags[0].step.unwrap().as_u16(), 0);
    }

    #[test]
    fn non_privileged_workflow_checking_out_pr_head_is_clean() {
        // Read-only permissions, no secrets — checkout is safe.
        let p = parse(
            "on: pull_request
permissions:
  contents: read
jobs:
  build:
    permissions:
      contents: read
    steps:
      - uses: actions/checkout@v4
        with:
          ref: ${{ github.event.pull_request.head.sha }}
",
        );
        assert!(PrHeadCheckoutRule::check_workflow(&p).is_empty());
    }

    #[test]
    fn privileged_workflow_without_pr_head_ref_is_clean() {
        let p = parse(
            "on: push
jobs:
  publish:
    permissions:
      contents: write
    steps:
      - uses: actions/checkout@v4
",
        );
        assert!(PrHeadCheckoutRule::check_workflow(&p).is_empty());
    }

    #[test]
    fn privileged_via_secret_reference_in_run_triggers_rule() {
        let p = parse(
            "on: pull_request_target
jobs:
  build:
    steps:
      - uses: actions/checkout@v4
        with:
          ref: ${{ github.head_ref }}
      - run: echo ${{ secrets.NPM_TOKEN }}
",
        );
        let diags = PrHeadCheckoutRule::check_workflow(&p);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("step 0"));
    }

    #[test]
    fn head_ref_variant_is_matched() {
        let p = parse(
            "on: pull_request_target
jobs:
  build:
    permissions:
      contents: write
    steps:
      - uses: actions/checkout@v4
        with:
          ref: ${{ github.event.pull_request.head.ref }}
",
        );
        let diags = PrHeadCheckoutRule::check_workflow(&p);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn checkout_without_ref_is_clean_even_if_privileged() {
        // Default-branch checkout under a privileged workflow is safe.
        let p = parse(
            "on: pull_request_target
jobs:
  ci:
    permissions:
      contents: write
    steps:
      - uses: actions/checkout@v4
",
        );
        assert!(PrHeadCheckoutRule::check_workflow(&p).is_empty());
    }
}
