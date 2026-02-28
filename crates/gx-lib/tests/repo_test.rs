use gx_lib::infrastructure::repo;
use std::fs;
use tempfile::TempDir;

fn init_git_repo(root: &std::path::Path) {
    // Create minimal git directory structure
    fs::create_dir(root.join(".git")).unwrap();
    fs::write(root.join(".git/HEAD"), "ref: refs/heads/main\n").unwrap();
    fs::create_dir_all(root.join(".git/objects")).unwrap();
    fs::create_dir_all(root.join(".git/refs")).unwrap();
}

#[test]
fn test_find_root_with_github_folder() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();

    init_git_repo(root);
    fs::create_dir(root.join(".github")).unwrap();

    let result = repo::find_root(root);

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), root);
}

#[test]
fn test_find_root_without_github_folder() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();

    init_git_repo(root);

    let result = repo::find_root(root);

    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), repo::RepoError::GithubFolder));
}
