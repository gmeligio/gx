use log::info;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;
use toml_edit::DocumentMut;

use crate::config::{LintConfig, RuleConfig};
use crate::domain::{ActionId, ActionOverride, ActionSpec, Manifest, ManifestDiff, Version};

pub const MANIFEST_FILE_NAME: &str = "gx.toml";

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
    #[serde(default)]
    lint: LintData,
}

/// The [lint] section of the manifest.
#[derive(Debug, Default, Deserialize, Serialize)]
struct LintData {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    rules: BTreeMap<String, RuleConfig>,
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
        lint: LintData::default(),
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

impl FileManifest {
    /// Save the given `Manifest` to this file.
    ///
    /// # Errors
    ///
    /// Returns [`ManifestError::Write`] if the file cannot be written.
    /// Returns [`ManifestError::Serialize`] if serialization fails.
    pub fn save(&self, manifest: &Manifest) -> Result<(), ManifestError> {
        let data = manifest_to_data(manifest);
        let content = format_manifest_toml(&data);
        fs::write(&self.path, content).map_err(|source| ManifestError::Write {
            path: self.path.clone(),
            source,
        })?;
        info!("Manifest updated: {}", self.path.display());
        Ok(())
    }
}

/// Load a manifest from a file path. Returns `Manifest::default()` if the file does not exist.
///
/// # Errors
///
/// Returns [`ManifestError::Read`] if the file cannot be read.
/// Returns [`ManifestError::Parse`] if the TOML is invalid.
/// Returns [`ManifestError::Validation`] if the manifest data is invalid.
pub fn parse_manifest(path: &Path) -> Result<Manifest, ManifestError> {
    if !path.exists() {
        return Ok(Manifest::default());
    }

    let content = fs::read_to_string(path).map_err(|source| ManifestError::Read {
        path: path.to_path_buf(),
        source,
    })?;

    let data: ManifestData = toml::from_str(&content).map_err(|source| ManifestError::Parse {
        path: path.to_path_buf(),
        source: Box::new(source),
    })?;

    manifest_from_data(data, path)
}

/// Load lint configuration from a manifest file. Returns `LintConfig::default()` if the file does not exist or has no `[lint]` section.
///
/// # Errors
///
/// Returns [`ManifestError::Read`] if the file cannot be read.
/// Returns [`ManifestError::Parse`] if the TOML is invalid.
pub fn parse_lint_config(path: &Path) -> Result<LintConfig, ManifestError> {
    if !path.exists() {
        return Ok(LintConfig::default());
    }

    let content = fs::read_to_string(path).map_err(|source| ManifestError::Read {
        path: path.to_path_buf(),
        source,
    })?;

    let data: ManifestData = toml::from_str(&content).map_err(|source| ManifestError::Parse {
        path: path.to_path_buf(),
        source: Box::new(source),
    })?;

    Ok(LintConfig {
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
/// Returns [`ManifestError::Write`] if the file cannot be written.
pub fn create_manifest(path: &Path, diff: &ManifestDiff) -> Result<(), ManifestError> {
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

    fs::write(path, content).map_err(|source| ManifestError::Write {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(())
}

/// Apply a `ManifestDiff` to an existing manifest file using `toml_edit` for surgical patching.
///
/// The file must already exist. For creating a new manifest from scratch, use `create_manifest`.
///
/// # Errors
///
/// Returns [`ManifestError::Read`] if the file cannot be read.
/// Returns [`ManifestError::Write`] if the file cannot be written.
/// Returns [`ManifestError::Validation`] if the TOML cannot be parsed by `toml_edit`.
pub fn apply_manifest_diff(path: &Path, diff: &ManifestDiff) -> Result<(), ManifestError> {
    if diff.is_empty() {
        return Ok(());
    }

    let content = fs::read_to_string(path).map_err(|source| ManifestError::Read {
        path: path.to_path_buf(),
        source,
    })?;

    let mut doc: DocumentMut = content
        .parse()
        .map_err(|e| ManifestError::Validation(format!("toml_edit parse error: {e}")))?;

    // Ensure [actions] table exists
    if doc.get("actions").is_none() {
        doc["actions"] = toml_edit::Item::Table(toml_edit::Table::new());
    }
    let actions = doc["actions"]
        .as_table_mut()
        .ok_or_else(|| ManifestError::Validation("[actions] is not a table".to_string()))?;

    // Remove actions
    for id in &diff.removed {
        actions.remove(id.as_str());
    }

    // Add actions (sorted insertion for consistency)
    for (id, version) in &diff.added {
        actions.insert(id.as_str(), toml_edit::value(version.as_str()));
    }

    // Update existing action versions
    for (id, version) in &diff.updated {
        actions.insert(id.as_str(), toml_edit::value(version.as_str()));
    }
    actions.sort_values();

    // Handle override removals
    if !diff.overrides_removed.is_empty() {
        apply_override_removals(actions, &diff.overrides_removed);
    }

    // Handle override additions
    if !diff.overrides_added.is_empty() {
        apply_override_additions(actions, &diff.overrides_added);
    }

    fs::write(path, doc.to_string()).map_err(|source| ManifestError::Write {
        path: path.to_path_buf(),
        source,
    })?;

    Ok(())
}

/// Check if an override entry matches a given `ActionOverride` by comparing workflow/job/step.
fn override_entry_matches(
    workflow: Option<&str>,
    job: Option<&str>,
    step: Option<i64>,
    ovr: &ActionOverride,
) -> bool {
    workflow == Some(ovr.workflow.as_str())
        && job == ovr.job.as_deref()
        && step.map(|s| usize::try_from(s).unwrap_or(usize::MAX)) == ovr.step
}

/// Remove matching overrides from the `[actions.overrides]` table.
fn apply_override_removals(
    actions: &mut toml_edit::Table,
    removals: &[(ActionId, Vec<ActionOverride>)],
) {
    let Some(overrides_table) = actions
        .get_mut("overrides")
        .and_then(toml_edit::Item::as_table_mut)
    else {
        return;
    };

    for (id, removed_list) in removals {
        let indices = collect_override_removal_indices(overrides_table, id, removed_list);
        if let Some(arr_item) = overrides_table.get_mut(id.as_str()) {
            if let Some(arr) = arr_item.as_array_of_tables_mut() {
                for i in indices.into_iter().rev() {
                    arr.remove(i);
                }
                if arr.is_empty() {
                    overrides_table.remove(id.as_str());
                }
            } else if let Some(arr) = arr_item.as_array_mut() {
                for i in indices.into_iter().rev() {
                    arr.remove(i);
                }
                if arr.is_empty() {
                    overrides_table.remove(id.as_str());
                }
            }
        }
    }

    if overrides_table.is_empty() {
        actions.remove("overrides");
    }
}

/// Collect indices of override entries that match any of the given overrides to remove.
/// Reads from the table immutably, returning indices to remove.
fn collect_override_removal_indices(
    overrides_table: &toml_edit::Table,
    id: &ActionId,
    removed_list: &[ActionOverride],
) -> Vec<usize> {
    let mut indices = Vec::new();
    let Some(arr_item) = overrides_table.get(id.as_str()) else {
        return indices;
    };

    if let Some(arr) = arr_item.as_array_of_tables() {
        for (i, entry) in arr.iter().enumerate() {
            let wf = entry.get("workflow").and_then(toml_edit::Item::as_str);
            let job = entry.get("job").and_then(toml_edit::Item::as_str);
            let step = entry.get("step").and_then(toml_edit::Item::as_integer);
            for ovr in removed_list {
                if override_entry_matches(wf, job, step, ovr) {
                    indices.push(i);
                    break;
                }
            }
        }
    } else if let Some(arr) = arr_item.as_array() {
        for (i, entry) in arr.iter().enumerate() {
            if let Some(tbl) = entry.as_inline_table() {
                let wf = tbl.get("workflow").and_then(toml_edit::Value::as_str);
                let job = tbl.get("job").and_then(toml_edit::Value::as_str);
                let step = tbl.get("step").and_then(toml_edit::Value::as_integer);
                for ovr in removed_list {
                    if override_entry_matches(wf, job, step, ovr) {
                        indices.push(i);
                        break;
                    }
                }
            }
        }
    }

    indices
}

/// Add new overrides to the `[actions.overrides]` table, creating it if needed.
fn apply_override_additions(
    actions: &mut toml_edit::Table,
    additions: &[(ActionId, ActionOverride)],
) {
    // Ensure overrides sub-table exists
    if actions.get("overrides").is_none() {
        actions.insert("overrides", toml_edit::Item::Table(toml_edit::Table::new()));
    }
    let Some(overrides_table) = actions
        .get_mut("overrides")
        .and_then(toml_edit::Item::as_table_mut)
    else {
        return;
    };

    for (id, ovr) in additions {
        // Get or create the array for this action
        if overrides_table.get(id.as_str()).is_none() {
            overrides_table.insert(id.as_str(), toml_edit::value(toml_edit::Array::new()));
        }
        let arr = overrides_table[id.as_str()]
            .as_array_mut()
            .expect("override entry is always an array");

        // Build the inline table for this override entry
        let mut inline = toml_edit::InlineTable::new();
        inline.insert("workflow", ovr.workflow.as_str().into());
        if let Some(job) = &ovr.job {
            inline.insert("job", job.as_str().into());
        }
        if let Some(step) = ovr.step {
            #[allow(clippy::cast_possible_wrap)]
            inline.insert("step", (step as i64).into());
        }
        inline.insert("version", ovr.version.as_str().into());

        arr.push(inline);
    }
    overrides_table.sort_values();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_file_manifest_save_and_load_roundtrip() {
        let file = NamedTempFile::new().unwrap();
        let store = FileManifest::new(file.path());

        let mut manifest = Manifest::default();
        manifest.set(ActionId::from("actions/checkout"), Version::from("v4"));
        manifest.set(ActionId::from("actions/setup-node"), Version::from("v3"));

        store.save(&manifest).unwrap();

        let loaded = parse_manifest(file.path()).unwrap();
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

        let manifest = parse_manifest(file.path()).unwrap();
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
    fn test_parse_manifest_missing_returns_empty() {
        let manifest = parse_manifest(Path::new("/nonexistent/gx.toml")).unwrap();
        assert!(manifest.is_empty());
    }

    #[test]
    fn test_parse_manifest_reads_file() {
        let content = "[actions]\n\"actions/checkout\" = \"v4\"\n";
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();
        let manifest = parse_manifest(file.path()).unwrap();
        assert_eq!(
            manifest.get(&ActionId::from("actions/checkout")),
            Some(&Version::from("v4"))
        );
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

        let manifest = parse_manifest(file.path()).unwrap();

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

        let loaded = parse_manifest(file.path()).unwrap();
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

        let result = parse_manifest(file.path());
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

        let result = parse_manifest(file.path());
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

        let result = parse_manifest(file.path());
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
        let loaded = parse_manifest(file.path()).unwrap();
        let overrides = loaded.overrides_for(&ActionId::from("actions/checkout"));
        assert_eq!(overrides.len(), 1);
        assert_eq!(overrides[0].workflow, ".github/workflows/windows.yml");
        assert_eq!(overrides[0].job.as_deref(), Some("test_windows"));
        assert_eq!(overrides[0].step, Some(0));
        assert_eq!(overrides[0].version.as_str(), "v5");
    }

    #[test]
    fn parse_lint_config_missing_file_returns_default() {
        let config = parse_lint_config(Path::new("/nonexistent/gx.toml")).unwrap();
        assert!(config.rules.is_empty());
    }

    #[test]
    fn parse_lint_config_no_lint_section_returns_default() {
        let content = r#"
[actions]
"actions/checkout" = "v4"
        "#;
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();

        let config = parse_lint_config(file.path()).unwrap();
        assert!(config.rules.is_empty());
    }

    #[test]
    fn parse_lint_config_with_rules() {
        let content = r#"
[actions]
"actions/checkout" = "v4"

[lint.rules]
sha-mismatch = { level = "error" }
unpinned = { level = "error", ignore = [
  { action = "actions/internal-tool" },
] }
stale-comment = { level = "off" }
        "#;
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();

        let config = parse_lint_config(file.path()).unwrap();
        assert_eq!(config.rules.len(), 3);
        assert!(config.rules.contains_key("sha-mismatch"));
        assert!(config.rules.contains_key("unpinned"));
        assert!(config.rules.contains_key("stale-comment"));
    }

    #[test]
    fn parse_lint_config_ignore_targets() {
        let content = r#"
[actions]
"actions/checkout" = "v4"

[lint.rules]
unpinned = { level = "warn", ignore = [
  { action = "actions/checkout" },
  { workflow = ".github/workflows/legacy.yml" },
  { action = "actions/cache", workflow = ".github/workflows/ci.yml", job = "build" },
] }
        "#;
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();

        let config = parse_lint_config(file.path()).unwrap();
        let unpinned = &config.rules["unpinned"];
        assert_eq!(unpinned.ignore.len(), 3);
        assert_eq!(
            unpinned.ignore[0].action,
            Some("actions/checkout".to_string())
        );
        assert!(unpinned.ignore[0].workflow.is_none());
        assert_eq!(
            unpinned.ignore[1].workflow,
            Some(".github/workflows/legacy.yml".to_string())
        );
        assert_eq!(unpinned.ignore[2].action, Some("actions/cache".to_string()));
        assert_eq!(
            unpinned.ignore[2].workflow,
            Some(".github/workflows/ci.yml".to_string())
        );
        assert_eq!(unpinned.ignore[2].job, Some("build".to_string()));
    }

    // ========== Step 11: apply_manifest_diff tests ==========

    #[test]
    fn test_apply_empty_diff_does_not_modify_file() {
        let content = "[actions]\n\"actions/checkout\" = \"v4\"\n";
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();

        let diff = ManifestDiff::default();
        apply_manifest_diff(file.path(), &diff).unwrap();

        let after = fs::read_to_string(file.path()).unwrap();
        assert_eq!(content, after, "Empty diff must not modify file");
    }

    #[test]
    fn test_apply_add_one_action_preserves_existing() {
        let content = "[actions]\n\"actions/checkout\" = \"v4\"\n";
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();

        let diff = ManifestDiff {
            added: vec![(ActionId::from("actions/setup-node"), Version::from("v3"))],
            ..Default::default()
        };
        apply_manifest_diff(file.path(), &diff).unwrap();

        let after = fs::read_to_string(file.path()).unwrap();
        assert!(
            after.contains("\"actions/checkout\" = \"v4\""),
            "Existing entry must be preserved, got:\n{after}"
        );
        assert!(
            after.contains("\"actions/setup-node\" = \"v3\""),
            "New entry must be added, got:\n{after}"
        );

        // Round-trip: parse back
        let loaded = parse_manifest(file.path()).unwrap();
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
    fn test_apply_remove_one_action() {
        let content = "[actions]\n\"actions/checkout\" = \"v4\"\n\"actions/setup-node\" = \"v3\"\n";
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();

        let diff = ManifestDiff {
            removed: vec![ActionId::from("actions/checkout")],
            ..Default::default()
        };
        apply_manifest_diff(file.path(), &diff).unwrap();

        let after = fs::read_to_string(file.path()).unwrap();
        assert!(
            !after.contains("actions/checkout"),
            "Removed entry must be gone, got:\n{after}"
        );
        assert!(
            after.contains("\"actions/setup-node\" = \"v3\""),
            "Other entry must be preserved, got:\n{after}"
        );
    }

    #[test]
    fn test_apply_add_override_creates_section_if_missing() {
        let content = "[actions]\n\"actions/checkout\" = \"v4\"\n";
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();

        let diff = ManifestDiff {
            overrides_added: vec![(
                ActionId::from("actions/checkout"),
                ActionOverride {
                    workflow: ".github/workflows/deploy.yml".to_string(),
                    job: None,
                    step: None,
                    version: Version::from("v3"),
                },
            )],
            ..Default::default()
        };
        apply_manifest_diff(file.path(), &diff).unwrap();

        // Round-trip
        let loaded = parse_manifest(file.path()).unwrap();
        let overrides = loaded.overrides_for(&ActionId::from("actions/checkout"));
        assert_eq!(overrides.len(), 1);
        assert_eq!(overrides[0].workflow, ".github/workflows/deploy.yml");
        assert_eq!(overrides[0].version.as_str(), "v3");
    }

    #[test]
    fn test_apply_add_override_to_existing_section() {
        let content = r#"[actions]
"actions/checkout" = "v4"

[actions.overrides]
"actions/checkout" = [
  { workflow = ".github/workflows/deploy.yml", version = "v3" },
]
"#;
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();

        let diff = ManifestDiff {
            overrides_added: vec![(
                ActionId::from("actions/checkout"),
                ActionOverride {
                    workflow: ".github/workflows/ci.yml".to_string(),
                    job: Some("legacy".to_string()),
                    step: None,
                    version: Version::from("v2"),
                },
            )],
            ..Default::default()
        };
        apply_manifest_diff(file.path(), &diff).unwrap();

        let loaded = parse_manifest(file.path()).unwrap();
        let overrides = loaded.overrides_for(&ActionId::from("actions/checkout"));
        assert_eq!(overrides.len(), 2, "Should have 2 overrides now");
    }

    #[test]
    fn test_apply_remove_all_overrides_removes_action_entry() {
        let content = r#"[actions]
"actions/checkout" = "v4"

[actions.overrides]
"actions/checkout" = [
  { workflow = ".github/workflows/deploy.yml", version = "v3" },
]
"#;
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();

        let diff = ManifestDiff {
            overrides_removed: vec![(
                ActionId::from("actions/checkout"),
                vec![ActionOverride {
                    workflow: ".github/workflows/deploy.yml".to_string(),
                    job: None,
                    step: None,
                    version: Version::from("v3"),
                }],
            )],
            ..Default::default()
        };
        apply_manifest_diff(file.path(), &diff).unwrap();

        let loaded = parse_manifest(file.path()).unwrap();
        assert!(
            loaded
                .overrides_for(&ActionId::from("actions/checkout"))
                .is_empty()
        );
    }

    #[test]
    fn test_apply_remove_last_override_removes_section() {
        let content = r#"[actions]
"actions/checkout" = "v4"

[actions.overrides]
"actions/checkout" = [
  { workflow = ".github/workflows/deploy.yml", version = "v3" },
]
"#;
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();

        let diff = ManifestDiff {
            overrides_removed: vec![(
                ActionId::from("actions/checkout"),
                vec![ActionOverride {
                    workflow: ".github/workflows/deploy.yml".to_string(),
                    job: None,
                    step: None,
                    version: Version::from("v3"),
                }],
            )],
            ..Default::default()
        };
        apply_manifest_diff(file.path(), &diff).unwrap();

        let after = fs::read_to_string(file.path()).unwrap();
        assert!(
            !after.contains("overrides"),
            "Overrides section must be removed when empty, got:\n{after}"
        );
    }

    #[test]
    fn test_apply_roundtrip_domain_state_matches() {
        let content = r#"[actions]
"actions/checkout" = "v4"
"actions/setup-node" = "v3"
"#;
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();

        let diff = ManifestDiff {
            added: vec![(ActionId::from("actions/cache"), Version::from("v3"))],
            removed: vec![ActionId::from("actions/setup-node")],
            overrides_added: vec![(
                ActionId::from("actions/checkout"),
                ActionOverride {
                    workflow: ".github/workflows/windows.yml".to_string(),
                    job: None,
                    step: None,
                    version: Version::from("v3"),
                },
            )],
            ..Default::default()
        };
        apply_manifest_diff(file.path(), &diff).unwrap();

        let loaded = parse_manifest(file.path()).unwrap();
        assert_eq!(
            loaded.get(&ActionId::from("actions/checkout")),
            Some(&Version::from("v4"))
        );
        assert_eq!(
            loaded.get(&ActionId::from("actions/cache")),
            Some(&Version::from("v3"))
        );
        assert!(loaded.get(&ActionId::from("actions/setup-node")).is_none());
        let overrides = loaded.overrides_for(&ActionId::from("actions/checkout"));
        assert_eq!(overrides.len(), 1);
        assert_eq!(overrides[0].workflow, ".github/workflows/windows.yml");
    }

    // ========== Step 13: create_manifest tests ==========

    #[test]
    fn test_create_manifest_from_diff_with_3_actions() {
        let file = NamedTempFile::new().unwrap();

        let diff = ManifestDiff {
            added: vec![
                (ActionId::from("actions/checkout"), Version::from("v4")),
                (ActionId::from("actions/setup-node"), Version::from("v3")),
                (ActionId::from("actions/cache"), Version::from("v3")),
            ],
            ..Default::default()
        };
        create_manifest(file.path(), &diff).unwrap();

        let content = fs::read_to_string(file.path()).unwrap();
        assert!(content.contains("[actions]"));

        let loaded = parse_manifest(file.path()).unwrap();
        assert_eq!(
            loaded.get(&ActionId::from("actions/checkout")),
            Some(&Version::from("v4"))
        );
        assert_eq!(
            loaded.get(&ActionId::from("actions/setup-node")),
            Some(&Version::from("v3"))
        );
        assert_eq!(
            loaded.get(&ActionId::from("actions/cache")),
            Some(&Version::from("v3"))
        );
    }

    #[test]
    fn test_create_manifest_with_overrides() {
        let file = NamedTempFile::new().unwrap();

        let diff = ManifestDiff {
            added: vec![(ActionId::from("actions/checkout"), Version::from("v4"))],
            overrides_added: vec![(
                ActionId::from("actions/checkout"),
                ActionOverride {
                    workflow: ".github/workflows/windows.yml".to_string(),
                    job: None,
                    step: None,
                    version: Version::from("v3"),
                },
            )],
            ..Default::default()
        };
        create_manifest(file.path(), &diff).unwrap();

        let content = fs::read_to_string(file.path()).unwrap();
        assert!(content.contains("[actions]"));
        assert!(content.contains("[actions.overrides]"));

        let loaded = parse_manifest(file.path()).unwrap();
        assert_eq!(
            loaded.get(&ActionId::from("actions/checkout")),
            Some(&Version::from("v4"))
        );
        let overrides = loaded.overrides_for(&ActionId::from("actions/checkout"));
        assert_eq!(overrides.len(), 1);
        assert_eq!(overrides[0].workflow, ".github/workflows/windows.yml");
    }
}
