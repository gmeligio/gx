pub mod github;
pub mod lock;
pub mod manifest;
pub mod repo;
pub mod workflow;

pub use github::{GithubError, GithubRegistry};
pub use lock::{FileLock, LOCK_FILE_NAME, LOCK_FILE_VERSION, LockFileError, parse_lock};
pub use manifest::{
    FileManifest, MANIFEST_FILE_NAME, ManifestError, parse_lint_config, parse_manifest,
};
pub use repo::{RepoError, find_root};
pub use workflow::{FileWorkflowScanner, FileWorkflowUpdater};
// WorkflowError and UpdateResult are now in domain; re-export from there for convenience
pub use crate::domain::{UpdateResult, WorkflowError};
