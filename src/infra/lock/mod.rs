use crate::domain::Parsed;
use crate::domain::lock::Lock;
use crate::domain::plan::LockDiff;
use convert::{LockData, build_lock_document, lock_from_data, populate_lock_table};
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;
use toml_edit::DocumentMut;

mod convert;
mod migration;

pub const LOCK_FILE_NAME: &str = "gx.lock";

/// Errors that can occur when working with lock files
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

    #[error("failed to serialize lock file to TOML")]
    Serialize(#[source] toml::ser::Error),

    #[error("invalid lock file: {0}")]
    Validation(String),
}

/// File-backed lock store. Reads from and writes to `.github/gx.lock`.
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
    /// Save the given `Lock` to this file.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Write`] if the file cannot be written.
    pub fn save(&self, lock: &Lock) -> Result<(), Error> {
        let doc = build_lock_document(lock);
        fs::write(&self.path, doc.to_string()).map_err(|source| Error::Write {
            path: self.path.clone(),
            source,
        })?;
        Ok(())
    }
}

/// Load a lock from a file path. Returns `Parsed<Lock>` with `migrated = false` if the file
/// does not exist. Dispatches to the appropriate parser/migrator based on the version field.
/// This is a pure function — it does NOT rewrite the file.
///
/// # Errors
///
/// Returns [`Error::Read`] if the file cannot be read.
/// Returns [`Error::Parse`] if the TOML is invalid.
pub fn parse(path: &Path) -> Result<Parsed<Lock>, Error> {
    #[derive(Deserialize)]
    struct VersionOnly {
        #[serde(default)]
        version: String,
    }

    if !path.exists() {
        return Ok(Parsed {
            value: Lock::default(),
            migrated: false,
        });
    }

    let content = fs::read_to_string(path).map_err(|source| Error::Read {
        path: path.to_path_buf(),
        source,
    })?;

    // Try to detect v1.0 format (plain string SHA values, no version field with inline tables)
    // by attempting v1.0 parse first (HashMap<String, String> for actions)
    if let Ok(v1) = toml::from_str::<migration::LockDataV1>(&content) {
        // Only use v1 parser if actions are actually strings (v1.0 format)
        // The v1.0 format has no version field or version = "" and plain string values
        if !v1.actions.is_empty() && !content.contains("sha =") {
            let data = migration::migrate_v1(v1);
            let lock = lock_from_data(data);
            return Ok(Parsed {
                value: lock,
                migrated: true,
            });
        }
    }

    // Parse the version field to dispatch
    let version_only: VersionOnly = toml::from_str(&content).map_err(|source| Error::Parse {
        path: path.to_path_buf(),
        source: Box::new(source),
    })?;

    match version_only.version.as_str() {
        // New format: no version field
        "" => {
            // Could be v1.0 (no actions with sha) or new format (standard tables)
            // If we reach here, it's the new format (v1.0 was already handled above)
            let data: LockData = toml::from_str(&content).map_err(|source| Error::Parse {
                path: path.to_path_buf(),
                source: Box::new(source),
            })?;
            let lock = lock_from_data(data);
            Ok(Parsed {
                value: lock,
                migrated: false,
            })
        }
        "1.0" => {
            // v1.0 format (explicit version = "1.0")
            let v1: migration::LockDataV1 =
                toml::from_str(&content).map_err(|source| Error::Parse {
                    path: path.to_path_buf(),
                    source: Box::new(source),
                })?;
            let data = migration::migrate_v1(v1);
            let lock = lock_from_data(data);
            Ok(Parsed {
                value: lock,
                migrated: true,
            })
        }
        "1.1" | "1.2" | "1.3" => {
            // v1.3 format: specifier field, @v6 style keys
            let v1_3: migration::LockDataV1_3 =
                toml::from_str(&content).map_err(|source| Error::Parse {
                    path: path.to_path_buf(),
                    source: Box::new(source),
                })?;
            let data = migration::migrate_v1_3(v1_3);
            let lock = lock_from_data(data);
            Ok(Parsed {
                value: lock,
                migrated: true,
            })
        }
        "1.4" => {
            // v1.4 format (inline tables with version field) — migrate
            let data: LockData = toml::from_str(&content).map_err(|source| Error::Parse {
                path: path.to_path_buf(),
                source: Box::new(source),
            })?;
            let lock = lock_from_data(data);
            Ok(Parsed {
                value: lock,
                migrated: true,
            })
        }
        _v => {
            // Unknown version field — ignore it (forward compatibility)
            let data: LockData = toml::from_str(&content).map_err(|source| Error::Parse {
                path: path.to_path_buf(),
                source: Box::new(source),
            })?;
            let lock = lock_from_data(data);
            Ok(Parsed {
                value: lock,
                migrated: true,
            })
        }
    }
}

