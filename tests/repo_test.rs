use std::env;
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

    // Initialize a git repo
    init_git_repo(root);

    // Create .github folder
    let github_dir = root.join(".github");
    fs::create_dir(&github_dir).unwrap();

    // Change to temp directory
    let original_dir = env::current_dir().unwrap();
    env::set_current_dir(root).unwrap();

    let result = gx::repo::find_root();

    // Restore original directory
    env::set_current_dir(original_dir).unwrap();

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), root);
}

#[test]
fn test_find_root_without_github_folder() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();

    // Initialize a git repo
    init_git_repo(root);

    // Don't create .github folder

    // Change to temp directory
    let original_dir = env::current_dir().unwrap();
    env::set_current_dir(root).unwrap();

    let result = gx::repo::find_root();

    // Restore original directory
    env::set_current_dir(original_dir).unwrap();

    assert!(result.is_err());
    let error = result.unwrap_err();
    assert!(
        error
            .downcast_ref::<gx::repo::GithubFolderNotFound>()
            .is_some()
    );
}
