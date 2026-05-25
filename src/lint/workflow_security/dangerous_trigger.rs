use crate::config::Level;
use crate::domain::workflow_parsed::{Parsed, Trigger};
use crate::lint::{Context, Diagnostic, Rule, RuleName};

/// `dangerous-trigger` rule: emits an error per `pull_request_target` or `workflow_run`
/// trigger. Both run in the target-repo context with full secrets and a write-scoped
/// `GITHUB_TOKEN`, and both are reachable from fork PRs.
pub struct DangerousTriggerRule;

impl DangerousTriggerRule {
    /// Yields one diagnostic per matched trigger so the user can act on each line.
    pub fn check_workflow(workflow: &Parsed) -> Vec<Diagnostic> {
        workflow
            .on
            .iter()
            .filter_map(|t| match t {
                Trigger::PullRequestTarget => Some(Self::diagnostic(
                    workflow,
                    "pull_request_target",
                    "prefer `pull_request` for code-execution workflows; `pull_request_target` runs PR jobs with the base-repo token and secrets",
                )),
                Trigger::WorkflowRun => Some(Self::diagnostic(
                    workflow,
                    "workflow_run",
                    "runs in the target-repo context with secrets and is triggerable by fork PRs; `github.repository == ...` guards do not mitigate the risk",
                )),
                _ => None,
            })
            .collect()
    }

    fn diagnostic(workflow: &Parsed, trigger: &str, hint: &str) -> Diagnostic {
        let msg = format!(
            "{}: dangerous trigger `{}` — {}",
            workflow.path, trigger, hint
        );
        Diagnostic::new(RuleName::DangerousTrigger, Level::Error, msg)
            .with_workflow(workflow.path.clone())
    }
}

impl Rule for DangerousTriggerRule {
    fn name(&self) -> RuleName {
        RuleName::DangerousTrigger
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
        let r = DangerousTriggerRule;
        assert_eq!(r.name(), RuleName::DangerousTrigger);
        assert_eq!(r.default_level(), Level::Error);
    }

    #[test]
    fn pull_request_only_is_clean() {
        let p = parse("on: pull_request\njobs: {}\n");
        assert!(DangerousTriggerRule::check_workflow(&p).is_empty());
    }

    #[test]
    fn pull_request_target_errors() {
        let p = parse("on: pull_request_target\njobs: {}\n");
        let diags = DangerousTriggerRule::check_workflow(&p);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].level, Level::Error);
        assert!(diags[0].message.contains("pull_request_target"));
    }

    #[test]
    fn workflow_run_errors() {
        let p = parse(
            "on:
  workflow_run:
    workflows: [CI]
    types: [completed]
jobs: {}
",
        );
        let diags = DangerousTriggerRule::check_workflow(&p);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("workflow_run"));
        assert!(diags[0].message.contains("github.repository"));
    }

    #[test]
    fn both_triggers_in_one_file_produce_two_diagnostics() {
        let p = parse(
            "on:
  pull_request_target:
    types: [opened]
  workflow_run:
    workflows: [CI]
jobs: {}
",
        );
        let diags = DangerousTriggerRule::check_workflow(&p);
        assert_eq!(diags.len(), 2);
        let triggers: Vec<&str> = diags
            .iter()
            .map(|d| {
                if d.message.contains("pull_request_target") {
                    "pull_request_target"
                } else {
                    "workflow_run"
                }
            })
            .collect();
        assert!(triggers.contains(&"pull_request_target"));
        assert!(triggers.contains(&"workflow_run"));
    }

    #[test]
    fn push_only_is_clean() {
        let p = parse("on: push\njobs: {}\n");
        assert!(DangerousTriggerRule::check_workflow(&p).is_empty());
    }
}
