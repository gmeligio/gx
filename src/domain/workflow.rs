use std::collections::HashMap;
use std::path::PathBuf;
use thiserror::Error;

use crate::domain::ActionId;

/// Errors that can occur when working with workflow files
#[derive(Debug, Error)]
pub enum WorkflowError {
    /// Failed to scan workflow files
    #[error("failed to scan workflows: {reason}")]
    ScanFailed { reason: String },

    /// Failed to parse a workflow file
    #[error("failed to parse workflow {path}: {reason}")]
    ParseFailed { path: String, reason: String },

    /// Failed to update a workflow file
    #[error("failed to update workflow {path}: {reason}")]
    UpdateFailed { path: String, reason: String },
}

/// Result of updating a single workflow file
pub struct UpdateResult {
    pub file: PathBuf,
    pub changes: Vec<String>,
}

/// Trait for scanning workflow files and extracting action references
pub trait WorkflowScanner {
    /// Scan all workflow files, yielding one `LocatedAction` per step.
    ///
    /// Each item is a `Result` — errors are per-file and do not abort the scan.
    /// The caller decides whether to collect, short-circuit, or continue past errors.
    fn scan(
        &self,
    ) -> Box<dyn Iterator<Item = Result<crate::domain::LocatedAction, WorkflowError>> + '_>;

    /// Enumerate all workflow file paths.
    ///
    /// Each item is a `Result` — errors are per-file.
    fn scan_paths(
        &self,
    ) -> Box<dyn Iterator<Item = Result<std::path::PathBuf, WorkflowError>> + '_>;

    /// Scan all workflow files and collect into a `Vec`. Fails on the first error.
    ///
    /// # Errors
    ///
    /// Returns an error if any workflow file cannot be read or parsed.
    fn scan_all_located(&self) -> Result<Vec<crate::domain::LocatedAction>, WorkflowError> {
        self.scan().collect()
    }

    /// Find all workflow file paths and collect into a `Vec`.
    ///
    /// # Errors
    ///
    /// Returns an error if the workflow directory cannot be read.
    fn find_workflow_paths(&self) -> Result<Vec<std::path::PathBuf>, WorkflowError> {
        self.scan_paths().collect()
    }
}

/// Trait for updating action references in workflow files
pub trait WorkflowUpdater {
    /// Update all workflow files, replacing action references according to the map.
    ///
    /// The map keys are action IDs; the values are the full replacement ref strings
    /// (e.g. `"abc123...def456 # v4"`).
    ///
    /// # Errors
    ///
    /// Returns an error if any workflow file cannot be read or written.
    fn update_all(
        &self,
        actions: &HashMap<ActionId, String>,
    ) -> Result<Vec<UpdateResult>, WorkflowError>;

    /// Update a single workflow file with its specific action map.
    ///
    /// # Errors
    ///
    /// Returns an error if the workflow file cannot be read or written.
    fn update_file(
        &self,
        workflow_path: &std::path::Path,
        actions: &HashMap<ActionId, String>,
    ) -> Result<UpdateResult, WorkflowError>;
}
