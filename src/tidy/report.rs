use crate::command::CommandReport;
use crate::domain::action::identity::ActionId;
use crate::domain::action::specifier::Specifier;
use crate::output::lines::Line as OutputLine;

/// Report from the tidy command.
#[derive(Debug, Default)]
pub struct Report {
    /// Actions removed.
    pub removed: Vec<ActionId>,
    /// Actions added: (action, version).
    pub added: Vec<(ActionId, Specifier)>,
    /// Actions upgraded (sha→tag or version bump): (action, from, to).
    pub upgraded: Vec<(ActionId, String, Specifier)>,
    /// Number of workflow files updated.
    pub workflows_updated: usize,
}

impl CommandReport for Report {
    fn render(&self) -> Vec<OutputLine> {
        let has_changes =
            !self.removed.is_empty() || !self.added.is_empty() || !self.upgraded.is_empty();

        if !has_changes {
            return vec![OutputLine::Summary {
                text: "Everything up to date".to_owned(),
            }];
        }

        let mut lines = Vec::new();

        for action in &self.removed {
            lines.push(OutputLine::Removed {
                action: action.to_string(),
            });
        }

        for (action, version) in &self.added {
            lines.push(OutputLine::Added {
                action: action.to_string(),
                version: version.to_string(),
            });
        }

        for (action, from, to) in &self.upgraded {
            lines.push(OutputLine::Upgraded {
                action: action.to_string(),
                from: from.clone(),
                to: to.to_string(),
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
#[expect(
    clippy::indexing_slicing,
    reason = "tests use unwrap, indexing, and other patterns freely"
)]
mod tests {
    use super::{ActionId, CommandReport as _, OutputLine, Report, Specifier};

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
            removed: vec![ActionId::from("actions/unused")],
            added: vec![
                (ActionId::from("actions/new"), Specifier::from_v1("v2")),
                (ActionId::from("actions/other"), Specifier::from_v1("v1")),
            ],
            upgraded: vec![(
                ActionId::from("actions/checkout"),
                "sha".to_owned(),
                Specifier::from_v1("v6.0.2"),
            )],
            workflows_updated: 2,
        };
        let lines = report.render();

        assert!(lines.contains(&OutputLine::Removed {
            action: "actions/unused".to_owned(),
        }));
        assert!(lines.contains(&OutputLine::Added {
            action: "actions/new".to_owned(),
            version: "^2".to_owned(),
        }));
        assert!(lines.contains(&OutputLine::Summary {
            text: "1 removed · 2 added · 1 upgraded · 2 workflows".to_owned(),
        }));
    }
}
