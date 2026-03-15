use crate::domain::lock::Lock;
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

/// Current two-tier format: read + write.
mod format;
/// Legacy flat format reader.
mod migration;

pub const LOCK_FILE_NAME: &str = "gx.lock";

/// Errors that can occur when working with lock files.
#[derive(Debug, Error)]
pub enum Error {
    #[error("failed to read lock file: {}", path.display())]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to parse lock file: {}", path.display())]
    Parse {
        path: PathBuf,
        #[source]
        source: Box<toml::de::Error>,
    },

    #[error("failed to write lock file: {}", path.display())]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("unrecognized lock file format: {}", path.display())]
    UnrecognizedFormat { path: PathBuf },
}

/// File-backed lock store. Reads from and writes to `.github/gx.lock`.
pub struct Store {
    /// Path to the lock file on disk.
    path: PathBuf,
}

impl Store {
    #[must_use]
    pub fn new(path: &Path) -> Self {
        Self {
            path: path.to_path_buf(),
        }
    }
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "constructor and methods are in separate impl blocks for readability"
)]
impl Store {
    /// Load a `Lock` from this file.
    ///
    /// Returns `Lock::default()` if the file does not exist or is empty.
    /// Tries the current two-tier format first, then the legacy flat format.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Read`] if the file cannot be read.
    /// Returns [`Error::Parse`] if the TOML is invalid.
    /// Returns [`Error::UnrecognizedFormat`] if the content is not a recognized lock format.
    pub fn load(&self) -> Result<Lock, Error> {
        if !self.path.exists() {
            return Ok(Lock::default());
        }

        let content = fs::read_to_string(&self.path).map_err(|source| Error::Read {
            path: self.path.clone(),
            source,
        })?;

        if content.trim().is_empty() {
            return Ok(Lock::default());
        }

        // Try current two-tier format first
        if let Some(lock) = format::try_parse(&content, &self.path)? {
            return Ok(lock);
        }

        // Try legacy flat format
        if let Some(lock) = migration::try_parse(&content, &self.path)? {
            return Ok(lock);
        }

        Err(Error::UnrecognizedFormat {
            path: self.path.clone(),
        })
    }

    /// Save the given `Lock` to this file using the current two-tier format.
    ///
    /// Always writes the full lock (no diff-based patching).
    ///
    /// # Errors
    ///
    /// Returns [`Error::Write`] if the file cannot be written.
    pub fn save(&self, lock: &Lock) -> Result<(), Error> {
        let output = format::write(lock);
        fs::write(&self.path, output).map_err(|source| Error::Write {
            path: self.path.clone(),
            source,
        })?;
        Ok(())
    }
}

/// Deserialize TOML content into the requested type, mapping errors to [`Error::Parse`].
fn parse_toml<T: for<'de> Deserialize<'de>>(content: &str, path: &Path) -> Result<T, Error> {
    toml::from_str(content).map_err(|source| Error::Parse {
        path: path.to_path_buf(),
        source: Box::new(source),
    })
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "tests use unwrap, indexing, and other patterns freely"
)]
#[path = "tests.rs"]
mod tests;
