use thiserror::Error;

/// Error when a file path has not been initialized
#[derive(Debug, Error)]
#[error("{file_type} path not initialized. Use load_from_repo or load to create a {} with a path.", file_type.to_lowercase())]
pub struct PathNotInitialized {
    pub file_type: &'static str,
}

impl PathNotInitialized {
    #[must_use]
    pub fn manifest() -> Self {
        Self {
            file_type: "Manifest",
        }
    }

    #[must_use]
    pub fn lock_file() -> Self {
        Self {
            file_type: "LockFile",
        }
    }
}

/// Error when .github folder is not found in the repository
#[derive(Debug, Error)]
#[error(".github folder not found")]
pub struct GithubFolderNotFound;

/// Error when `GITHUB_TOKEN` is required but not set
#[derive(Debug, Error)]
#[error(
    "GITHUB_TOKEN environment variable is required for this operation.\n\
     Set it with: export GITHUB_TOKEN=<your-token>\n\
     Create a token at: https://github.com/settings/tokens"
)]
pub struct GitHubTokenRequired;
