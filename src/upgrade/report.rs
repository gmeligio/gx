use crate::command::CommandReport;
use crate::output::lines::Line as OutputLine;

/// Report from the upgrade command.
#[derive(Debug, Default)]
pub struct Report {
    /// Actions that were upgraded: (action, `from_version`, `to_version`)
    pub upgrades: Vec<(String, String, String)>,
    /// Actions that were skipped: (action, reason)
    pub skipped: Vec<(String, String)>,
    /// Warnings encountered during upgrade
    pub warnings: Vec<String>,
    /// Number of workflow files updated
    pub workflows_updated: usize,
    /// True if everything was already up to date
    pub up_to_date: bool,
}

impl CommandReport for Report {
    fn render(&self) -> Vec<OutputLine> {
        if self.up_to_date {
            return vec![OutputLine::Summary {
                text: "All actions up to date".to_string(),
            }];
        }

        if self.upgrades.is_empty() && self.skipped.is_empty() && self.warnings.is_empty() {
            return vec![OutputLine::Summary {
                text: "All actions up to date".to_string(),
            }];
        }

        let mut lines = Vec::new();

        for (action, from, to) in &self.upgrades {
            lines.push(OutputLine::Upgraded {
                action: action.clone(),
                from: from.clone(),
                to: to.clone(),
            });
        }

        for (action, reason) in &self.skipped {
            lines.push(OutputLine::Skipped {
                action: action.clone(),
                reason: reason.clone(),
            });
        }

        for message in &self.warnings {
            lines.push(OutputLine::Warning {
                message: message.clone(),
            });
        }

        lines.push(OutputLine::Blank);

        let upgrade_count = self.upgrades.len();
        let wf = self.workflows_updated;
        let summary = format!(
            "{} upgraded · {} workflow{}",
            upgrade_count,
            wf,
            if wf == 1 { "" } else { "s" }
        );
        lines.push(OutputLine::Summary { text: summary });

        lines
    }
}

#[cfg(test)]
mod tests {
    use super::{CommandReport, OutputLine, Report};

    #[test]
    fn render_upgrade_up_to_date() {
        let report = Report {
            up_to_date: true,
            ..Default::default()
        };
        let lines = report.render();
        assert_eq!(lines.len(), 1);
        assert!(
            matches!(&lines[0], OutputLine::Summary { text } if text == "All actions up to date")
        );
    }

    #[test]
    fn render_upgrade_with_upgrades() {
        let report = Report {
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
        let lines = report.render();

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
}
