use std::fmt;
use std::path::PathBuf;

/// Error when the manifest file path has not been initialized
#[derive(Debug)]
pub struct ManifestPathNotInitialized;

impl fmt::Display for ManifestPathNotInitialized {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Manifest path not initialized. Use load_from_repo or load to create a manifest with a path."
        )
    }
}

impl std::error::Error for ManifestPathNotInitialized {}

/// Error when the lock file path has not been initialized
#[derive(Debug)]
pub struct LockFilePathNotInitialized;

impl fmt::Display for LockFilePathNotInitialized {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "LockFile path not initialized. Use load_from_repo or load to create a lock file with a path."
        )
    }
}

impl std::error::Error for LockFilePathNotInitialized {}

/// Error when .github folder is not found in the repository
#[derive(Debug)]
pub struct GithubFolderNotFound;

impl fmt::Display for GithubFolderNotFound {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, ".github folder not found")
    }
}

impl std::error::Error for GithubFolderNotFound {}

/// Error when GITHUB_TOKEN is required but not set
#[derive(Debug)]
pub struct GitHubTokenRequired;

impl fmt::Display for GitHubTokenRequired {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "GITHUB_TOKEN environment variable is required for this operation.\n\
             Set it with: export GITHUB_TOKEN=<your-token>\n\
             Create a token at: https://github.com/settings/tokens"
        )
    }
}

impl std::error::Error for GitHubTokenRequired {}

/// Error when reading a file fails
#[derive(Debug)]
pub struct FileReadError {
    pub path: PathBuf,
    pub source: std::io::Error,
}

impl fmt::Display for FileReadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Failed to read file: {}", self.path.display())
    }
}

impl std::error::Error for FileReadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.source)
    }
}

/// Error when writing a file fails
#[derive(Debug)]
pub struct FileWriteError {
    pub path: PathBuf,
    pub source: std::io::Error,
}

impl fmt::Display for FileWriteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Failed to write file: {}", self.path.display())
    }
}

impl std::error::Error for FileWriteError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.source)
    }
}

/// Error when parsing a TOML file fails
#[derive(Debug)]
pub struct TomlParseError {
    pub path: PathBuf,
    pub source: toml::de::Error,
}

impl fmt::Display for TomlParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Failed to parse TOML file: {}", self.path.display())
    }
}

impl std::error::Error for TomlParseError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.source)
    }
}
