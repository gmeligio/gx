use crate::config::Config;
use crate::output::lines::Line as OutputLine;
use std::fmt::Debug;
use std::path::Path;

/// Trait for report types returned by commands.
#[allow(clippy::module_name_repetitions)]
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
    type Error: std::error::Error;

    /// Run the command and return a report.
    ///
    /// # Errors
    ///
    /// Returns `Self::Error` if the command fails.
    fn run(
        &self,
        repo_root: &Path,
        config: Config,
        on_progress: &mut dyn FnMut(&str),
    ) -> Result<Self::Report, Self::Error>;
}
