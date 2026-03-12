use crate::command::CommandReport;
use crate::output::lines::Line as OutputLine;

/// Report from the tidy command.
#[derive(Debug, Default)]
pub struct Report {
    /// Actions removed: action names
    pub removed: Vec<String>,
    /// Actions added: (action, version)
    pub added: Vec<(String, String)>,
    /// Actions upgraded (sha→tag or version bump): (action, from, to)
    pub upgraded: Vec<(String, String, String)>,
    /// Version corrections applied
    pub corrections: usize,
    /// Number of workflow files updated
    pub workflows_updated: usize,
}

impl CommandReport for Report {
    fn render(&self) -> Vec<OutputLine> {
        let has_changes =
            !self.removed.is_empty() || !self.added.is_empty() || !self.upgraded.is_empty();

        if !has_changes {
            return vec![OutputLine::Summary {
                text: "Everything up to date".to_string(),
            }];
        }

        let mut lines = Vec::new();

        for action in &self.removed {
            lines.push(OutputLine::Removed {
                action: action.clone(),
            });
        }

        for (action, version) in &self.added {
            lines.push(OutputLine::Added {
                action: action.clone(),
                version: version.clone(),
            });
        }

        for (action, from, to) in &self.upgraded {
            lines.push(OutputLine::Upgraded {
                action: action.clone(),
                from: from.clone(),
                to: to.clone(),
            });
        }

        lines.push(OutputLine::Blank);

        let mut parts = Vec::new();
        if !self.removed.is_empty() {
            parts.push(format!("{} removed", self.removed.len()));
        }
        if !self.added.is_empty() {
            parts.push(format!("{} added", self.added.len()));
        }
        if !self.upgraded.is_empty() {
            parts.push(format!("{} upgraded", self.upgraded.len()));
        }
        let wf = self.workflows_updated;
        parts.push(format!("{} workflow{}", wf, if wf == 1 { "" } else { "s" }));

        lines.push(OutputLine::Summary {
            text: parts.join(" · "),
        });

        lines
    }
}

#[cfg(test)]
mod tests {
    use super::{CommandReport, OutputLine, Report};

    #[test]
    fn render_tidy_nothing_changed() {
        let report = Report::default();
        let lines = report.render();
        assert_eq!(lines.len(), 1);
        assert!(
            matches!(&lines[0], OutputLine::Summary { text } if text == "Everything up to date")
        );
    }

    #[test]
    fn render_tidy_with_changes() {
        let report = Report {
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
        let lines = report.render();

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
}
