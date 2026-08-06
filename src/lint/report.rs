use super::Diagnostic;
use crate::command::CommandReport;
use crate::config::Level;
use crate::output::lines::Line as OutputLine;

/// Report from the lint command.
#[derive(Debug, Default)]
pub struct Report {
    /// All diagnostics found.
    pub diagnostics: Vec<Diagnostic>,
    /// Number of error-level diagnostics.
    pub error_count: usize,
    /// Number of warning-level diagnostics.
    pub warning_count: usize,
}

impl Report {
    /// Build a `Report` from a list of diagnostics.
    #[must_use]
    pub fn from_diagnostics(diagnostics: Vec<Diagnostic>) -> Self {
        let error_count = diagnostics
            .iter()
            .filter(|d| d.level == Level::Error)
            .count();
        let warning_count = diagnostics
            .iter()
            .filter(|d| d.level == Level::Warn)
            .count();
        Self {
            diagnostics,
            error_count,
            warning_count,
        }
    }
}

impl CommandReport for Report {
    fn render(&self) -> Vec<OutputLine> {
        if self.diagnostics.is_empty() {
            return vec![OutputLine::Summary {
                text: "No lint issues found".to_owned(),
            }];
        }

        let mut lines = Vec::new();

        for diag in &self.diagnostics {
            lines.push(OutputLine::LintDiag {
                level: diag.level,
                workflow: diag.workflow.as_ref().map(std::string::ToString::to_string),
                line: diag.line,
                rule: diag.rule.to_string(),
                message: diag.message.clone(),
            });
        }

        lines.push(OutputLine::Blank);

        let err_count = self.error_count;
        let warn_count = self.warning_count;
        let summary = match (err_count, warn_count) {
            (0, 0) => "No lint issues found".to_owned(),
            (errs, 0) => format!("{errs} error{}", if errs == 1 { "" } else { "s" }),
            (0, warns) => format!("{warns} warning{}", if warns == 1 { "" } else { "s" }),
            (errs, warns) => format!(
                "{} error{} · {} warning{}",
                errs,
                if errs == 1 { "" } else { "s" },
                warns,
                if warns == 1 { "" } else { "s" }
            ),
        };
        lines.push(OutputLine::Summary { text: summary });

        lines
    }

    fn exit_code(&self) -> i32 {
        i32::from(self.error_count > 0)
    }
}

#[cfg(test)]
#[expect(
    clippy::indexing_slicing,
    reason = "tests use unwrap, indexing, and other patterns freely"
)]
mod tests {
    use super::*;
    use crate::domain::action::identity::{ActionId, CommitDate, CommitSha, Repository, Version};
    use crate::domain::action::resolved::{Commit, ResolvedRef};
    use crate::domain::action::spec::Spec;
    use crate::domain::action::specifier::Specifier;
    use crate::domain::action::uses_ref::ParsedRef;
    use crate::domain::file::actions::{ActionSet, Located, WorkflowAction};
    use crate::domain::file::site::{Id, JobId, Origin, Slot, StepIndex, WorkflowPath};
    use crate::domain::lock::Lock;
    use crate::domain::manifest::Manifest;
    use crate::lint::rule::Rule as _;
    use crate::lint::{Context, RuleName};
    use std::collections::HashMap;

    /// Kind-nouns a producer must not use to label an identifier it then names.
    const KIND_NOUNS: [&str; 3] = ["action", "workflow", "component"];

    #[test]
    fn render_lint_clean() {
        let report = Report::default();
        let lines = report.render();
        assert_eq!(lines.len(), 1);
        assert!(
            matches!(&lines[0], OutputLine::Summary { text } if text == "No lint issues found")
        );
    }

    #[test]
    fn render_lint_with_violations() {
        let diagnostics = vec![
            Diagnostic::new(
                RuleName::Unpinned,
                Level::Error,
                "actions/checkout@main is not pinned",
            )
            .with_workflow(WorkflowPath::new("ci.yml")),
            Diagnostic::new(
                RuleName::StaleComment,
                Level::Warn,
                "version comment does not match lock",
            )
            .with_workflow(WorkflowPath::new("ci.yml")),
        ];
        let report = Report::from_diagnostics(diagnostics);
        let lines = report.render();

        assert!(lines.iter().any(|l| matches!(
            l,
            OutputLine::LintDiag {
                level: Level::Error,
                ..
            }
        )));
        assert!(lines.iter().any(|l| matches!(
            l,
            OutputLine::LintDiag {
                level: Level::Warn,
                ..
            }
        )));
        assert!(lines.contains(&OutputLine::Summary {
            text: "1 error · 1 warning".to_owned(),
        }));
    }

