use anyhow::Result;
use std::path::PathBuf;
use thiserror::Error;

/// Errors that can occur when interacting with the local repository
#[derive(Debug, Error)]
pub enum RepoError {
    #[error(".github folder not found")]
    GithubFolder,

    #[error("repository has no work tree")]
    BareRepository,

    #[error("no valid git repository could be found")]
    GitRepository(#[source] gix_discover::upwards::Error),
}

/// Find the root of the git repository containing the given path.
///
/// # Errors
///
/// Returns an error if no git repository is found, the repository is bare, or the `.github` folder is missing.
pub fn find_root(start: &std::path::Path) -> Result<PathBuf, RepoError> {
    let (repo_path, _trust) = gix_discover::upwards(start).map_err(RepoError::GitRepository)?;

    let (_git_dir, work_tree) = repo_path.into_repository_and_work_tree_directories();

    let root = work_tree.ok_or(RepoError::BareRepository)?;

    if root.join(".github").is_dir() {
        Ok(root)
    } else {
        Err(RepoError::GithubFolder)
    }
}
