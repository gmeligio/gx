use crate::config::Level;
use crate::domain::workflow_actions::{JobId, StepIndex};
use crate::domain::workflow_parsed::{Job, Parsed, Step};
use crate::lint::{Context, Diagnostic, Rule, RuleName};
use crate::regex::static_regex;
use std::collections::BTreeSet;

/// `invalid-expression` rule: flags `${{ }}` references to `needs.<job>` / `steps.<id>`
/// that cannot resolve at run time. GitHub Actions resolves an unresolvable reference to
/// an empty string silently — the classic "my output is mysteriously blank" failure.
///
/// Conservative by construction: only bare-identifier dotted access is resolved, so a
/// missed shape is a false negative (silence), never a false positive.
pub struct InvalidExpressionRule;

// Matches a `needs.<id>` or `steps.<id>` reference, optionally followed by
// `.outputs.<key>`, anchored so the context word is not part of a longer identifier and
// the segment after it is a bare identifier (dot access, not `[` indexing). `needs[...]`
// and `steps[...]` therefore do not match — they are dynamic and intentionally skipped.
static_regex!(
    REF_RE,
    r"(?:^|[^A-Za-z0-9_.])(needs|steps)\.([A-Za-z_][A-Za-z0-9_-]*)(?:\.outputs\.([A-Za-z_][A-Za-z0-9_-]*))?"
);

// Matches a `${{ ... }}` expression span; capture group 1 is the inner text.
static_regex!(EXPR_SPAN_RE, r"\$\{\{(.*?)\}\}");

impl InvalidExpressionRule {
    /// Flags every unresolvable `needs.*` / `steps.*` reference across all jobs.
    pub fn check_workflow(workflow: &Parsed) -> Vec<Diagnostic> {
        let mut out = Vec::new();
        for job in &workflow.jobs {
            let needs: BTreeSet<&str> = job.needs.iter().map(String::as_str).collect();
            // Step ids accumulate as we walk steps in order, so a reference to a later
            // step's id is unresolved (the step hasn't run yet).
            let mut declared_ids: BTreeSet<&str> = BTreeSet::new();
            for (idx, step) in job.steps.iter().enumerate() {
                for message in step_findings(workflow, job, &needs, &declared_ids, step) {
                    let mut diag =
                        Diagnostic::new(RuleName::InvalidExpression, Level::Error, message)
                            .with_workflow(workflow.path.clone())
                            .with_job(JobId::from(job.id.clone()));
                    if let Ok(si) = StepIndex::try_from(idx) {
                        diag = diag.with_step(si);
                    }
                    out.push(diag);
                }
                if let Some(id) = &step.id {
                    declared_ids.insert(id.as_str());
                }
            }
        }
        out
    }
}

/// Resolves every `needs.*` / `steps.*` reference in a single step's scannable fields,
/// returning a diagnostic message for each broken one.
fn step_findings(
    workflow: &Parsed,
    job: &Job,
    needs: &BTreeSet<&str>,
    declared_ids: &BTreeSet<&str>,
    step: &Step,
) -> Vec<String> {
    let mut out = Vec::new();
    let mut scan = |text: &str| {
        for span in EXPR_SPAN_RE.captures_iter(text) {
            let inner = &span[1];
            for cap in REF_RE.captures_iter(inner) {
                if let Some(message) = resolve_ref(workflow, job, needs, declared_ids, &cap) {
                    out.push(message);
                }
            }
        }
    };
    if let Some(if_cond) = &step.if_cond {
        scan(if_cond);
    }
    for v in step.with.values() {
        scan(v.as_str());
    }
    for v in step.env.values() {
        scan(v.as_str());
    }
    if let Some(run) = &step.run {
        scan(run);
    }
    out
}

/// Resolves a single captured reference. Returns a diagnostic message when it is broken.
fn resolve_ref(
    workflow: &Parsed,
    job: &Job,
    needs: &BTreeSet<&str>,
    declared_ids: &BTreeSet<&str>,
    cap: &regex::Captures,
) -> Option<String> {
    let context = &cap[1];
    let id = &cap[2];
    let output_key = cap.get(3).map(|m| m.as_str());
    match context {
        "needs" => resolve_needs(workflow, job, needs, id, output_key),
        "steps" => resolve_steps(job, declared_ids, id),
        _ => None,
    }
}

/// Resolves `needs.<id>` and, when present, `needs.<id>.outputs.<key>`.
fn resolve_needs(
    workflow: &Parsed,
    job: &Job,
    needs: &BTreeSet<&str>,
    id: &str,
    output_key: Option<&str>,
) -> Option<String> {
    if !needs.contains(id) {
        return Some(format!(
            "job `{}` references `needs.{id}` but `{id}` is not in its `needs:` list — the reference resolves to nothing at run time",
            job.id
        ));
    }
    // The job IS a declared dependency. Validate the output key only when the producing
    // job declares a non-empty inline `outputs:` map. An empty map (a `uses:` reusable-
    // workflow job, whose outputs live in the called file) means we cannot resolve the
    // key — fall back to job-existence only and do not flag.
    let key = output_key?;
    let producer = workflow.jobs.iter().find(|j| j.id == id)?;
    if producer.outputs.is_empty() || producer.outputs.contains_key(key) {
        return None;
    }
    Some(format!(
        "job `{}` references `needs.{id}.outputs.{key}` but job `{id}` declares no `{key}` output — the reference resolves to nothing at run time",
        job.id
    ))
}

