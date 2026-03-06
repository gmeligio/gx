use super::lines::OutputLine;
use super::report::{InitReport, LintReport, TidyReport, UpgradeReport};

/// Render an `UpgradeReport` into a list of output lines.
#[must_use]
pub fn render_upgrade(report: &UpgradeReport) -> Vec<OutputLine> {
    if report.up_to_date {
        return vec![OutputLine::Summary {
            text: "All actions up to date".to_string(),
        }];
    }

    if report.upgrades.is_empty() && report.skipped.is_empty() && report.warnings.is_empty() {
        return vec![OutputLine::Summary {
            text: "All actions up to date".to_string(),
        }];
    }

    let mut lines = Vec::new();

    for (action, from, to) in &report.upgrades {
        lines.push(OutputLine::Upgraded {
            action: action.clone(),
            from: from.clone(),
            to: to.clone(),
        });
    }

    for (action, reason) in &report.skipped {
        lines.push(OutputLine::Skipped {
            action: action.clone(),
            reason: reason.clone(),
        });
    }

    for message in &report.warnings {
        lines.push(OutputLine::Warning {
            message: message.clone(),
        });
    }

    lines.push(OutputLine::Blank);

    let upgrade_count = report.upgrades.len();
    let wf = report.workflows_updated;
    let summary = format!(
        "{} upgraded · {} workflow{}",
        upgrade_count,
        wf,
        if wf == 1 { "" } else { "s" }
    );
    lines.push(OutputLine::Summary { text: summary });

    lines
}

/// Render a `TidyReport` into a list of output lines.
#[must_use]
pub fn render_tidy(report: &TidyReport) -> Vec<OutputLine> {
    let has_changes =
        !report.removed.is_empty() || !report.added.is_empty() || !report.upgraded.is_empty();

    if !has_changes {
        return vec![OutputLine::Summary {
            text: "Everything up to date".to_string(),
        }];
    }

    let mut lines = Vec::new();

    for action in &report.removed {
        lines.push(OutputLine::Removed {
            action: action.clone(),
        });
    }

    for (action, version) in &report.added {
        lines.push(OutputLine::Added {
            action: action.clone(),
            version: version.clone(),
        });
    }

    for (action, from, to) in &report.upgraded {
        lines.push(OutputLine::Upgraded {
            action: action.clone(),
            from: from.clone(),
            to: to.clone(),
        });
    }

    lines.push(OutputLine::Blank);

    let mut parts = Vec::new();
    if !report.removed.is_empty() {
        parts.push(format!("{} removed", report.removed.len()));
    }
    if !report.added.is_empty() {
        parts.push(format!("{} added", report.added.len()));
    }
    if !report.upgraded.is_empty() {
        parts.push(format!("{} upgraded", report.upgraded.len()));
    }
    let wf = report.workflows_updated;
    parts.push(format!("{} workflow{}", wf, if wf == 1 { "" } else { "s" }));

    lines.push(OutputLine::Summary {
        text: parts.join(" · "),
    });

    lines
}

/// Render a `LintReport` into a list of output lines.
#[must_use]
pub fn render_lint(report: &LintReport) -> Vec<OutputLine> {
    if report.diagnostics.is_empty() {
        return vec![OutputLine::Summary {
            text: "No lint issues found".to_string(),
        }];
    }

    let mut lines = Vec::new();

    for diag in &report.diagnostics {
        lines.push(OutputLine::LintDiag {
            level: diag.level,
            workflow: diag.workflow.clone(),
            rule: diag.rule.clone(),
            message: diag.message.clone(),
        });
    }

    lines.push(OutputLine::Blank);

    let e = report.error_count;
    let w = report.warning_count;
    let summary = match (e, w) {
        (0, 0) => "No lint issues found".to_string(),
        (e, 0) => format!("{e} error{}", if e == 1 { "" } else { "s" }),
        (0, w) => format!("{w} warning{}", if w == 1 { "" } else { "s" }),
        (e, w) => format!(
            "{} error{} · {} warning{}",
            e,
            if e == 1 { "" } else { "s" },
            w,
            if w == 1 { "" } else { "s" }
        ),
    };
    lines.push(OutputLine::Summary { text: summary });

    lines
}

