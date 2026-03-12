use crate::config::Level;
use console::style;
use std::path::PathBuf;

/// Semantic output line variants produced by render functions.
/// Colors are applied at print boundary — no ANSI codes here.
#[derive(Debug, Clone, PartialEq)]
pub enum Line {
    /// An action was upgraded from one version to another.
    Upgraded {
        action: String,
        from: String,
        to: String,
    },
    /// An action was added.
    Added { action: String, version: String },
    /// An action was removed.
    Removed { action: String },
    /// An action was changed with a detail note.
    Changed { action: String, detail: String },
    /// An action was skipped with a reason.
    Skipped { action: String, reason: String },
    /// A warning message.
    Warning { message: String },
    /// A lint diagnostic.
    LintDiag {
        level: Level,
        workflow: Option<String>,
        rule: String,
        message: String,
    },
    /// A summary line (success/result).
    Summary { text: String },
    /// The log file path shown at end of output.
    LogPath { path: PathBuf },
    /// CI mode notice.
    CiNotice { message: String },
    /// A blank separator line.
    Blank,
}

impl Line {
    /// Format this line into a printable string, optionally with ANSI color.
    #[must_use]
    pub fn format_line(&self, use_color: bool) -> String {
        match self {
            Line::Upgraded { action, from, to } => {
                let arrow = if use_color {
                    style("↑").cyan().to_string()
                } else {
                    "↑".to_string()
                };
                format!(" {arrow} {action:<30} {from} → {to}")
            }
            Line::Added { action, version } => {
                let plus = if use_color {
                    style("+").green().to_string()
                } else {
                    "+".to_string()
                };
                format!(" {plus} {action:<30} {version}")
            }
            Line::Removed { action } => {
                let minus = if use_color {
                    style("−").red().to_string()
                } else {
                    "−".to_string()
                };
                format!(" {minus} {action}")
            }
            Line::Changed { action, detail } => {
                format!(" ~ {action:<30} {detail}")
            }
            Line::Skipped { action, reason } => {
                format!(" - {action:<30} ({reason})")
            }
            Line::Warning { message } => {
                let prefix = if use_color {
                    style("⚠").yellow().to_string()
                } else {
                    "⚠".to_string()
                };
                format!(" {prefix} {message}")
            }
            Line::LintDiag {
                level,
                workflow,
                rule,
                message,
            } => {
                let colored_symbol = match level {
                    Level::Error => {
                        if use_color {
                            style("✗").red().to_string()
                        } else {
                            "✗".to_string()
                        }
                    }
                    Level::Warn => {
                        if use_color {
                            style("⚠").yellow().to_string()
                        } else {
                            "⚠".to_string()
                        }
                    }
                    Level::Off => String::new(),
                };
                let location = workflow
                    .as_ref()
                    .map(|w| format!("{w}: "))
                    .unwrap_or_default();
                format!(" {colored_symbol} {location}{rule}: {message}")
            }
            Line::Summary { text } => {
                let check = if use_color {
                    style("✓").green().to_string()
                } else {
                    "✓".to_string()
                };
                format!("\n {check} {text}")
            }
            Line::LogPath { path } => {
                let icon = if use_color {
                    style("📋").to_string()
                } else {
                    "📋".to_string()
                };
                format!(" {icon} {}", path.display())
            }
            Line::CiNotice { message } => {
                let prefix = if use_color {
                    style("ℹ").blue().to_string()
                } else {
                    "ℹ".to_string()
                };
                format!(" {prefix} {message}")
            }
            Line::Blank => String::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Level, Line};
    use std::path::PathBuf;

    #[test]
    fn format_line_upgraded_no_color() {
        let line = Line::Upgraded {
            action: "actions/checkout".to_string(),
            from: "v3".to_string(),
            to: "v4".to_string(),
        };
        let result = line.format_line(false);
        assert!(result.contains("↑"));
        assert!(result.contains("actions/checkout"));
        assert!(result.contains("v3"));
        assert!(result.contains("v4"));
    }

    #[test]
    fn format_line_lint_diag_no_color() {
        let line = Line::LintDiag {
            level: Level::Error,
            workflow: Some("ci.yml".to_string()),
            rule: "pinned-version".to_string(),
            message: "action must be pinned".to_string(),
        };
        let result = line.format_line(false);
        assert!(result.contains("✗"));
        assert!(result.contains("ci.yml"));
        assert!(result.contains("pinned-version"));
        assert!(result.contains("action must be pinned"));
    }

    #[test]
    fn format_line_summary_no_color() {
        let line = Line::Summary {
            text: "All done".to_string(),
        };
        let result = line.format_line(false);
        assert!(result.contains("✓"));
        assert!(result.contains("All done"));
    }

    #[test]
    fn format_line_blank_no_color() {
        let line = Line::Blank;
        let result = line.format_line(false);
        assert_eq!(result, "");
    }

    #[test]
    fn format_line_added_no_color() {
        let line = Line::Added {
            action: "actions/setup-node".to_string(),
            version: "v4".to_string(),
        };
        let result = line.format_line(false);
        assert!(result.contains('+'));
        assert!(result.contains("actions/setup-node"));
        assert!(result.contains("v4"));
    }

    #[test]
    fn format_line_log_path_no_color() {
        let line = Line::LogPath {
            path: PathBuf::from("/tmp/gx/test.log"),
        };
        let result = line.format_line(false);
        assert!(result.contains("📋"));
        assert!(result.contains("test.log"));
    }
}
