pub mod github;
pub mod lock;
pub mod manifest;
pub mod repo;
pub mod workflow;

pub use github::GithubRegistry;
pub use lock::{FileLock, LOCK_FILE_NAME, LOCK_FILE_VERSION, LockFileError, LockStore, MemoryLock};
pub use manifest::{
    FileManifest, MANIFEST_FILE_NAME, ManifestError, ManifestStore, MemoryManifest,
};
pub use repo::{RepoError, find_root};
pub use workflow::{FileWorkflowScanner, FileWorkflowUpdater};
// WorkflowError and UpdateResult are now in domain; re-export from there for convenience
pub use crate::domain::{UpdateResult, WorkflowError};
