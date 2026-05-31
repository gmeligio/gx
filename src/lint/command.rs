use super::report::Report;
use super::rule::{
    Context, Diagnostic, Rule as _, RuleName, format_and_report, is_ignored, matches_ignore,
    run_workflow_rule,
};
use super::sha_mismatch::ShaMismatchRule;
use super::stale_comment::StaleCommentRule;
use super::unpinned::UnpinnedRule;
use super::unsynced_manifest::UnsyncedManifestRule;
use super::workflow_security::{
    DangerousTriggerRule, ExcessivePermissionsRule, MissingConcurrencyRule, MissingPermissionsRule,
    PrHeadCheckoutRule, UnprotectedSecretsRule,
};
use crate::command::Command;
use crate::config::{Config, Level, Lint as LintConfig};
use crate::domain::lock::Lock;
use crate::domain::manifest::Manifest;
use crate::domain::workflow::{Error as WorkflowError, Scanner as WorkflowScanner};
use crate::domain::workflow_actions::{
    ActionSet as WorkflowActionSet, JobId, StepIndex, WorkflowPath,
};
use crate::infra::workflow_scan::FileScanner as FileWorkflowScanner;
use std::path::Path;
use thiserror::Error;

/// Errors that can occur during the lint command.
#[derive(Debug, Error)]
pub enum Error {
    /// A workflow parsing or I/O error occurred.
    #[error(transparent)]
    Workflow(#[from] WorkflowError),
}

/// Run lint checks by scanning workflows and return diagnostics.
///
/// File-local rules (sha-mismatch, unpinned, stale-comment) run per-action during scanning.
/// Global rules (unsynced-manifest) run after the full scan completes.
///
/// # Errors
///
/// Returns [`Error::Workflow`] if a workflow parsing error occurs.
pub fn collect_diagnostics(
    manifest: &Manifest,
    lock: &Lock,
    scanner: &dyn WorkflowScanner,
    lint_config: &LintConfig,
    on_progress: &mut dyn FnMut(&str),
) -> Result<Vec<Diagnostic>, Error> {
    on_progress("Scanning workflows...");
    let sha_mismatch_level = lint_config
        .get_rule(RuleName::ShaMismatch, Level::Error)
        .level;
    let unpinned_level = lint_config.get_rule(RuleName::Unpinned, Level::Error).level;
    let stale_comment_level = lint_config
        .get_rule(RuleName::StaleComment, Level::Warn)
        .level;

    let mut all_diagnostics = Vec::new();
    let mut action_set = WorkflowActionSet::new();

    // Single parse pass yields both per-step action references and the
    // structural Parsed view the workflow-security rules consume.
    let (located, parsed_workflows) = scanner.scan_all_with_parsed()?;

    // Phase 1: per-action rules
    for action in &located {
        if sha_mismatch_level != Level::Off
            && let Some(mut diag) = ShaMismatchRule::check_action(action, lock)
        {
            diag.level = sha_mismatch_level;
            if !is_ignored(
                &diag,
                RuleName::ShaMismatch,
                Level::Error,
                lint_config,
                action,
            ) {
                all_diagnostics.push(diag);
            }
        }
        if unpinned_level != Level::Off
            && let Some(mut diag) = UnpinnedRule::check_action(action)
        {
            diag.level = unpinned_level;
            if !is_ignored(&diag, RuleName::Unpinned, Level::Error, lint_config, action) {
                all_diagnostics.push(diag);
            }
        }
        if stale_comment_level != Level::Off
            && let Some(mut diag) = StaleCommentRule::check_action(action, lock)
        {
            diag.level = stale_comment_level;
            if !is_ignored(
                &diag,
                RuleName::StaleComment,
                Level::Warn,
                lint_config,
                action,
            ) {
                all_diagnostics.push(diag);
            }
        }

        action_set.add(&action.action);
    }

    // Phase 2: action-aggregate rules
    let unsynced_level = lint_config
        .get_rule(RuleName::UnsyncedManifest, Level::Error)
        .level;
    let ctx = Context {
        manifest,
        lock,
        workflows: &located,
        workflows_full: &parsed_workflows,
        action_set: &action_set,
    };
    if unsynced_level != Level::Off {
        let rule = UnsyncedManifestRule;
        for mut diag in rule.check(&ctx) {
            diag.level = unsynced_level;
            let ignored = lint_config
                .get_rule(RuleName::UnsyncedManifest, Level::Error)
                .ignore
                .iter()
                .any(|target| matches_ignore(&diag, target, &located));
            if !ignored {
                all_diagnostics.push(diag);
            }
        }
    }

    // Phase 3: workflow-security rules. Each runs against ctx.workflows_full and emits
    // diagnostics carrying workflow + (optionally) job/step location.
    run_workflow_security_rules(&ctx, lint_config, &mut all_diagnostics);

    // Stable diagnostic ordering: (workflow_path, job_id, step_index, rule_name).
    all_diagnostics.sort_by(|a, b| {
        let aw = a.workflow.as_ref().map_or("", WorkflowPath::as_str);
        let bw = b.workflow.as_ref().map_or("", WorkflowPath::as_str);
        let aj = a.job.as_ref().map_or("", JobId::as_str);
        let bj = b.job.as_ref().map_or("", JobId::as_str);
        let as_ = a.step.map_or(u16::MAX, StepIndex::as_u16);
        let bs = b.step.map_or(u16::MAX, StepIndex::as_u16);
        (aw, aj, as_, a.rule).cmp(&(bw, bj, bs, b.rule))
    });

    Ok(all_diagnostics)
}

/// Run all workflow-security rules and append their diagnostics.
fn run_workflow_security_rules(
    ctx: &Context,
    lint_config: &LintConfig,
    all_diagnostics: &mut Vec<Diagnostic>,
) {
    run_workflow_rule(
        &MissingPermissionsRule,
        Level::Error,
        ctx,
        lint_config,
        all_diagnostics,
    );
    run_workflow_rule(
        &ExcessivePermissionsRule,
        Level::Error,
        ctx,
        lint_config,
        all_diagnostics,
    );
    run_workflow_rule(
        &DangerousTriggerRule,
        Level::Error,
        ctx,
        lint_config,
        all_diagnostics,
    );
    run_workflow_rule(
        &PrHeadCheckoutRule,
        Level::Error,
        ctx,
        lint_config,
        all_diagnostics,
    );
    run_workflow_rule(
        &MissingConcurrencyRule,
        Level::Warn,
        ctx,
        lint_config,
        all_diagnostics,
    );
    run_workflow_rule(
        &UnprotectedSecretsRule,
        Level::Error,
        ctx,
        lint_config,
        all_diagnostics,
    );
}

/// The lint command struct.
pub struct Lint;

impl Command for Lint {
    type Report = Report;
    type Error = Error;

    fn run(
        &self,
        repo_root: &Path,
        config: Config,
        on_progress: &mut dyn FnMut(&str),
    ) -> Result<Report, Error> {
        let scanner = FileWorkflowScanner::new(repo_root);

        let diagnostics = collect_diagnostics(
            &config.manifest,
            &config.lock,
            &scanner,
            &config.lint_config,
            on_progress,
        )?;

        Ok(format_and_report(diagnostics))
    }
}
