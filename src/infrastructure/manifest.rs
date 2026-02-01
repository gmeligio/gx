use log::info;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

use crate::domain::{ActionId, ActionSpec, Version};

pub const MANIFEST_FILE_NAME: &str = "gx.toml";

/// Trait defining operations on a manifest (action → version mapping)
pub trait ManifestStore {
    /// Get the version for an action
    fn get(&self, id: &ActionId) -> Option<&Version>;

    /// Set or update an action version
    fn set(&mut self, id: ActionId, version: Version);

    /// Check if the manifest contains an action
    fn has(&self, id: &ActionId) -> bool;

    /// Save the manifest only if there were changes.
    ///
    /// # Errors
    ///
    /// Returns an error if saving is required but fails.
    fn save(&mut self) -> Result<(), ManifestError>;

    /// Get all action specs from the manifest
    fn specs(&self) -> Vec<ActionSpec>;

    /// Remove an action
    fn remove(&mut self, id: &ActionId);

    /// Get the path to the manifest file
    ///
    /// # Errors
    /// Returns `PathNotInitialized` if the path has not been initialized
    fn path(&self) -> Result<&Path, ManifestError>;

    /// Check if manifest is empty
    fn is_empty(&self) -> bool;
}

/// Errors that can occur when working with manifest files
#[derive(Debug, Error)]
pub enum ManifestError {
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

    #[error(
        "`ManifestFile.path` not initialized. Use load_or_default or load to create a ManifestFile with a path."
    )]
    PathNotInitialized,
}

/// Internal structure for TOML serialization
#[derive(Debug, Default, Deserialize, Serialize)]
struct ManifestData {
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    actions: HashMap<String, String>,
}

/// The main manifest structure mapping actions to versions
#[derive(Debug, Default)]
pub struct FileManifest {
    /// Maps `ActionId` to `Version`
    actions: HashMap<ActionId, Version>,
    path: Option<PathBuf>,
    dirty: bool,
}

impl FileManifest {
    /// Load a manifest from the given path.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read or parsed.
    pub fn load(path: &Path) -> Result<Self, ManifestError> {
        let content = fs::read_to_string(path).map_err(|source| ManifestError::Read {
            path: path.to_path_buf(),
            source,
        })?;

        let data: ManifestData =
            toml::from_str(&content).map_err(|source| ManifestError::Parse {
                path: path.to_path_buf(),
                source: Box::new(source),
            })?;

        let actions = data
            .actions
            .into_iter()
            .map(|(k, v)| (ActionId(k), Version(v)))
            .collect();

        Ok(Self {
            actions,
            path: Some(path.to_path_buf()),
            ..Default::default()
        })
    }

    /// Load a manifest from the given path, or return a default if it doesn't exist.
    ///
    /// # Errors
    ///
    /// Returns an error if the file exists but cannot be read or parsed.
    pub fn load_or_default(path: &Path) -> Result<Self, ManifestError> {
        if path.exists() {
            Self::load(path)
        } else {
            Ok(Self {
                path: Some(path.to_path_buf()),
                ..Default::default()
            })
        }
    }

    fn save_to_disk(&self) -> Result<(), ManifestError> {
        let path = self.path()?;

        // Convert to serializable format
        let data = ManifestData {
            actions: self
                .actions
                .iter()
                .map(|(k, v)| (k.0.clone(), v.0.clone()))
                .collect(),
        };

        let content = toml::to_string_pretty(&data).map_err(ManifestError::Serialize)?;

        fs::write(path, content).map_err(|source| ManifestError::Write {
            path: path.to_path_buf(),
            source,
        })?;

        info!("Manifest updated: {}", path.display());
        Ok(())
    }
}

impl ManifestStore for FileManifest {
    fn specs(&self) -> Vec<ActionSpec> {
        self.actions
            .iter()
            .map(|(id, version)| ActionSpec::new(id.clone(), version.clone()))
            .collect()
    }

    fn set(&mut self, id: ActionId, version: Version) {
        let existing = self.actions.get(&id);
        if existing != Some(&version) {
            self.actions.insert(id, version);
            self.dirty = true;
        }
    }

    fn remove(&mut self, id: &ActionId) {
        if self.actions.remove(id).is_some() {
            self.dirty = true;
        }
    }

    fn has(&self, id: &ActionId) -> bool {
        self.actions.contains_key(id)
    }

    fn get(&self, id: &ActionId) -> Option<&Version> {
        self.actions.get(id)
    }

