use crate::lint::{Context, Diagnostic, Rule, RuleName};
use crate::config::Level;
use crate::domain::workflow_parsed::{Parsed, Trigger};

/// `missing-concurrency` rule: warns when a workflow triggered by `push:` or `schedule:`
/// has no top-level `concurrency:` block. Without one, two runs racing on the same ref
/// can overwrite each other (registry pushes, tag creation, deploys).
pub struct MissingConcurrencyRule;

impl MissingConcurrencyRule {
    pub fn check_workflow(workflow: &Parsed) -> Option<Diagnostic> {
        let race_trigger = workflow.on.iter().find_map(|t| match t {
            Trigger::Push => Some("push"),
            Trigger::Schedule => Some("schedule"),
            _ => None,
        })?;
        if workflow.concurrency.is_some() {
            return None;
        }
        let msg = format!(
            "{}: workflow triggered by `{}` has no `concurrency:` block — add one (e.g. `concurrency: {{ group: ${{{{ github.workflow }}}}-${{{{ github.ref }}}}, cancel-in-progress: true }}`) to serialize racing runs",
            workflow.path, race_trigger
        );
        Some(
            Diagnostic::new(RuleName::MissingConcurrency, Level::Warn, msg)
                .with_workflow(workflow.path.clone()),
        )
    }
}

impl Rule for MissingConcurrencyRule {
    fn name(&self) -> RuleName {
        RuleName::MissingConcurrency
    }

    fn default_level(&self) -> Level {
        Level::Warn
    }

    fn check(&self, ctx: &Context) -> Vec<Diagnostic> {
        ctx.workflows_full
            .iter()
            .filter_map(Self::check_workflow)
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
        let r = MissingConcurrencyRule;
        assert_eq!(r.name(), RuleName::MissingConcurrency);
        assert_eq!(r.default_level(), Level::Warn);
    }

    #[test]
    fn push_without_concurrency_warns() {
        let p = parse("on: push\njobs: {}\n");
        let d = MissingConcurrencyRule::check_workflow(&p).unwrap();
        assert_eq!(d.level, Level::Warn);
        assert!(d.message.contains("push"));
    }

    #[test]
    fn push_with_concurrency_is_clean() {
        let p = parse(
            "on: push
concurrency:
  group: ci-${{ github.ref }}
  cancel-in-progress: true
jobs: {}
",
        );
        assert!(MissingConcurrencyRule::check_workflow(&p).is_none());
    }

    #[test]
    fn pull_request_without_concurrency_is_clean() {
        // Rule applies only to push/schedule.
        let p = parse("on: pull_request\njobs: {}\n");
        assert!(MissingConcurrencyRule::check_workflow(&p).is_none());
    }

    #[test]
    fn schedule_without_concurrency_warns() {
        let p = parse(
            "on:
  schedule:
    - cron: '0 0 * * *'
jobs: {}
",
        );
        let d = MissingConcurrencyRule::check_workflow(&p).unwrap();
        assert!(d.message.contains("schedule"));
    }
}
