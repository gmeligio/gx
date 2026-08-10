//! Aggregation over a set of diagnostics: severity counts, the exit code they imply,
//! and the pluralized summary line. None of this is specific to any one command.

use super::record::Diagnostic;
use crate::config::Level;

/// A set of diagnostics plus the severity counts derived from them.
///
/// `Id` is the reporting command's rule-identity type (e.g. `lint::RuleName`).
#[derive(Debug)]
pub struct Report<Id> {
    /// All diagnostics found.
    pub diagnostics: Vec<Diagnostic<Id>>,
    /// Number of error-level diagnostics.
    pub error_count: usize,
    /// Number of warning-level diagnostics.
    pub warning_count: usize,
}

impl<Id> Default for Report<Id> {
    fn default() -> Self {
        Self {
            diagnostics: Vec::new(),
            error_count: 0,
            warning_count: 0,
        }
    }
}

impl<Id> Report<Id> {
    /// Build a report from a list of diagnostics, counting severities.
    #[must_use]
    pub fn from_diagnostics(diagnostics: Vec<Diagnostic<Id>>) -> Self {
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

    /// Exit code implied by the diagnostics: non-zero when any error was reported.
    #[must_use]
    pub fn exit_code(&self) -> i32 {
        i32::from(self.error_count > 0)
    }

    /// The pluralized summary line, e.g. `2 errors · 1 warning`.
    ///
    /// `clean` is the text used when nothing was reported, which differs per command.
    #[must_use]
    pub fn summary(&self, clean: &str) -> String {
        match (self.error_count, self.warning_count) {
            (0, 0) => clean.to_owned(),
            (errs, 0) => format!("{errs} error{}", plural(errs)),
            (0, warns) => format!("{warns} warning{}", plural(warns)),
            (errs, warns) => format!(
                "{} error{} · {} warning{}",
                errs,
                plural(errs),
                warns,
                plural(warns)
            ),
        }
    }
}

/// The plural suffix for a count: empty for one, `s` otherwise.
const fn plural(count: usize) -> &'static str {
    if count == 1 { "" } else { "s" }
}
