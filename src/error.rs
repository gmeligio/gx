use std::error::Error;
use std::fmt::{Display, Formatter, Result};
use std::path::PathBuf;

/// Error when a file path has not been initialized
#[derive(Debug)]
pub struct PathNotInitialized {
    pub file_type: &'static str,
}

impl PathNotInitialized {
    #[must_use]
    pub fn manifest() -> Self {
        Self {
            file_type: "Manifest",
        }
    }

    #[must_use]
    pub fn lock_file() -> Self {
        Self {
            file_type: "LockFile",
        }
    }
}

impl Display for PathNotInitialized {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        write!(
            f,
            "{} path not initialized. Use load_from_repo or load to create a {} with a path.",
            self.file_type,
            self.file_type.to_lowercase()
        )
    }
}

impl Error for PathNotInitialized {}

/// Error when .github folder is not found in the repository
#[derive(Debug)]
pub struct GithubFolderNotFound;

impl Display for GithubFolderNotFound {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        write!(f, ".github folder not found")
    }
}

impl Error for GithubFolderNotFound {}

/// Error when `GITHUB_TOKEN` is required but not set
#[derive(Debug)]
pub struct GitHubTokenRequired;

impl Display for GitHubTokenRequired {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        write!(
            f,
            "GITHUB_TOKEN environment variable is required for this operation.\n\
             Set it with: export GITHUB_TOKEN=<your-token>\n\
             Create a token at: https://github.com/settings/tokens"
        )
    }
}

impl Error for GitHubTokenRequired {}

/// Error when reading a file fails
#[derive(Debug)]
pub struct FileReadError {
    pub path: PathBuf,
    pub source: std::io::Error,
}

impl Display for FileReadError {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        write!(f, "Failed to read file: {}", self.path.display())
    }
}

impl Error for FileReadError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.source)
    }
}

/// Error when writing a file fails
#[derive(Debug)]
pub struct FileWriteError {
    pub path: PathBuf,
    pub source: std::io::Error,
}

impl Display for FileWriteError {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        write!(f, "Failed to write file: {}", self.path.display())
    }
}

impl Error for FileWriteError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.source)
    }
}

/// Error when parsing a TOML file fails
#[derive(Debug)]
pub struct TomlParseError {
    pub path: PathBuf,
    pub source: toml::de::Error,
}

impl Display for TomlParseError {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        write!(f, "Failed to parse TOML file: {}", self.path.display())
    }
}

impl Error for TomlParseError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.source)
    }
}