    /// Build a `Located` for `actions/checkout` with the given reference shape.
    fn located(reference: ParsedRef) -> Located {
        Located {
            action: WorkflowAction {
                id: ActionId::from("actions/checkout"),
                reference,
            },
            site: Id {
                file: WorkflowPath::new(".github/workflows/ci.yml"),
                slot: Slot::WorkflowStep {
                    job: JobId::from("build"),
                    step: StepIndex::from(0_u16),
                },
            },
            origin: Origin { line: Some(7) },
        }
    }

    /// Collect messages the four action-scoped rules actually produce.
    ///
    /// Every string here comes from a rule's own `format!` — never from a literal
    /// written in this test — which is what makes the assertion below a guard
    /// rather than a restatement of what the author typed.
    fn rule_produced_diagnostics() -> Vec<Diagnostic> {
        let sha = CommitSha::from("8e8c483db84b4bee98b60c0593521ed34d9990e8");
        let mut out = Vec::new();

        // unpinned: a bare tag ref is not a SHA pin.
        out.extend(crate::lint::unpinned::UnpinnedRule::check_action(&located(
            ParsedRef::Ref(Version::from("v4")),
        )));

        // sha-mismatch: a bare SHA absent from an empty lock.
        let empty_lock = Lock::new(HashMap::new());
        out.extend(crate::lint::sha_mismatch::ShaMismatchRule::check_action(
            &located(ParsedRef::Sha(sha.clone())),
            &empty_lock,
        ));

        // stale-comment: a pinned SHA that disagrees with the locked one.
        let spec = Spec::new(ActionId::from("actions/checkout"), Specifier::from_v1("v4"));
        let mut lock = Lock::new(HashMap::new());
        lock.set(
            &spec,
            ResolvedRef::Tag(Version::from("v4")),
            Commit {
                sha: CommitSha::from("1111111111111111111111111111111111111111"),
                repository: Repository::from("actions/checkout"),
                ref_type: None,
                date: CommitDate::from("2024-01-01T00:00:00Z"),
            },
        );
        out.extend(crate::lint::stale_comment::StaleCommentRule::check_action(
            &located(ParsedRef::Pinned {
                sha,
                comment: Version::from("v4"),
            }),
            &lock,
        ));

        // unsynced-manifest: both directions — one action only in a workflow,
        // one only in the manifest.
        let mut action_set = ActionSet::new();
        action_set.add(&WorkflowAction {
            id: ActionId::from("actions/only-in-workflow"),
            reference: ParsedRef::Ref(Version::from("v1")),
        });
        let mut manifest = Manifest::new(HashMap::new());
        manifest.set(
            ActionId::from("actions/only-in-manifest"),
            Specifier::from_v1("v1"),
        );
        let ctx = Context {
            manifest: &manifest,
            lock: &empty_lock,
            workflows: &[],
            workflows_full: &[],
            action_set: &action_set,
        };
        out.extend(crate::lint::unsynced_manifest::UnsyncedManifestRule.check(&ctx));

        out
    }

    #[test]
    fn rendered_diagnostics_carry_no_kind_noun() {
        // The renderer owns user-facing vocabulary: a rule must not label an
        // identifier with the noun naming its kind, or the noun is fixed upstream
        // where the renderer cannot change it. Sibling of
        // `format_line_lint_diag_renders_location_once` in `output/lines.rs`,
        // which guards the same discipline for the workflow path.
        //
        // Asserted against messages the rules actually produced, so restoring an
        // `action ` prefix in any rule below turns this red.
        let diagnostics = rule_produced_diagnostics();
        assert!(
            diagnostics.len() >= 5,
            "fixture should exercise all four offending rules (unsynced-manifest \
             emits two), got {} diagnostics",
            diagnostics.len()
        );

        let report = Report::from_diagnostics(diagnostics);
        for line in report.render() {
            let OutputLine::LintDiag { message, .. } = line else {
                continue;
            };
            for noun in KIND_NOUNS {
                // Only a noun *labelling an identifier* is the defect: `action
                // actions/checkout uses ...` repeats what the identifier already
                // says. A noun carrying a sentence that names no identifier
                // (`workflow has no top-level permissions: block`) is legitimate,
                // so require a following token that looks like an `owner/repo`.
                let Some(rest) = message.strip_prefix(noun).and_then(|r| r.strip_prefix(' '))
                else {
                    continue;
                };
                let labels_an_id = rest
                    .split_whitespace()
                    .next()
                    .is_some_and(|word| word.contains('/'));
                assert!(
                    !labels_an_id,
                    "rule message must not prefix an identifier with `{noun}` — the \
                     renderer owns that vocabulary. Got: {message}"
                );
            }
        }
    }
}
