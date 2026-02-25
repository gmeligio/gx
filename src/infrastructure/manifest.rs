use log::info;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

use crate::domain::{ActionId, ActionOverride, ActionSpec, Manifest, Version, WorkflowActionSet};

pub const MANIFEST_FILE_NAME: &str = "gx.toml";

/// Pure I/O trait for loading and saving the manifest.
/// Domain operations (get, set, remove, etc.) live on `Manifest`.
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

    #[error("invalid manifest: {0}")]
    Validation(String),
}

// ---- TOML wire types ----

#[derive(Debug, Default, Deserialize, Serialize)]
struct TomlOverride {
    workflow: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    job: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    step: Option<usize>,
    version: String,
}

/// The [actions] section: flat string entries + optional [actions.overrides] sub-table.
#[derive(Debug, Default, Deserialize, Serialize)]
struct TomlActions {
    #[serde(default, flatten)]
    versions: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    overrides: BTreeMap<String, Vec<TomlOverride>>,
}

#[derive(Debug, Default, Deserialize, Serialize)]
struct ManifestData {
    #[serde(default)]
    actions: TomlActions,
}

// ---- conversion ----

fn manifest_from_data(data: ManifestData, _path: &Path) -> Result<Manifest, ManifestError> {
    // Build global actions map
    let actions: HashMap<ActionId, ActionSpec> = data
        .actions
        .versions
        .into_iter()
        .map(|(k, v)| {
            let id = ActionId::from(k);
            let version = Version::from(v);
            let spec = ActionSpec::new(id.clone(), version);
            (id, spec)
        })
        .collect();

    // Validate and convert overrides
    let mut overrides: HashMap<ActionId, Vec<ActionOverride>> = HashMap::new();

    for (action_str, toml_overrides) in data.actions.overrides {
        let id = ActionId::from(action_str.clone());

        // Validation: override without global default is an error
        if !actions.contains_key(&id) {
            return Err(ManifestError::Validation(format!(
                "\"{action_str}\" has overrides but no global version — run 'gx tidy' to fix"
            )));
        }

        let mut seen_scopes: Vec<(String, Option<String>, Option<usize>)> = Vec::new();

        let mut converted = Vec::new();
        for exc in toml_overrides {
            // Validation: step without job
            if exc.step.is_some() && exc.job.is_none() {
                return Err(ManifestError::Validation(format!(
                    "override for \"{}\" in \"{}\" has a step but no job",
                    action_str, exc.workflow
                )));
            }

            // Validation: duplicate scope
            let scope = (exc.workflow.clone(), exc.job.clone(), exc.step);
            if seen_scopes.contains(&scope) {
                return Err(ManifestError::Validation(format!(
                    "duplicate override scope for \"{}\" in \"{}\"",
                    action_str, exc.workflow
                )));
            }
            seen_scopes.push(scope);

            converted.push(ActionOverride {
                workflow: exc.workflow,
                job: exc.job,
                step: exc.step,
                version: Version::from(exc.version),
            });
        }
        overrides.insert(id, converted);
    }

    Ok(Manifest::with_overrides(actions, overrides))
}

fn manifest_to_data(manifest: &Manifest) -> ManifestData {
    let versions: BTreeMap<String, String> = manifest
        .specs()
        .into_iter()
        .map(|spec| {
            (
                spec.id.as_str().to_owned(),
                spec.version.as_str().to_owned(),
            )
        })
        .collect();

    let overrides: BTreeMap<String, Vec<TomlOverride>> = {
        let mut map: BTreeMap<String, Vec<TomlOverride>> = BTreeMap::new();
        for (id, excs) in manifest.all_overrides() {
            if excs.is_empty() {
                continue;
            }
            let toml_excs: Vec<TomlOverride> = excs
                .iter()
                .map(|e| TomlOverride {
                    workflow: e.workflow.clone(),
                    job: e.job.clone(),
                    step: e.step,
                    version: e.version.as_str().to_owned(),
                })
                .collect();
            map.insert(id.as_str().to_owned(), toml_excs);
        }
        map
    };

    ManifestData {
        actions: TomlActions {
            versions,
            overrides,
        },
    }
}

