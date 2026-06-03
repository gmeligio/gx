use crate::config::Level;
use crate::domain::workflow_actions::JobId;
use crate::domain::workflow_parsed::Parsed;
use crate::lint::{Context, Diagnostic, Rule, RuleName};
use std::collections::BTreeSet;

/// `dangling-reference` rule: flags a job whose `needs:` lists a job id that does not
/// exist in the workflow. GitHub Actions accepts this at parse time and fails the run
/// with an "unknown job" error only when the workflow is dispatched; this catches it
/// statically.
pub struct DanglingReferenceRule;

impl DanglingReferenceRule {
    /// Returns one diagnostic per dangling `needs:` entry across all jobs in the workflow.
    pub fn check_workflow(workflow: &Parsed) -> Vec<Diagnostic> {
        let job_ids: BTreeSet<&str> = workflow.jobs.iter().map(|j| j.id.as_str()).collect();
        let mut out = Vec::new();
        for job in &workflow.jobs {
            for needed in &job.needs {
                if !job_ids.contains(needed.as_str()) {
                    let msg = format!(
                        "{}: job `{}` needs job `{}` which does not exist in this workflow",
                        workflow.path, job.id, needed
                    );
                    out.push(
                        Diagnostic::new(RuleName::DanglingReference, Level::Error, msg)
                            .with_workflow(workflow.path.clone())
                            .with_job(JobId::from(job.id.clone())),
                    );
                }
            }
        }
        out
    }
}

impl Rule for DanglingReferenceRule {
    fn name(&self) -> RuleName {
        RuleName::DanglingReference
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
#[expect(clippy::unwrap_used, reason = "tests use unwrap freely")]
mod tests {
    use super::*;
    use crate::domain::workflow_actions::WorkflowPath;

    fn parse(content: &str) -> Parsed {
        Parsed::from_yaml(WorkflowPath::new(".github/workflows/ci.yml"), content).unwrap()
    }

    #[test]
    fn rule_metadata() {
        let r = DanglingReferenceRule;
        assert_eq!(r.name(), RuleName::DanglingReference);
        assert_eq!(r.default_level(), Level::Error);
    }

    #[test]
    fn needs_nonexistent_job_is_flagged() {
        // Spec scenario: deploy needs `buld` (typo for build), which does not exist.
        let p = parse(
            "on: push
jobs:
  build:
    steps: []
  deploy:
    needs: [buld]
    steps: []
",
        );
        let diags = DanglingReferenceRule::check_workflow(&p);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].level, Level::Error);
        assert!(diags[0].message.contains("buld"));
        assert_eq!(diags[0].job.as_ref().unwrap().as_str(), "deploy");
    }

    #[test]
    fn needs_scalar_to_real_job_is_clean() {
        // Spec scenario: needs as a scalar pointing at an existing job is accepted.
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
        assert!(DanglingReferenceRule::check_workflow(&p).is_empty());
    }

    #[test]
    fn sequence_with_one_bad_entry_flags_only_the_bad_one() {
        let p = parse(
            "on: push
jobs:
  build:
    steps: []
  lint:
    steps: []
  deploy:
    needs: [build, lnit]
    steps: []
",
        );
        let diags = DanglingReferenceRule::check_workflow(&p);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("lnit"));
        assert!(!diags[0].message.contains("build"));
    }

    #[test]
    fn job_with_no_needs_is_clean() {
        let p = parse("on: push\njobs:\n  build:\n    steps: []\n");
        assert!(DanglingReferenceRule::check_workflow(&p).is_empty());
    }
}
