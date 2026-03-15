use crate::command::CommandReport;
use crate::output::lines::Line as OutputLine;

/// Report from the init command.
#[derive(Debug, Default)]
pub struct Report {
    /// Number of actions discovered from workflows.
    pub actions_discovered: usize,
    /// True if manifest and lock files were created.
    pub created: bool,
}

impl CommandReport for Report {
    fn render(&self) -> Vec<OutputLine> {
        if !self.created {
            return vec![OutputLine::Summary {
                text: "No actions found in workflows".to_owned(),
            }];
        }

        let mut lines = Vec::new();
        lines.push(OutputLine::Blank);

        let n = self.actions_discovered;
        lines.push(OutputLine::Summary {
            text: format!(
                "{} action{} discovered · manifest created",
                n,
                if n == 1 { "" } else { "s" }
            ),
        });

        lines
    }
}

#[cfg(test)]
mod tests {
    use super::{CommandReport as _, OutputLine, Report};

    #[test]
    fn render_init_no_actions() {
        let report = Report {
            actions_discovered: 0,
            created: false,
        };
        let lines = report.render();
        assert_eq!(lines.len(), 1);
    }

    #[test]
    fn render_init_with_actions() {
        let report = Report {
            actions_discovered: 2,
            created: true,
        };
        let lines = report.render();
        assert!(lines.contains(&OutputLine::Summary {
            text: "2 actions discovered · manifest created".to_owned(),
        }));
    }
}
