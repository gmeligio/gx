mod convert;
pub mod patch;

use crate::config::Lint;
use crate::domain::Parsed;
use crate::domain::manifest::Manifest;
use crate::domain::plan::ManifestDiff;
use convert::{ManifestData, build_manifest_document, manifest_from_data};
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
    pub fn save(&self, manifest: &Manifest) -> Result<(), Error> {
        let doc = build_manifest_document(manifest);
        fs::write(&self.path, doc.to_string()).map_err(|source| Error::Write {
            path: self.path.clone(),
            source,
        })?;
        Ok(())
    }
}

/// Load a manifest from a file path. Returns `Parsed { value: Manifest::default(), migrated: false }` if the file does not exist.
///
/// Format detection:
/// - No `[gx]` section + `"v4"` style values → v1 format, parse via `from_v1()`, `migrated = true`
/// - `[gx]` section present → old v2 format, strip section, `migrated = true`
/// - No `[gx]` section + `"^4"` style values → current format, `migrated = false`
///
/// # Errors
///
/// Returns [`Error::Read`] if the file cannot be read.
/// Returns [`Error::Parse`] if the TOML is invalid.
/// Returns [`Error::Validation`] if the manifest data is invalid.
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

    let has_gx_section = data.gx.is_some();

    if has_gx_section {
        // Old v2 format with [gx] section — ignore the section, set migrated = true
        // Parse values as v2 (semver specifiers like "^4")
        let manifest = manifest_from_data(data, path, true)?;
        Ok(Parsed {
            value: manifest,
            migrated: true,
        })
    } else {
        // No [gx] section — could be v1 (old "v4" style) or current format ("^4" style)
        // Detect v1 by checking if any value looks like v1 format
        let is_v1 = data.actions.versions.values().any(|v| {
            v.starts_with('v') && v[1..].chars().next().is_some_and(|c| c.is_ascii_digit())
        });

        let manifest = manifest_from_data(data, path, !is_v1)?;
        Ok(Parsed {
            value: manifest,
            migrated: is_v1,
        })
    }
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

    let doc = build_manifest_document(&manifest);
    fs::write(path, doc.to_string()).map_err(|source| Error::Write {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(())
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
