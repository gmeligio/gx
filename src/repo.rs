use anyhow::Result;
use std::{env, path::PathBuf};
use thiserror::Error;

/// Errors that can occur when interacting with the local repository
#[derive(Debug, Error)]
pub enum RepoError {
    #[error(".github folder not found")]
    GithubFolder(),

    #[error("repository has no work tree")]
    BareRepository,

    #[error("current directory doesn't exist or there are insufficient permissions to access it")]
    CurrentDirectory(#[source] std::io::Error),

    #[error("no valid git repository could be found")]
    GitRepository(#[source] gix_discover::upwards::Error),
}

/// Find the root of the git repository containing the current directory.
///
/// # Errors
///
/// Returns an error if no git repository is found, the repository is bare, or the `.github` folder is missing.
pub fn find_root() -> Result<PathBuf, RepoError> {
    let cwd = env::current_dir().map_err(RepoError::CurrentDirectory)?;

    let (repo_path, _trust) = gix_discover::upwards(&cwd).map_err(RepoError::GitRepository)?;

    let (_git_dir, work_tree) = repo_path.into_repository_and_work_tree_directories();

    let root = work_tree.ok_or(RepoError::BareRepository)?;

    if root.join(".github").is_dir() {
        Ok(root)
    } else {
        Err(RepoError::GithubFolder())
    }
}
