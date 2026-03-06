use crate::commands::lint::Diagnostic;

/// Report from the upgrade command.
#[derive(Debug, Default)]
pub struct UpgradeReport {
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

/// Report from the tidy command.
#[derive(Debug, Default)]
pub struct TidyReport {
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

/// Report from the lint command.
#[derive(Debug, Default)]
pub struct LintReport {
    /// All diagnostics found
    pub diagnostics: Vec<Diagnostic>,
    /// Number of error-level diagnostics
    pub error_count: usize,
    /// Number of warning-level diagnostics
    pub warning_count: usize,
}

impl LintReport {
    /// Build a `LintReport` from a list of diagnostics.
    #[must_use]
    pub fn from_diagnostics(diagnostics: Vec<Diagnostic>) -> Self {
        use crate::config::Level;
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

/// Report from the init command.
#[derive(Debug, Default)]
pub struct InitReport {
    /// Number of actions discovered from workflows
    pub actions_discovered: usize,
    /// True if manifest and lock files were created
    pub created: bool,
}
