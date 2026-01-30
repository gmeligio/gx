use thiserror::Error;

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
