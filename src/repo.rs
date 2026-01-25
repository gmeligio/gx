use anyhow::{Result, anyhow};
use gix::discover;
use std::{env, path::PathBuf};

#[derive(Debug)]
pub struct DotGitHubFolderNotFound;

impl std::fmt::Display for DotGitHubFolderNotFound {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, ".github folder not found")
    }
}

impl std::error::Error for DotGitHubFolderNotFound {}

pub fn find_root() -> Result<PathBuf> {
    let cwd = env::current_dir()?;
    let repo = discover(cwd)?;
    let root = repo.workdir().unwrap();

    if root.join(".github").is_dir() {
        Ok(root.to_path_buf())
    } else {
        Err(anyhow!(DotGitHubFolderNotFound))
    }
}
