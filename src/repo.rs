use anyhow::{Context, Result, anyhow};
use std::{env, path::PathBuf};

use crate::error::GithubFolderNotFound;

pub fn find_root() -> Result<PathBuf> {
    let cwd = env::current_dir()?;

    let (repo_path, _trust) =
        gix_discover::upwards(&cwd).context("Failed to discover git repository")?;

    let (_git_dir, work_tree) = repo_path.into_repository_and_work_tree_directories();

    let root = work_tree.ok_or_else(|| anyhow!("Repository has no work tree (bare repository)"))?;

    if root.join(".github").is_dir() {
        Ok(root)
    } else {
        Err(anyhow!(GithubFolderNotFound))
    }
}
