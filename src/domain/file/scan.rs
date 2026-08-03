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

/// Trait for scanning the files gx manages and extracting action references.
///
/// "Managed files" are workflows (`.github/workflows`) and composite action definitions
/// (`.github/actions/**/action.yml`). Both hold `uses:` references, at
/// `jobs.<id>.steps` and `runs.steps` respectively; a composite step carries no job.
pub trait Scanner {
    /// Scan all managed files, yielding one `LocatedAction` per step.
    ///
    /// Each item is a `Result` — errors are per-file and do not abort the scan.
    /// The caller decides whether to collect, short-circuit, or continue past errors.
    fn scan(
        &self,
    ) -> Box<dyn Iterator<Item = Result<crate::domain::file::actions::Located, Error>> + '_>;

    /// Enumerate all managed file paths.
    ///
    /// Each item is a `Result` — errors are per-file.
    fn scan_paths(&self) -> Box<dyn Iterator<Item = Result<std::path::PathBuf, Error>> + '_>;

    /// Scan all managed files and collect into a `Vec`. Fails on the first error.
    ///
    /// # Errors
    ///
    /// Returns an error if any managed file cannot be read or parsed.
    fn scan_all_located(&self) -> Result<Vec<crate::domain::file::actions::Located>, Error> {
        self.scan().collect()
    }

    /// Find all managed file paths and collect into a `Vec`.
    ///
    /// # Errors
    ///
    /// Returns an error if a discovery directory cannot be read.
    fn find_workflow_paths(&self) -> Result<Vec<std::path::PathBuf>, Error> {
        self.scan_paths().collect()
    }

    /// Parse every managed file once and return both the structural `Parsed` model
    /// and the existing `Located` action list. The lint command uses this to
    /// feed both action-hygiene rules and workflow-security rules from a single
    /// parse pass. Each `Parsed` carries the schema its file follows, so callers can
    /// scope schema-specific rules to workflows.
    ///
    /// # Errors
    ///
    /// Returns an error if any managed file cannot be read or parsed.
    fn scan_all_with_parsed(
        &self,
    ) -> Result<
        (
            Vec<crate::domain::file::actions::Located>,
            Vec<crate::domain::file::parsed::Parsed>,
        ),
        Error,
    >;
}