    fn save(&mut self) -> Result<(), ManifestError> {
        if self.dirty {
            self.save_to_disk()?;
            self.dirty = false;
        }
        Ok(())
    }

    fn path(&self) -> Result<&Path, ManifestError> {
        self.path
            .as_deref()
            .ok_or(ManifestError::PathNotInitialized)
    }

    fn is_empty(&self) -> bool {
        self.actions.is_empty()
    }
}

/// In-memory manifest that doesn't persist to disk
#[derive(Debug, Default)]
pub struct MemoryManifest {
    actions: HashMap<ActionId, Version>,
}

impl ManifestStore for MemoryManifest {
    fn specs(&self) -> Vec<ActionSpec> {
        self.actions
            .iter()
            .map(|(id, version)| ActionSpec::new(id.clone(), version.clone()))
            .collect()
    }

    fn set(&mut self, id: ActionId, version: Version) {
        self.actions.insert(id, version);
    }

    fn remove(&mut self, id: &ActionId) {
        self.actions.remove(id);
    }

    fn has(&self, id: &ActionId) -> bool {
        self.actions.contains_key(id)
    }

    fn get(&self, id: &ActionId) -> Option<&Version> {
        self.actions.get(id)
    }

    fn save(&mut self) -> Result<(), ManifestError> {
        Ok(()) // no-op for in-memory
    }

    fn path(&self) -> Result<&Path, ManifestError> {
        Ok(Path::new("in-memory"))
    }

    fn is_empty(&self) -> bool {
        self.actions.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_parse_manifest() {
        let content = r#"
[actions]
"actions/checkout" = "v4"
"actions/setup-node" = "v4"
"docker/build-push-action" = "v5"
"#;

        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();

        let manifest = FileManifest::load(file.path()).unwrap();

        assert_eq!(
            manifest.get(&ActionId::from("actions/checkout")),
            Some(&Version::from("v4"))
        );
        assert_eq!(
            manifest.get(&ActionId::from("actions/setup-node")),
            Some(&Version::from("v4"))
        );
        assert_eq!(
            manifest.get(&ActionId::from("docker/build-push-action")),
            Some(&Version::from("v5"))
        );
    }

    #[test]
    fn test_empty_actions() {
        let content = "[actions]\n";

        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();

        let manifest = FileManifest::load(file.path()).unwrap();
        assert!(manifest.is_empty());
    }

    #[test]
    fn test_load_or_default_existing() {
        let content = r#"
[actions]
"actions/checkout" = "v4"
"#;
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();

        let manifest = FileManifest::load_or_default(file.path()).unwrap();
        assert_eq!(
            manifest.get(&ActionId::from("actions/checkout")),
            Some(&Version::from("v4"))
        );
    }

    #[test]
    fn test_load_or_default_missing() {
        let manifest =
            FileManifest::load_or_default(Path::new("/nonexistent/path/gx.toml")).unwrap();
        assert!(manifest.is_empty());
    }

    #[test]
    fn test_save_and_load() {
        let mut manifest = FileManifest::default();
        manifest.set(ActionId::from("actions/checkout"), Version::from("v4"));
        manifest.set(ActionId::from("actions/setup-node"), Version::from("v3"));

        let file = NamedTempFile::new().unwrap();
        manifest.path = Some(file.path().to_path_buf());
        manifest.dirty = true;
        manifest.save().unwrap();

        let loaded = FileManifest::load(file.path()).unwrap();
        assert_eq!(
            loaded.get(&ActionId::from("actions/checkout")),
            Some(&Version::from("v4"))
        );
        assert_eq!(
            loaded.get(&ActionId::from("actions/setup-node")),
            Some(&Version::from("v3"))
        );
    }

    #[test]
    fn test_path_not_initialized_error() {
        let manifest = FileManifest::default();
        let result = manifest.path();
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("`ManifestFile.path` not initialized"));
    }

    #[test]
    fn test_specs() {
        let mut manifest = MemoryManifest::default();
        manifest.set(ActionId::from("actions/checkout"), Version::from("v4"));
        manifest.set(ActionId::from("actions/setup-node"), Version::from("v3"));

        let specs = manifest.specs();
        assert_eq!(specs.len(), 2);
    }

    #[test]
    fn test_contains() {
        let mut manifest = MemoryManifest::default();
        manifest.set(ActionId::from("actions/checkout"), Version::from("v4"));

        assert!(manifest.has(&ActionId::from("actions/checkout")));
        assert!(!manifest.has(&ActionId::from("actions/setup-node")));
    }
}
