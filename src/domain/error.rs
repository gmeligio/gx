use thiserror::Error;

use crate::domain::WorkflowError;
use crate::infra::{GithubError, LockFileError, ManifestError};

use crate::lint::LintError;
use crate::tidy::TidyError;
use crate::upgrade::UpgradeError;

/// Errors that can occur during command orchestration
#[derive(Debug, Error)]
pub enum AppError {
    /// The manifest file already exists when running init.
    #[error("already initialized \u{2014} use `gx tidy` to update")]
    AlreadyInitialized,

    /// The manifest store encountered an error.
    #[error(transparent)]
    Manifest(#[from] ManifestError),

    /// The lock store encountered an error.
    #[error(transparent)]
    Lock(#[from] LockFileError),

    /// Workflow scanning or updating failed.
    #[error(transparent)]
    Workflow(#[from] WorkflowError),

    /// The GitHub registry could not be initialized.
    #[error(transparent)]
    Github(#[from] GithubError),

    /// The tidy command failed.
    #[error(transparent)]
    Tidy(#[from] TidyError),

    /// The upgrade command failed.
    #[error(transparent)]
    Upgrade(#[from] UpgradeError),

    /// The lint command failed.
    #[error(transparent)]
    Lint(#[from] LintError),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_error_already_initialized_message() {
        let err = AppError::AlreadyInitialized;
        assert_eq!(
            err.to_string(),
            "already initialized \u{2014} use `gx tidy` to update"
        );
    }
}
