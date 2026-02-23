use log::info;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

use crate::domain::{ActionId, ActionSpec, Manifest, Version, WorkflowActionSet};

pub const MANIFEST_FILE_NAME: &str = "gx.toml";

/// Pure I/O trait for loading and saving the manifest.
/// Domain operations (get, set, remove, `detect_drift`, etc.) live on `Manifest`.
pub trait ManifestStore {
    /// Load the manifest from storage, returning a `Manifest` domain entity.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read or parsed.
    fn load(&self) -> Result<Manifest, ManifestError>;

    /// Save the given `Manifest` to storage.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be written.
    fn save(&self, manifest: &Manifest) -> Result<(), ManifestError>;

    /// The path this store reads from and writes to.
    fn path(&self) -> &Path;
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
}

/// Internal structure for TOML serialization
#[derive(Debug, Default, Deserialize, Serialize)]
struct ManifestData {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    actions: BTreeMap<String, String>,
}

fn manifest_from_data(data: ManifestData) -> Manifest {
    let actions: HashMap<ActionId, ActionSpec> = data
        .actions
        .into_iter()
        .map(|(k, v)| {
            let id = ActionId::from(k);
            let version = Version::from(v);
            let spec = ActionSpec::new(id.clone(), version);
            (id, spec)
        })
        .collect();
    Manifest::new(actions)
}

fn manifest_to_data(manifest: &Manifest) -> ManifestData {
    let actions = manifest
        .specs()
        .into_iter()
        .map(|spec| {
            (
                spec.id.as_str().to_owned(),
                spec.version.as_str().to_owned(),
            )
        })
        .collect();
    ManifestData { actions }
}

/// File-backed manifest store. Reads from and writes to `.github/gx.toml`.
pub struct FileManifest {
    path: PathBuf,
}

impl FileManifest {
    #[must_use]
    pub fn new(path: &Path) -> Self {
        Self {
            path: path.to_path_buf(),
        }
    }
}

impl ManifestStore for FileManifest {
    fn load(&self) -> Result<Manifest, ManifestError> {
        if !self.path.exists() {
            return Ok(Manifest::default());
        }

        let content = fs::read_to_string(&self.path).map_err(|source| ManifestError::Read {
            path: self.path.clone(),
            source,
        })?;

        let data: ManifestData =
            toml::from_str(&content).map_err(|source| ManifestError::Parse {
                path: self.path.clone(),
                source: Box::new(source),
            })?;

        Ok(manifest_from_data(data))
    }

    fn save(&self, manifest: &Manifest) -> Result<(), ManifestError> {
        let data = manifest_to_data(manifest);
        let content = toml::to_string_pretty(&data).map_err(ManifestError::Serialize)?;
        fs::write(&self.path, content).map_err(|source| ManifestError::Write {
            path: self.path.clone(),
            source,
        })?;
        info!("Manifest updated: {}", self.path.display());
        Ok(())
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

/// In-memory manifest store that doesn't persist to disk. Used when no `gx.toml` exists.
#[derive(Default)]
pub struct MemoryManifest {
    initial: Option<Manifest>,
}

impl MemoryManifest {
    /// Create a store pre-seeded with a manifest built from workflow actions.
    /// For each action, picks the highest semantic version found across workflows.
    #[must_use]
    pub fn from_workflows(action_set: &WorkflowActionSet) -> Self {
        let mut manifest = Manifest::default();
        for action_id in action_set.action_ids() {
            let versions = action_set.versions_for(&action_id);
            let version = Version::highest(&versions).unwrap_or_else(|| versions[0].clone());
            manifest.set(action_id, version);
        }
        Self {
            initial: Some(manifest),
        }
    }
}

impl ManifestStore for MemoryManifest {
    fn load(&self) -> Result<Manifest, ManifestError> {
        Ok(self.initial.as_ref().map_or_else(Manifest::default, |m| {
            // Re-build from specs — Manifest doesn't implement Clone, build from specs
            let mut fresh = Manifest::default();
            for spec in m.specs() {
                fresh.set(spec.id.clone(), spec.version.clone());
            }
            fresh
        }))
    }

    fn save(&self, _manifest: &Manifest) -> Result<(), ManifestError> {
        Ok(()) // no-op
    }

    fn path(&self) -> &Path {
        Path::new("in-memory")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_file_manifest_load_missing_returns_empty() {
        let store = FileManifest::new(Path::new("/nonexistent/path/gx.toml"));
        let manifest = store.load().unwrap();
        assert!(manifest.is_empty());
    }

    #[test]
    fn test_file_manifest_save_and_load_roundtrip() {
        let file = NamedTempFile::new().unwrap();
        let store = FileManifest::new(file.path());

        let mut manifest = Manifest::default();
        manifest.set(ActionId::from("actions/checkout"), Version::from("v4"));
        manifest.set(ActionId::from("actions/setup-node"), Version::from("v3"));

        store.save(&manifest).unwrap();

        let loaded = store.load().unwrap();
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
    fn test_file_manifest_load_existing_toml() {
        let content = r#"
[actions]
"actions/checkout" = "v4"
"actions/setup-node" = "v4"
"#;
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();

        let store = FileManifest::new(file.path());
        let manifest = store.load().unwrap();
        assert_eq!(
            manifest.get(&ActionId::from("actions/checkout")),
            Some(&Version::from("v4"))
        );
    }

    #[test]
    fn test_file_manifest_save_sorts_actions_alphabetically() {
        let file = NamedTempFile::new().unwrap();
        let store = FileManifest::new(file.path());

        let mut manifest = Manifest::default();
        manifest.set(
            ActionId::from("docker/build-push-action"),
            Version::from("v5"),
        );
        manifest.set(ActionId::from("actions/checkout"), Version::from("v4"));
        manifest.set(
            ActionId::from("actions-rust-lang/rustfmt"),
            Version::from("v1"),
        );

        store.save(&manifest).unwrap();

        let content = fs::read_to_string(file.path()).unwrap();
        let action_lines: Vec<&str> = content
            .lines()
            .filter(|l| l.trim().starts_with('"'))
            .collect();

        let mut sorted = action_lines.clone();
        sorted.sort_unstable();
        assert_eq!(action_lines, sorted);
        assert!(action_lines[0].contains("actions-rust-lang/rustfmt"));
        assert!(action_lines[1].contains("actions/checkout"));
        assert!(action_lines[2].contains("docker/build-push-action"));
    }

    #[test]
    fn test_memory_manifest_load_returns_empty_by_default() {
        let store = MemoryManifest::default();
        let manifest = store.load().unwrap();
        assert!(manifest.is_empty());
    }

    #[test]
    fn test_memory_manifest_from_workflows() {
        use crate::domain::{InterpretedRef, WorkflowActionSet};
        let mut action_set = WorkflowActionSet::new();
        action_set.add(&InterpretedRef {
            id: ActionId::from("actions/checkout"),
            version: Version::from("v4"),
            sha: None,
        });

        let store = MemoryManifest::from_workflows(&action_set);
        let manifest = store.load().unwrap();
        assert_eq!(
            manifest.get(&ActionId::from("actions/checkout")),
            Some(&Version::from("v4"))
        );
    }

    #[test]
    fn test_memory_manifest_save_is_noop() {
        let store = MemoryManifest::default();
        let manifest = Manifest::default();
        assert!(store.save(&manifest).is_ok());
    }
}