/// Render an `InitReport` into a list of output lines.
#[must_use]
pub fn render_init(report: &InitReport) -> Vec<OutputLine> {
    if !report.created {
        return vec![OutputLine::Summary {
            text: "No actions found in workflows".to_string(),
        }];
    }

    let mut lines = Vec::new();
    lines.push(OutputLine::Blank);

    let n = report.actions_discovered;
    lines.push(OutputLine::Summary {
        text: format!(
            "{} action{} discovered · manifest created",
            n,
            if n == 1 { "" } else { "s" }
        ),
    });

    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::lint::Diagnostic;
    use crate::config::Level;
    use crate::output::report::{InitReport, LintReport, TidyReport, UpgradeReport};

    #[test]
    fn render_upgrade_up_to_date() {
        let report = UpgradeReport {
            up_to_date: true,
            ..Default::default()
        };
        let lines = render_upgrade(&report);
        assert_eq!(lines.len(), 1);
        assert!(
            matches!(&lines[0], OutputLine::Summary { text } if text == "All actions up to date")
        );
    }

    #[test]
    fn render_upgrade_with_upgrades() {
        let report = UpgradeReport {
            upgrades: vec![
                (
                    "actions/checkout".to_string(),
                    "v6".to_string(),
                    "v6.0.2".to_string(),
                ),
                (
                    "jdx/mise-action".to_string(),
                    "v3".to_string(),
                    "v3.6.2".to_string(),
                ),
            ],
            workflows_updated: 1,
            ..Default::default()
        };
        let lines = render_upgrade(&report);

        assert!(lines.contains(&OutputLine::Upgraded {
            action: "actions/checkout".to_string(),
            from: "v6".to_string(),
            to: "v6.0.2".to_string(),
        }));
        assert!(lines.contains(&OutputLine::Upgraded {
            action: "jdx/mise-action".to_string(),
            from: "v3".to_string(),
            to: "v3.6.2".to_string(),
        }));
        assert!(lines.contains(&OutputLine::Summary {
            text: "2 upgraded · 1 workflow".to_string(),
        }));
    }

    #[test]
    fn render_tidy_nothing_changed() {
        let report = TidyReport::default();
        let lines = render_tidy(&report);
        assert_eq!(lines.len(), 1);
        assert!(
            matches!(&lines[0], OutputLine::Summary { text } if text == "Everything up to date")
        );
    }

    #[test]
    fn render_tidy_with_changes() {
        let report = TidyReport {
            removed: vec!["actions/unused".to_string()],
            added: vec![
                ("actions/new".to_string(), "v2".to_string()),
                ("actions/other".to_string(), "v1".to_string()),
            ],
            upgraded: vec![(
                "actions/checkout".to_string(),
                "sha".to_string(),
                "v6.0.2".to_string(),
            )],
            workflows_updated: 2,
            corrections: 0,
        };
        let lines = render_tidy(&report);

        assert!(lines.contains(&OutputLine::Removed {
            action: "actions/unused".to_string(),
        }));
        assert!(lines.contains(&OutputLine::Added {
            action: "actions/new".to_string(),
            version: "v2".to_string(),
        }));
        assert!(lines.contains(&OutputLine::Summary {
            text: "1 removed · 2 added · 1 upgraded · 2 workflows".to_string(),
        }));
    }

    #[test]
    fn render_lint_clean() {
        let report = LintReport::default();
        let lines = render_lint(&report);
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
        let report = LintReport::from_diagnostics(diagnostics);
        let lines = render_lint(&report);

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
            text: "1 error · 1 warning".to_string(),
        }));
    }

    #[test]
    fn render_init_no_actions() {
        let report = InitReport {
            actions_discovered: 0,
            created: false,
        };
        let lines = render_init(&report);
        assert_eq!(lines.len(), 1);
    }

    #[test]
    fn render_init_with_actions() {
        let report = InitReport {
            actions_discovered: 2,
            created: true,
        };
        let lines = render_init(&report);
        assert!(lines.contains(&OutputLine::Summary {
            text: "2 actions discovered · manifest created".to_string(),
        }));
    }
}