// ---- Formatting ----

/// Formats the manifest data as TOML with proper inline table syntax for overrides.
fn format_manifest_toml(data: &ManifestData) -> String {
    use std::fmt::Write as FmtWrite;

    let mut output = String::new();

    // Write [actions] section header
    output.push_str("[actions]\n");

    // Write global action versions (sorted alphabetically)
    for (action_id, version) in &data.actions.versions {
        writeln!(output, "\"{action_id}\" = \"{version}\"").ok();
    }

    // Write [actions.overrides] section if there are any overrides
    if !data.actions.overrides.is_empty() {
        output.push('\n');
        output.push_str("[actions.overrides]\n");

        // Write overrides with inline table arrays (sorted alphabetically by action ID)
        for (action_id, overrides) in &data.actions.overrides {
            writeln!(output, "\"{action_id}\" = [").ok();

            for override_entry in overrides {
                write!(output, "  {{ workflow = \"{}\"", override_entry.workflow).ok();

                if let Some(job) = &override_entry.job {
                    write!(output, ", job = \"{job}\"").ok();
                }

                if let Some(step) = override_entry.step {
                    write!(output, ", step = {step}").ok();
                }

                writeln!(output, ", version = \"{}\" }},", override_entry.version).ok();
            }

            output.push_str("]\n");
        }
    }

    output.push('\n');
    output
}

