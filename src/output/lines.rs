use std::path::PathBuf;

use crate::config::Level;

/// Semantic output line variants produced by render functions.
/// Colors are applied at print boundary — no ANSI codes here.
#[derive(Debug, Clone, PartialEq)]
pub enum OutputLine {
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
