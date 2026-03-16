use std::path::PathBuf;
use thiserror::Error;

/// Errors that can occur when working with workflow files.
#[derive(Debug, Error)]
pub enum Error {
    /// Failed to scan workflow files.
    #[error("failed to scan workflows: {reason}")]
    ScanFailed { reason: String },

    /// Failed to parse a workflow file.
    #[error("failed to parse workflow {path}: {reason}")]
    ParseFailed { path: String, reason: String },

    /// Failed to update a workflow file.
    #[error("failed to update workflow {path}: {reason}")]
    UpdateFailed { path: String, reason: String },
}

/// Result of updating a single workflow file.
pub struct UpdateResult {
    pub file: PathBuf,
    pub changes: Vec<String>,
}

/// Trait for scanning workflow files and extracting action references.
pub trait Scanner {
    /// Scan all workflow files, yielding one `LocatedAction` per step.
    ///
    /// Each item is a `Result` — errors are per-file and do not abort the scan.
    /// The caller decides whether to collect, short-circuit, or continue past errors.
    fn scan(
        &self,
    ) -> Box<dyn Iterator<Item = Result<crate::domain::workflow_actions::Located, Error>> + '_>;

    /// Enumerate all workflow file paths.
    ///
    /// Each item is a `Result` — errors are per-file.
    fn scan_paths(&self) -> Box<dyn Iterator<Item = Result<std::path::PathBuf, Error>> + '_>;

    /// Scan all workflow files and collect into a `Vec`. Fails on the first error.
    ///
    /// # Errors
    ///
    /// Returns an error if any workflow file cannot be read or parsed.
    fn scan_all_located(&self) -> Result<Vec<crate::domain::workflow_actions::Located>, Error> {
        self.scan().collect()
    }

    /// Find all workflow file paths and collect into a `Vec`.
    ///
    /// # Errors
    ///
    /// Returns an error if the workflow directory cannot be read.
    fn find_workflow_paths(&self) -> Result<Vec<std::path::PathBuf>, Error> {
        self.scan_paths().collect()
    }
}
