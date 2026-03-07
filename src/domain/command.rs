use std::fmt::Debug;
use std::path::Path;

use crate::config::Config;
use crate::domain::AppError;
use crate::output::OutputLine;

/// Trait for report types returned by commands.
pub trait CommandReport: Debug + Default {
    /// Render the report into output lines.
    fn render(&self) -> Vec<OutputLine>;

    /// Exit code to use after rendering; defaults to `0`.
    fn exit_code(&self) -> i32 {
        0
    }
}

/// Trait for command types that can be run.
pub trait Command {
    type Report: CommandReport;

    /// Run the command and return a report.
    ///
    /// # Errors
    ///
    /// Returns [`AppError`] if the command fails.
    fn run(
        &self,
        repo_root: &Path,
        config: Config,
        on_progress: &mut dyn FnMut(&str),
    ) -> Result<Self::Report, AppError>;
}
