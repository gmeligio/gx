use std::collections::HashMap;
use std::path::PathBuf;
use thiserror::Error;

use crate::domain::ActionId;

/// Errors that can occur when working with workflow files
#[derive(Debug, Error)]
pub enum WorkflowError {
    #[error("failed to read glob pattern")]
    Glob(#[from] glob::PatternError),

    #[error("failed to read workflow: {}", path.display())]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to parse YAML in workflow: {}", path.display())]
    Parse {
        path: PathBuf,
        #[source]
        source: Box<serde_saphyr::Error>,
    },

    #[error("failed to write workflow: {}", path.display())]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("invalid regex pattern")]
    Regex(#[from] regex::Error),
}

/// Result of updating a single workflow file
pub struct UpdateResult {
    pub file: PathBuf,
    pub changes: Vec<String>,
}

/// Trait for scanning workflow files and extracting action references
pub trait WorkflowScanner {
    /// Scan all workflow files and return the aggregated set of action references.
    ///
    /// # Errors
    ///
    /// Returns an error if any workflow file cannot be read or parsed.
    fn scan_all(&self) -> Result<crate::domain::WorkflowActionSet, WorkflowError>;
}

/// Trait for scanning workflow files with full location context per step.
pub trait WorkflowScannerLocated {
    /// Scan all workflow files and return one `LocatedAction` per step.
    ///
    /// # Errors
    ///
    /// Returns an error if any workflow file cannot be read or parsed.
    fn scan_all_located(&self) -> Result<Vec<crate::domain::LocatedAction>, WorkflowError>;

    /// Find all workflow file paths.
    ///
    /// # Errors
    ///
    /// Returns an error if the workflow directory cannot be read.
    fn find_workflow_paths(&self) -> Result<Vec<std::path::PathBuf>, WorkflowError>;
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