/// Create a new lock file from a `LockDiff`.
///
/// This builds a fresh lock file from the `added` entries.
/// Used for the `init` command when no lock file exists yet.
///
/// # Errors
///
/// Returns [`Error::Write`] if the file cannot be written.
pub fn create(path: &Path, diff: &LockDiff) -> Result<(), Error> {
    let actions = diff.added.iter().cloned().collect();
    let lock = Lock::new(actions);
    let doc = build_lock_document(&lock);
    fs::write(path, doc.to_string()).map_err(|source| Error::Write {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(())
}

/// Apply a `LockDiff` to an existing lock file by building a fresh document.
///
/// Reads the current lock, applies the diff to the domain model, then writes
/// a fresh document. For creating a new lock from scratch, use `create`.
///
/// # Errors
///
/// Returns [`Error::Read`] if the file cannot be read.
/// Returns [`Error::Write`] if the file cannot be written.
/// Returns [`Error::Parse`] if the file cannot be parsed as TOML.
pub fn apply_lock_diff(path: &Path, diff: &LockDiff) -> Result<(), Error> {
    if diff.is_empty() {
        return Ok(());
    }

    let content = fs::read_to_string(path).map_err(|source| Error::Read {
        path: path.to_path_buf(),
        source,
    })?;

    let mut doc: DocumentMut = content
        .parse()
        .map_err(|e| Error::Validation(format!("toml_edit parse error: {e}")))?;

    // Remove the version field if present (migration from old format)
    doc.remove("version");

    // Ensure [actions] table exists
    if doc.get("actions").is_none() {
        doc["actions"] = toml_edit::Item::Table(toml_edit::Table::new());
    }
    let Some(actions) = doc["actions"].as_table_mut() else {
        return Err(Error::Validation("[actions] is not a table".to_string()));
    };

    // Remove entries
    for key in &diff.removed {
        actions.remove(&key.to_string());
    }

    // Add entries as standard tables
    for (key, entry) in &diff.added {
        let mut table = toml_edit::Table::new();
        populate_lock_table(&mut table, key, entry);
        actions.insert(&key.to_string(), toml_edit::Item::Table(table));
    }

    // Update existing entries (patch specific fields)
    for (key, patch) in &diff.updated {
        let key_str = key.to_string();
        if let Some(item) = actions.get_mut(&key_str) {
            // Handle both inline tables (old format) and standard tables (new format)
            if let Some(table) = item.as_table_mut() {
                if let Some(version) = &patch.version {
                    match version {
                        Some(v) => {
                            table.insert("version", toml_edit::value(v.as_str()));
                        }
                        None => {
                            table.remove("version");
                        }
                    }
                }
                if let Some(comment) = &patch.comment {
                    table.insert("comment", toml_edit::value(comment.as_str()));
                }
            } else if let Some(inline) = item.as_inline_table_mut() {
                if let Some(version) = &patch.version {
                    match version {
                        Some(v) => inline.insert("version", v.as_str().into()),
                        None => inline.remove("version"),
                    };
                }
                if let Some(comment) = &patch.comment {
                    inline.insert("comment", comment.as_str().into());
                }
            }
        }
    }

    actions.sort_values();

    fs::write(path, doc.to_string()).map_err(|source| Error::Write {
        path: path.to_path_buf(),
        source,
    })?;

    Ok(())
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
