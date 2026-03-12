mod convert;
pub mod patch;

use crate::config::Lint;
use crate::domain::Parsed;
use crate::domain::manifest::Manifest;
use crate::domain::plan::ManifestDiff;
use convert::{ManifestData, format_manifest_toml, manifest_from_data, manifest_to_data};
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

pub const MANIFEST_FILE_NAME: &str = "gx.toml";

/// Errors that can occur when working with manifest files
#[derive(Debug, Error)]
pub enum Error {
    #[error("failed to read manifest file: {}", path.display())]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to parse manifest file: {}", path.display())]
    Parse {
        path: PathBuf,
        #[source]
        source: Box<toml::de::Error>,
    },

    #[error("failed to write manifest file: {}", path.display())]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to serialize manifest to TOML")]
    Serialize(#[source] toml::ser::Error),

    #[error("invalid manifest: {0}")]
    Validation(String),

    #[error("gx.toml requires gx >= {required} (you have {current})")]
    VersionRequired { required: String, current: String },
}

// ---- Store ----

pub struct Store {
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

impl Store {
    /// Save the given `Manifest` to this file.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Write`] if the file cannot be written.
    /// Returns [`Error::Serialize`] if serialization fails.
    pub fn save(&self, manifest: &Manifest) -> Result<(), Error> {
        let data = manifest_to_data(manifest);
        let content = format_manifest_toml(&data);
        fs::write(&self.path, content).map_err(|source| Error::Write {
            path: self.path.clone(),
            source,
        })?;
        Ok(())
    }
}

/// Load a manifest from a file path. Returns `Parsed { value: Manifest::default(), migrated: false }` if the file does not exist.
///
/// # Errors
///
/// Returns [`Error::Read`] if the file cannot be read.
/// Returns [`Error::Parse`] if the TOML is invalid.
/// Returns [`Error::Validation`] if the manifest data is invalid.
/// Returns [`Error::VersionRequired`] if the file requires a newer version of gx.
pub fn parse(path: &Path) -> Result<Parsed<Manifest>, Error> {
    if !path.exists() {
        return Ok(Parsed {
            value: Manifest::default(),
            migrated: false,
        });
    }

    let content = fs::read_to_string(path).map_err(|source| Error::Read {
        path: path.to_path_buf(),
        source,
    })?;

    let data: ManifestData = toml::from_str(&content).map_err(|source| Error::Parse {
        path: path.to_path_buf(),
        source: Box::new(source),
    })?;

    // Detect v1 by absence of [gx] section
    let is_v1 = data.gx.is_none();
    let is_v2 = !is_v1;

    // Version guard: check min_version if [gx] section is present
    if let Some(ref gx) = data.gx
        && !gx.min_version.is_empty()
    {
        let zero = semver::Version::new(0, 0, 0);
        let current = semver::Version::parse(env!("CARGO_PKG_VERSION")).unwrap_or(zero.clone());
        let required = semver::Version::parse(&gx.min_version).unwrap_or(zero);
        if current < required {
            return Err(Error::VersionRequired {
                required: gx.min_version.clone(),
                current: env!("CARGO_PKG_VERSION").to_string(),
            });
        }
    }

    let manifest = manifest_from_data(data, path, is_v2)?;

    Ok(Parsed {
        value: manifest,
        migrated: is_v1,
    })
}

/// Load lint configuration from a manifest file. Returns `Lint::default()` if the file does not exist or has no `[lint]` section.
///
/// # Errors
///
/// Returns [`Error::Read`] if the file cannot be read.
/// Returns [`Error::Parse`] if the TOML is invalid.
pub fn parse_lint_config(path: &Path) -> Result<Lint, Error> {
    if !path.exists() {
        return Ok(Lint::default());
    }

    let content = fs::read_to_string(path).map_err(|source| Error::Read {
        path: path.to_path_buf(),
        source,
    })?;

    let data: ManifestData = toml::from_str(&content).map_err(|source| Error::Parse {
        path: path.to_path_buf(),
        source: Box::new(source),
    })?;

    Ok(Lint {
        rules: data.lint.rules,
    })
}

/// Create a new manifest file from a `ManifestDiff`.
///
/// This builds a fresh manifest from the `added` and `overrides_added` fields.
/// Used for the `init` command when no manifest file exists yet.
///
/// # Errors
///
/// Returns [`Error::Write`] if the file cannot be written.
pub fn create(path: &Path, diff: &ManifestDiff) -> Result<(), Error> {
    // Build domain Manifest from the diff
    let mut manifest = Manifest::default();
    for (id, version) in &diff.added {
        manifest.set(id.clone(), version.clone());
    }
    for (id, ovr) in &diff.overrides_added {
        manifest.add_override(id.clone(), ovr.clone());
    }

    // Reuse existing formatting
    let data = manifest_to_data(&manifest);
    let content = format_manifest_toml(&data);

    fs::write(path, content).map_err(|source| Error::Write {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(())
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
