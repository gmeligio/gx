//! Audit's findings and the report that aggregates them.
//!
//! The record, the severity counting, the exit code, and the summary pluralization are all
//! [`crate::diagnostic`]'s, parameterized by audit's own [`CheckName`]. Only the rendering
//! and the JSON contract are audit-specific.

use super::check_name::CheckName;
use crate::command::CommandReport;
use crate::config::Level;
use crate::output::lines::Line as OutputLine;
use serde::Serialize;

/// A single audit finding — the shared diagnostic record carrying audit's check identity.
pub type Finding = crate::diagnostic::Diagnostic<CheckName>;

/// A set of audit findings with their severity counts.
pub type Report = crate::diagnostic::Report<CheckName>;

/// Text shown when an audit run found nothing.
const NO_ISSUES: &str = "No audit findings";

impl CommandReport for Report {
    fn render(&self) -> Vec<OutputLine> {
        if self.diagnostics.is_empty() {
            return vec![OutputLine::Summary {
                text: NO_ISSUES.to_owned(),
            }];
        }

        let mut lines: Vec<OutputLine> = self
            .diagnostics
            .iter()
            .map(|finding| OutputLine::LintDiag {
                level: finding.level,
                workflow: finding.workflow.as_ref().map(ToString::to_string),
                line: finding.line,
                rule: finding.rule.to_string(),
                message: finding.message.clone(),
            })
            .collect();

        lines.push(OutputLine::Blank);
        lines.push(OutputLine::Summary {
            text: self.summary(NO_ISSUES),
        });

        lines
    }

    fn exit_code(&self) -> i32 {
        Self::exit_code(self)
    }
}

/// One finding as it appears in `--json` output.
///
/// A distinct type from [`Finding`] rather than a `Serialize` derive on the shared
/// diagnostic: these key names are a published contract for CI consumers, so they must not
/// shift when the internal record gains or renames a field.
#[derive(Serialize)]
struct JsonFinding<'find> {
    /// The check that produced this finding, e.g. `mutable-ref`.
    check: CheckName,
    /// Severity, as `error` or `warn`.
    level: Level,
    /// Human-readable description.
    message: &'find str,
}

/// The `--json` document.
#[derive(Serialize)]
struct JsonReport<'rep> {
    /// Every finding. Always present, empty when nothing was found, so a consumer can
    /// index it unconditionally.
    findings: Vec<JsonFinding<'rep>>,
    /// How many findings were error-level.
    error_count: usize,
    /// How many findings were warning-level.
    warning_count: usize,
}

impl Report {
    /// Render the report as the single JSON document `--json` writes to stdout.
    ///
    /// # Panics
    ///
    /// Panics only if the report contains a value serde cannot serialize, which the
    /// types above make unreachable.
    #[must_use]
    pub fn to_json(&self) -> String {
        let document = JsonReport {
            findings: self
                .diagnostics
                .iter()
                .map(|finding| JsonFinding {
                    check: finding.rule,
                    level: finding.level,
                    message: &finding.message,
                })
                .collect(),
            error_count: self.error_count,
            warning_count: self.warning_count,
        };
        serde_json::to_string_pretty(&document).unwrap_or_else(|_| "{}".to_owned())
    }
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "tests use unwrap, indexing, and other patterns freely"
)]
mod tests {
    use super::{CheckName, CommandReport, Finding, Level, OutputLine, Report};
    use std::str::FromStr as _;

    fn warning() -> Finding {
        Finding::new(
            CheckName::MutableRef,
            Level::Warn,
            "actions/checkout is pinned to branch main",
        )
    }

    #[test]
    fn clean_report_renders_a_summary_only() {
        let lines = Report::default().render();
        assert_eq!(lines.len(), 1);
        assert!(matches!(&lines[0], OutputLine::Summary { text } if text == "No audit findings"));
    }

    #[test]
    fn findings_render_with_check_name_and_summary() {
        let report = Report::from_diagnostics(vec![warning()]);
        let lines = report.render();

        assert!(matches!(
            &lines[0],
            OutputLine::LintDiag { rule, level, .. }
                if rule == "mutable-ref" && *level == Level::Warn
        ));
        assert!(matches!(&lines[2], OutputLine::Summary { text } if text == "1 warning"));
    }

    #[test]
    fn warning_only_report_exits_zero() {
        let report = Report::from_diagnostics(vec![warning()]);
        assert_eq!(<Report as CommandReport>::exit_code(&report), 0);
    }

    #[test]
    fn error_report_exits_one() {
        let finding = Finding::new(CheckName::MutableRef, Level::Error, "boom");
        let report = Report::from_diagnostics(vec![finding]);
        assert_eq!(<Report as CommandReport>::exit_code(&report), 1);
    }

    #[test]
    fn json_carries_the_contracted_field_names() {
        let report = Report::from_diagnostics(vec![warning()]);
        let value: serde_json::Value = serde_json::from_str(&report.to_json()).unwrap();

        assert_eq!(value["findings"][0]["check"], "mutable-ref");
        assert_eq!(value["findings"][0]["level"], "warn");
        assert_eq!(
            value["findings"][0]["message"],
            "actions/checkout is pinned to branch main"
        );
        assert_eq!(value["error_count"], 0);
        assert_eq!(value["warning_count"], 1);
    }

    #[test]
    fn json_findings_is_an_empty_array_when_clean() {
        // Never null and never absent, so a consumer can index it unconditionally.
        let value: serde_json::Value = serde_json::from_str(&Report::default().to_json()).unwrap();
        assert_eq!(value["findings"], serde_json::json!([]));
        assert_eq!(value["error_count"], 0);
        assert_eq!(value["warning_count"], 0);
    }

    #[test]
    fn check_name_round_trips_through_its_literal_string() {
        // The name a user greps for, the name in JSON, and the name in the terminal are
        // one string. This asserts the literal so a rename cannot pass silently.
        assert_eq!(CheckName::MutableRef.as_str(), "mutable-ref");
        assert_eq!(CheckName::MutableRef.to_string(), "mutable-ref");
        assert_eq!(
            CheckName::from_str("mutable-ref"),
            Ok(CheckName::MutableRef)
        );
        CheckName::from_str("mutable_ref").unwrap_err();
    }

    #[test]
    fn every_check_is_reachable_by_name() {
        // Guards the `rule_ids!` contract for checks added later: whatever is in ALL must
        // parse back from its own string.
        for &check in CheckName::ALL {
            assert_eq!(CheckName::from_str(check.as_str()), Ok(check));
        }
    }
}