/// Resolves `steps.<id>` against the ids declared by earlier steps in the same job.
/// The output key (`steps.<id>.outputs.<key>`) is never resolved — out of scope by design.
fn resolve_steps(job: &Job, declared_ids: &BTreeSet<&str>, id: &str) -> Option<String> {
    if declared_ids.contains(id) {
        return None;
    }
    Some(format!(
        "job `{}` references `steps.{id}` but no earlier step declares `id: {id}` — the reference resolves to nothing at run time",
        job.id
    ))
}

impl Rule for InvalidExpressionRule {
    fn name(&self) -> RuleName {
        RuleName::InvalidExpression
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
        let r = InvalidExpressionRule;
        assert_eq!(r.name(), RuleName::InvalidExpression);
        assert_eq!(r.default_level(), Level::Error);
    }

    #[test]
    fn expression_reads_undeclared_needs_job_is_flagged() {
        // Spec: pr declares needs: [compose] but reads needs.validate.outputs.id.
        let p = parse(
            "on: push
jobs:
  validate:
    outputs:
      id: ${{ steps.x.outputs.id }}
    steps: []
  compose:
    steps: []
  pr:
    needs: [compose]
    steps:
      - run: echo ${{ needs.validate.outputs.id }}
",
        );
        let diags = InvalidExpressionRule::check_workflow(&p);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].level, Level::Error);
        assert!(diags[0].message.contains("needs.validate"));
        assert_eq!(diags[0].job.as_ref().unwrap().as_str(), "pr");
    }

    #[test]
    fn expression_reads_nonexistent_step_id_is_flagged() {
        let p = parse(
            "on: push
jobs:
  build:
    steps:
      - run: echo ${{ steps.upload.outputs.artifact-id }}
",
        );
        let diags = InvalidExpressionRule::check_workflow(&p);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("steps.upload"));
    }

    #[test]
    fn expression_reads_valid_step_id_is_clean() {
        let p = parse(
            "on: push
jobs:
  build:
    steps:
      - id: upload
        run: echo hi
      - run: echo ${{ steps.upload.outputs.artifact-id }}
",
        );
        assert!(InvalidExpressionRule::check_workflow(&p).is_empty());
    }

    #[test]
    fn expression_reads_later_step_id_is_flagged() {
        // The producing step runs AFTER the reference — unresolved at run time.
        let p = parse(
            "on: push
jobs:
  build:
    steps:
      - run: echo ${{ steps.upload.outputs.x }}
      - id: upload
        run: echo hi
",
        );
        let diags = InvalidExpressionRule::check_workflow(&p);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("steps.upload"));
    }

    #[test]
    fn expression_reads_nonexistent_job_output_key_is_flagged() {
        // build declares outputs: {sha}; deploy reads needs.build.outputs.shaa (typo).
        let p = parse(
            "on: push
jobs:
  build:
    outputs:
      sha: ${{ steps.x.outputs.sha }}
    steps: []
  deploy:
    needs: [build]
    steps:
      - run: echo ${{ needs.build.outputs.shaa }}
",
        );
        let diags = InvalidExpressionRule::check_workflow(&p);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("shaa"));
        assert_eq!(diags[0].job.as_ref().unwrap().as_str(), "deploy");
    }

    #[test]
    fn expression_reads_valid_job_output_key_is_clean() {
        let p = parse(
            "on: push
jobs:
  build:
    outputs:
      sha: ${{ steps.x.outputs.sha }}
    steps: []
  deploy:
    needs: [build]
    steps:
      - run: echo ${{ needs.build.outputs.sha }}
",
        );
        assert!(InvalidExpressionRule::check_workflow(&p).is_empty());
    }

    #[test]
    fn output_key_of_reusable_workflow_job_is_not_flagged() {
        // release is a `uses:` job with no inline outputs map — its outputs live in the
        // called file. We must fall back to job-existence only and not flag the key.
        let p = parse(
            "on: push
jobs:
  release:
    uses: ./.github/workflows/release.yml
  notify:
    needs: [release]
    steps:
      - run: echo ${{ needs.release.outputs.url }}
",
        );
        assert!(InvalidExpressionRule::check_workflow(&p).is_empty());
    }

    #[test]
    fn dynamic_reference_is_not_flagged() {
        // needs[matrix.target] is dynamic — the job segment is not a bare identifier.
        let p = parse(
            "on: push
jobs:
  build:
    steps:
      - run: echo ${{ needs[matrix.target].outputs.x }}
",
        );
        assert!(InvalidExpressionRule::check_workflow(&p).is_empty());
    }

    #[test]
    fn out_of_scope_contexts_are_not_flagged() {
        let p = parse(
            "on: push
jobs:
  build:
    steps:
      - run: echo ${{ env.FLUTTER_VERSION_PATH }} ${{ matrix.os }}
",
        );
        assert!(InvalidExpressionRule::check_workflow(&p).is_empty());
    }

    #[test]
    fn bare_needs_without_declared_needs_is_flagged() {
        // The job declares no `needs:` at all — any needs.* reference is unresolvable.
        let p = parse(
            "on: push
jobs:
  build:
    steps: []
  deploy:
    steps:
      - run: echo ${{ needs.build.outputs.x }}
",
        );
        let diags = InvalidExpressionRule::check_workflow(&p);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("needs.build"));
    }

    #[test]
    fn longer_identifier_is_not_mistaken_for_a_prefix() {
        // `needs.build-extra` must not be read as `needs.build`; the bare-id regex is
        // maximal. Here build-extra is undeclared so it IS flagged, but as build-extra.
        let p = parse(
            "on: push
jobs:
  build:
    steps: []
  deploy:
    needs: [build]
    steps:
      - run: echo ${{ needs.build-extra.outputs.x }}
",
        );
        let diags = InvalidExpressionRule::check_workflow(&p);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("build-extra"));
    }
}
