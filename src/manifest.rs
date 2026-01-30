use log::info;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

pub const MANIFEST_FILE_NAME: &str = "gx.toml";

/// Trait defining operations on a manifest (action → version mapping)
pub trait Manifest {
    /// Get a reference to the actions map
    fn actions(&self) -> &HashMap<String, String>;

    /// Set or update an action version
    fn set(&mut self, action: String, version: String);

    /// Remove an action
    fn remove(&mut self, action: &str);

    /// Save the manifest only if there were changes.
    ///
    /// # Errors
    ///
    /// Returns an error if saving is required but fails.
    fn save_if_changed(&mut self) -> Result<(), ManifestError>;

    /// Get the path to the manifest file
    ///
    /// # Errors
    /// Returns `PathNotInitialized` if the path has not been initialized
    fn path(&self) -> Result<&Path, ManifestError>;
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
    PathNotInitialized(),
}

/// The main manifest structure mapping actions to versions
#[derive(Debug, Default, Deserialize, Serialize)]
pub struct FileManifest {
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub actions: HashMap<String, String>,
    #[serde(skip)]
    path: Option<PathBuf>,
    #[serde(skip)]
    changed: bool,
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

        let mut manifest: FileManifest =
            toml::from_str(&content).map_err(|source| ManifestError::Parse {
                path: path.to_path_buf(),
                source: Box::new(source),
            })?;

        manifest.path = Some(path.to_path_buf());

        Ok(manifest)
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
            let manifest = Self {
                path: Some(path.to_path_buf()),
                ..Default::default()
            };
            Ok(manifest)
        }
    }

    /// Save the manifest to disk.
    ///
    /// # Errors
    ///
    /// Returns an error if the path is not initialized, serialization fails, or the file cannot be written.
    pub fn save(&self) -> Result<(), ManifestError> {
        let path = self.path()?;
        let content = toml::to_string_pretty(self).map_err(ManifestError::Serialize)?;

        fs::write(path, content).map_err(|source| ManifestError::Write {
            path: path.to_path_buf(),
            source,
        })?;

        info!("Manifest updated: {}", path.display());
        Ok(())
    }
}

impl Manifest for FileManifest {
    fn actions(&self) -> &HashMap<String, String> {
        &self.actions
    }

    fn set(&mut self, action: String, version: String) {
        let existing = self.actions.get(&action);
        if existing != Some(&version) {
            self.actions.insert(action, version);
            self.changed = true;
        }
    }

    fn remove(&mut self, action: &str) {
        if self.actions.remove(action).is_some() {
            self.changed = true;
        }
    }

    fn save_if_changed(&mut self) -> Result<(), ManifestError> {
        if self.changed { self.save() } else { Ok(()) }
    }

    fn path(&self) -> Result<&Path, ManifestError> {
        self.path
            .as_deref()
            .ok_or_else(ManifestError::PathNotInitialized)
    }
}

/// In-memory manifest that doesn't persist to disk
#[derive(Debug, Default)]
pub struct MemoryManifest {
    pub actions: HashMap<String, String>,
}

impl Manifest for MemoryManifest {
    fn actions(&self) -> &HashMap<String, String> {
        &self.actions
    }

    fn set(&mut self, action: String, version: String) {
        self.actions.insert(action, version);
    }

    fn remove(&mut self, action: &str) {
        self.actions.remove(action);
    }

    fn save_if_changed(&mut self) -> Result<(), ManifestError> {
        Ok(()) // no-op for in-memory
    }

    fn path(&self) -> Result<&Path, ManifestError> {
        Ok(Path::new("in-memory"))
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
            manifest.actions.get("actions/checkout"),
            Some(&"v4".to_string())
        );
        assert_eq!(
            manifest.actions.get("actions/setup-node"),
            Some(&"v4".to_string())
        );
        assert_eq!(
            manifest.actions.get("docker/build-push-action"),
            Some(&"v5".to_string())
        );
    }

    #[test]
    fn test_empty_actions() {
        let content = "[actions]\n";

        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();

        let manifest = FileManifest::load(file.path()).unwrap();
        assert!(manifest.actions.is_empty());
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
            manifest.actions.get("actions/checkout"),
            Some(&"v4".to_string())
        );
    }

    #[test]
    fn test_load_or_default_missing() {
        let manifest =
            FileManifest::load_or_default(Path::new("/nonexistent/path/gx.toml")).unwrap();
        assert!(manifest.actions.is_empty());
    }

    #[test]
    fn test_save_and_load() {
        let mut manifest = FileManifest::default();
        manifest
            .actions
            .insert("actions/checkout".to_string(), "v4".to_string());
        manifest
            .actions
            .insert("actions/setup-node".to_string(), "v3".to_string());

        let file = NamedTempFile::new().unwrap();
        manifest.path = Some(file.path().to_path_buf());
        manifest.save().unwrap();

        let loaded = FileManifest::load(file.path()).unwrap();
        assert_eq!(
            loaded.actions.get("actions/checkout"),
            Some(&"v4".to_string())
        );
        assert_eq!(
            loaded.actions.get("actions/setup-node"),
            Some(&"v3".to_string())
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
    fn test_save_without_path_fails() {
        let manifest = FileManifest::default();
        let result = manifest.save();
        assert!(result.is_err());
    }
}