// ---- FileManifest ----

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

        manifest_from_data(data, &self.path)
    }

    fn save(&self, manifest: &Manifest) -> Result<(), ManifestError> {
        let data = manifest_to_data(manifest);
        let content = format_manifest_toml(&data);
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

// ---- MemoryManifest ----

#[derive(Default)]
pub struct MemoryManifest {
    initial: Option<Manifest>,
}

impl MemoryManifest {
    /// Create a store pre-seeded with a manifest built from workflow actions.
    /// For each action, picks the dominant version (most-used; tiebreak: highest semver).
    #[must_use]
    pub fn from_workflows(action_set: &WorkflowActionSet) -> Self {
        let mut manifest = Manifest::default();
        for action_id in action_set.action_ids() {
            let version = action_set.dominant_version(&action_id).unwrap_or_else(|| {
                let versions = action_set.versions_for(&action_id);
                Version::highest(&versions).unwrap_or_else(|| versions[0].clone())
            });
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
            // Copy over overrides
            for (id, excs) in m.all_overrides() {
                for exc in excs {
                    fresh.add_override(id.clone(), exc.clone());
                }
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
            .filter(|l| l.trim().starts_with('"') && l.contains(" = ") && !l.contains('['))
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

    #[test]
    fn test_load_manifest_with_overrides() {
        let content = r#"
[actions]
"actions/checkout" = "v4"

[actions.overrides]
"actions/checkout" = [
  { workflow = ".github/workflows/deploy.yml", version = "v3" },
  { workflow = ".github/workflows/ci.yml", job = "legacy-build", version = "v2" },
]
"#;
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();

        let store = FileManifest::new(file.path());
        let manifest = store.load().unwrap();

        assert_eq!(
            manifest.get(&ActionId::from("actions/checkout")),
            Some(&Version::from("v4"))
        );

        let overrides = manifest.overrides_for(&ActionId::from("actions/checkout"));
        assert_eq!(overrides.len(), 2);
        assert_eq!(overrides[0].workflow, ".github/workflows/deploy.yml");
        assert_eq!(overrides[0].version.as_str(), "v3");
        assert_eq!(overrides[1].job.as_deref(), Some("legacy-build"));
        assert_eq!(overrides[1].version.as_str(), "v2");
    }

    #[test]
    fn test_save_and_load_roundtrip_with_overrides() {
        let file = NamedTempFile::new().unwrap();
        let store = FileManifest::new(file.path());

        let mut manifest = Manifest::default();
        manifest.set(ActionId::from("actions/checkout"), Version::from("v4"));
        manifest.add_override(
            ActionId::from("actions/checkout"),
            ActionOverride {
                workflow: ".github/workflows/deploy.yml".to_string(),
                job: None,
                step: None,
                version: Version::from("v3"),
            },
        );

        store.save(&manifest).unwrap();
        let content = fs::read_to_string(file.path()).unwrap();
        assert!(
            content.contains("actions.overrides"),
            "Expected overrides section, got:\n{content}"
        );

        let loaded = store.load().unwrap();
        let overrides = loaded.overrides_for(&ActionId::from("actions/checkout"));
        assert_eq!(overrides.len(), 1);
        assert_eq!(overrides[0].workflow, ".github/workflows/deploy.yml");
        assert_eq!(overrides[0].version.as_str(), "v3");
    }

    #[test]
    fn test_load_override_without_global_is_error() {
        let content = r#"
[actions]
"actions/setup-node" = "v4"

[actions.overrides]
"actions/checkout" = [
  { workflow = ".github/workflows/deploy.yml", version = "v3" },
]
"#;
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();

        let store = FileManifest::new(file.path());
        let result = store.load();
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("actions/checkout"), "got: {err}");
        assert!(err.to_string().contains("gx tidy"), "got: {err}");
    }

    #[test]
    fn test_load_override_step_without_job_is_error() {
        let content = r#"
[actions]
"actions/checkout" = "v4"

[actions.overrides]
"actions/checkout" = [
  { workflow = ".github/workflows/ci.yml", step = 0, version = "v3" },
]
"#;
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();

        let store = FileManifest::new(file.path());
        let result = store.load();
        assert!(result.is_err());
    }

    #[test]
    fn test_load_duplicate_scope_is_error() {
        let content = r#"
[actions]
"actions/checkout" = "v4"

[actions.overrides]
"actions/checkout" = [
  { workflow = ".github/workflows/deploy.yml", version = "v3" },
  { workflow = ".github/workflows/deploy.yml", version = "v2" },
]
"#;
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();

        let store = FileManifest::new(file.path());
        let result = store.load();
        assert!(result.is_err());
    }

    #[test]
    fn test_save_no_overrides_section_when_empty() {
        let file = NamedTempFile::new().unwrap();
        let store = FileManifest::new(file.path());

        let mut manifest = Manifest::default();
        manifest.set(ActionId::from("actions/checkout"), Version::from("v4"));

        store.save(&manifest).unwrap();
        let content = fs::read_to_string(file.path()).unwrap();
        assert!(!content.contains("overrides"), "got:\n{content}");
    }

    #[test]
    fn test_save_and_load_roundtrip_generates_correct_toml_format() {
        let file = NamedTempFile::new().unwrap();
        let store = FileManifest::new(file.path());

        let mut manifest = Manifest::default();
        manifest.set(ActionId::from("actions/checkout"), Version::from("v4"));
        manifest.add_override(
            ActionId::from("actions/checkout"),
            ActionOverride {
                workflow: ".github/workflows/windows.yml".to_string(),
                job: Some("test_windows".to_string()),
                step: Some(0),
                version: Version::from("v5"),
            },
        );

        store.save(&manifest).unwrap();
        let content = fs::read_to_string(file.path()).unwrap();

        // The format should be [actions.overrides] with inline table array syntax,
        // NOT [[actions.overrides."actions/checkout"]]
        assert!(
            content.contains("[actions.overrides]"),
            "Expected [actions.overrides] section, got:\n{content}"
        );
        assert!(
            !content.contains("[[actions.overrides"),
            "Should not use array-of-tables syntax, got:\n{content}"
        );
        assert!(
            content.contains(r#""actions/checkout" = ["#),
            "Expected inline table array syntax, got:\n{content}"
        );

        // Verify it can be loaded back correctly
        let loaded = store.load().unwrap();
        let overrides = loaded.overrides_for(&ActionId::from("actions/checkout"));
        assert_eq!(overrides.len(), 1);
        assert_eq!(overrides[0].workflow, ".github/workflows/windows.yml");
        assert_eq!(overrides[0].job.as_deref(), Some("test_windows"));
        assert_eq!(overrides[0].step, Some(0));
        assert_eq!(overrides[0].version.as_str(), "v5");
    }
}
