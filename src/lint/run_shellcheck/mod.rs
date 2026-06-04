//! `run-shellcheck` rule: runs the `shellcheck` static analyzer over the shell body of
//! each `run:` step whose effective shell is bash or sh, surfacing shell bugs (unquoted
//! expansions, masked pipeline failures, ...) at lint time. Mirrors actionlint's shellcheck
//! integration. The subprocess lives behind the [`ShellChecker`] seam so this rule's logic
//! is pure and unit-testable without the binary on `PATH`.

use crate::config::Level;
use crate::domain::workflow_actions::{JobId, StepIndex};
use crate::domain::workflow_parsed::{Defaults, Parsed, effective_shell};
use super::{Context, Diagnostic, Rule, RuleName};
use crate::infra::shellcheck::{
    Availability, Finding, Severity, Sh, ShellChecker, sanitize_expressions,
};

/// The `run-shellcheck` rule. Owns the probed [`Availability`] so the binary is looked up
/// once per lint run; when absent the rule emits a single skip diagnostic instead of
/// failing.
pub struct RunShellcheckRule {
    /// Probed once on construction: whether `shellcheck` is usable on this host.
    availability: Availability,
}

impl RunShellcheckRule {
    /// Construct the rule, probing `PATH` for `shellcheck` once.
    #[must_use]
    pub fn new() -> Self {
        Self {
            availability: Availability::probe(),
        }
    }
}

impl Default for RunShellcheckRule {
    fn default() -> Self {
        Self::new()
    }
}

impl Rule for RunShellcheckRule {
    fn name(&self) -> RuleName {
        RuleName::RunShellcheck
    }

    fn default_level(&self) -> Level {
        Level::Warn
    }

    fn check(&self, ctx: &Context) -> Vec<Diagnostic> {
        match &self.availability {
            Availability::Absent => vec![skip_diagnostic()],
            Availability::Present(cli) => check_workflows(cli, ctx.workflows_full),
        }
    }
}

/// The single informational diagnostic emitted when `shellcheck` is not installed. Carries
/// no workflow location — it is about the rule, not a finding. Level is set by the runner;
/// the message explains why the rule did not fire.
fn skip_diagnostic() -> Diagnostic {
    Diagnostic::new(
        RuleName::RunShellcheck,
        Level::Warn,
        "run-shellcheck skipped: `shellcheck` binary not found on PATH",
    )
}

/// Core rule logic, factored out of the trait so tests can drive it with any
/// [`ShellChecker`] (e.g. a `FakeChecker`) without a live binary. Returns one diagnostic
/// per shellcheck finding across every analyzable `run:` step.
fn check_workflows(checker: &dyn ShellChecker, workflows: &[Parsed]) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    for wf in workflows {
        for job in &wf.jobs {
            for (index, step) in job.steps.iter().enumerate() {
                let Some(body) = &step.run else { continue };
                let Some(shell) = resolve_shell(step.shell.as_deref(), job.defaults.as_ref(), wf.defaults.as_ref())
                else {
                    continue; // non-shell (pwsh/python/...) → skipped
                };
                let sanitized = sanitize_expressions(body);
                for finding in checker.check(&sanitized, shell) {
                    out.push(finding_to_diagnostic(&finding, wf, &job.id, index));
                }
            }
        }
    }
    out
}

/// Resolve a step's effective shell to a [`Sh`], or `None` if it is a non-POSIX shell that
/// shellcheck cannot analyze (in which case the step is skipped).
fn resolve_shell(
    step_shell: Option<&str>,
    job_defaults: Option<&Defaults>,
    workflow_defaults: Option<&Defaults>,
) -> Option<Sh> {
    let token = effective_shell(step_shell, job_defaults, workflow_defaults);
    Sh::from_token(&token)
}

/// Map one shellcheck [`Finding`] to a step-scoped [`Diagnostic`]. The YAML line cannot be
/// reliably recovered from a block scalar, so the step is the locus and shellcheck's
/// in-script line + `SCxxxx` code go in the message as secondary context.
fn finding_to_diagnostic(
    finding: &Finding,
    workflow: &Parsed,
    job_id: &str,
    step_index: usize,
) -> Diagnostic {
    let message = format!(
        "SC{code} ({severity}) at script line {line}: {msg}",
        code = finding.code,
        severity = severity_label(finding.severity),
        line = finding.line,
        msg = finding.message,
    );
    Diagnostic::new(RuleName::RunShellcheck, Level::Warn, message)
        .with_workflow(workflow.path.clone())
        .with_job(JobId::from(job_id))
        .with_step(StepIndex::from(clamp_step_index(step_index)))
}

/// A short human label for a shellcheck severity, used in the diagnostic message.
fn severity_label(severity: Severity) -> &'static str {
    match severity {
        Severity::Error => "error",
        Severity::Warning => "warning",
        Severity::Info => "info",
        Severity::Style => "style",
    }
}

/// Clamp a `usize` step index into the `u16` `StepIndex` carries. Real workflows never
/// approach `u16::MAX` steps; clamping avoids a fallible conversion on the hot path.
fn clamp_step_index(index: usize) -> u16 {
    u16::try_from(index).unwrap_or(u16::MAX)
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "tests use unwrap and indexing freely"
)]
mod tests;
