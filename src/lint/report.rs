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
                workflow: diag.workflow.clone(),
                rule: diag.rule.clone(),
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
                "unpinned",
                Level::Error,
                "actions/checkout@main is not pinned",
            )
            .with_workflow("ci.yml"),
            Diagnostic::new(
                "stale-comment",
                Level::Warn,
                "version comment does not match lock",
            )
            .with_workflow("ci.yml"),
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
}
