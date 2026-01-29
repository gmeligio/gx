use std::fmt;
use std::path::PathBuf;

/// Error when a file path has not been initialized
#[derive(Debug)]
pub struct PathNotInitialized {
    pub file_type: &'static str,
}

impl PathNotInitialized {
    pub fn manifest() -> Self {
        Self {
            file_type: "Manifest",
        }
    }

    pub fn lock_file() -> Self {
        Self {
            file_type: "LockFile",
        }
    }
}

impl fmt::Display for PathNotInitialized {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} path not initialized. Use load_from_repo or load to create a {} with a path.",
            self.file_type,
            self.file_type.to_lowercase()
        )
    }
}

impl std::error::Error for PathNotInitialized {}

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
