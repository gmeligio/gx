use super::Error as ManifestError;
use crate::config::Rule;
use crate::domain::action::identity::ActionId;
use crate::domain::action::spec::Spec as ActionSpec;
use crate::domain::action::specifier::Specifier;
use crate::domain::manifest::Manifest;
use crate::domain::manifest::overrides::ActionOverride;
use crate::domain::workflow_actions::{JobId, StepIndex, WorkflowPath};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use toml_edit::DocumentMut;

// ---- TOML wire types ----

/// Legacy [gx] section — only used for reading old manifests.
#[derive(Debug, Default, Deserialize, Serialize)]
pub struct GxSection {
    /// Minimum gx version required (legacy field).
    #[serde(default)]
    pub min_version: String,
}

/// A single override entry in the TOML manifest.
#[derive(Debug, Default, Deserialize, Serialize)]
pub struct TomlOverride {
    /// The workflow file path this override applies to.
    pub workflow: String,
    /// Optional job name to narrow the override scope.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub job: Option<String>,
    /// Optional step index to narrow the override scope.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step: Option<usize>,
    /// The version specifier for this override.
    pub version: String,
}

/// The [actions] section: flat string entries + optional [actions.overrides] sub-table.
#[derive(Debug, Default, Deserialize, Serialize)]
pub struct TomlActions {
    /// Flat map of action IDs to version specifier strings.
    #[serde(default, flatten)]
    pub versions: BTreeMap<String, String>,
    /// Per-action override lists keyed by action ID.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub overrides: BTreeMap<String, Vec<TomlOverride>>,
}

/// Top-level TOML structure for the manifest file.
#[derive(Debug, Default, Deserialize, Serialize)]
pub struct ManifestData {
    /// Legacy [gx] section (only present in old manifests).
    #[serde(default)]
    pub gx: Option<GxSection>,
    /// The [actions] section containing version pins and overrides.
    #[serde(default)]
    pub actions: TomlActions,
    /// The [lint] section containing rule configuration.
    #[serde(default)]
    pub lint: LintData,
}

/// The [lint] section of the manifest.
#[derive(Debug, Default, Deserialize, Serialize)]
pub struct LintData {
    /// Map of rule names to their configuration.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub rules: BTreeMap<crate::lint::RuleName, Rule>,
}

// ---- conversion ----

/// Convert deserialized manifest data into a domain `Manifest`.
pub fn manifest_from_data(
    data: ManifestData,
    _path: &Path,
    is_v2: bool,
) -> Result<Manifest, ManifestError> {
    // Build global actions map
    let actions: HashMap<ActionId, ActionSpec> = data
        .actions
        .versions
        .into_iter()
        .map(|(k, v)| {
            let id = ActionId::from(k);
            let specifier = if is_v2 {
                Specifier::parse(&v)
            } else {
                Specifier::from_v1(&v)
            };
            let spec = ActionSpec::new(id.clone(), specifier);
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

            let specifier = if is_v2 {
                Specifier::parse(&exc.version)
            } else {
                Specifier::from_v1(&exc.version)
            };

            let step_index = exc
                .step
                .map(StepIndex::try_from)
                .transpose()
                .map_err(ManifestError::Validation)?;

            converted.push(ActionOverride {
                workflow: WorkflowPath::new(exc.workflow),
                job: exc.job.map(JobId::from),
                step: step_index,
                version: specifier,
            });
        }
        overrides.insert(id, converted);
    }

    Ok(Manifest::with_overrides(actions, overrides))
}

// ---- Building ----

/// Build a `toml_edit::DocumentMut` from a `Manifest`.
/// Output has no `[gx]` section. Sections: `[actions]`, optional `[actions.overrides]`,
/// optional `[lint]`.
pub fn build_manifest_document(manifest: &Manifest) -> DocumentMut {
    let mut doc = DocumentMut::new();

    // Build [actions] table with sorted key-value pairs
    let mut actions = toml_edit::Table::new();
    let mut specs: Vec<_> = manifest.specs().collect();
    specs.sort_by_key(|s| s.id.as_str().to_owned());

    for spec in &specs {
        actions.insert(spec.id.as_str(), toml_edit::value(spec.version.as_str()));
    }

    // Build [actions.overrides] if any overrides exist
    let mut all_overrides: Vec<(&ActionId, &Vec<ActionOverride>)> =
        manifest.all_overrides().iter().collect();
    all_overrides.sort_by_key(|(id, _)| id.as_str().to_owned());

    let has_overrides = all_overrides.iter().any(|(_, ovrs)| !ovrs.is_empty());
    if has_overrides {
        let mut overrides_table = toml_edit::Table::new();

        for (id, ovrs) in &all_overrides {
            if ovrs.is_empty() {
                continue;
            }
            let mut arr = toml_edit::Array::new();
            for ovr in *ovrs {
                let mut inline = toml_edit::InlineTable::new();
                inline.insert("workflow", ovr.workflow.as_str().into());
                if let Some(job) = &ovr.job {
                    inline.insert("job", toml_edit::Value::from(job.as_str()));
                }
                if let Some(step) = ovr.step {
                    inline.insert("step", i64::from(step).into());
                }
                inline.insert("version", ovr.version.as_str().into());
                arr.push(inline);
            }
            overrides_table.insert(id.as_str(), toml_edit::value(arr));
        }
        actions.insert("overrides", toml_edit::Item::Table(overrides_table));
    }

    doc.insert("actions", toml_edit::Item::Table(actions));

    doc
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "tests use unwrap, indexing, and other patterns freely"
)]
mod tests {
    use super::{Manifest, build_manifest_document};
    use crate::domain::action::identity::ActionId;
    use crate::domain::action::specifier::Specifier;
    use crate::domain::manifest::overrides::ActionOverride;
    use crate::domain::workflow_actions::WorkflowPath;
    use crate::infra::manifest::{Store, parse};
    use std::fs;
    use std::io::Write as _;
    use tempfile::NamedTempFile;

    #[test]
    fn file_manifest_save_and_load_roundtrip() {
        let file = NamedTempFile::new().unwrap();
        let store = Store::new(file.path());

        let mut manifest = Manifest::default();
        manifest.set(ActionId::from("actions/checkout"), Specifier::parse("^4"));
        manifest.set(ActionId::from("actions/setup-node"), Specifier::parse("^3"));

        store.save(&manifest).unwrap();

        let loaded = parse(file.path()).unwrap();
        assert_eq!(
            loaded.value.get(&ActionId::from("actions/checkout")),
            Some(&Specifier::parse("^4"))
        );
        assert_eq!(
            loaded.value.get(&ActionId::from("actions/setup-node")),
            Some(&Specifier::parse("^3"))
        );
    }

    #[test]
    fn file_manifest_load_existing_toml() {
        // v1 format (no [gx] section) — values like "v4" get converted via from_v1
        let content = r#"
[actions]
"actions/checkout" = "v4"
"actions/setup-node" = "v4"
"#;
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();

        let loaded = parse(file.path()).unwrap();
        assert_eq!(
            loaded.value.get(&ActionId::from("actions/checkout")),
            Some(&Specifier::from_v1("v4"))
        );
    }

    #[test]
    fn file_manifest_save_sorts_actions_alphabetically() {
        let file = NamedTempFile::new().unwrap();
        let store = Store::new(file.path());

        let mut manifest = Manifest::default();
        manifest.set(
            ActionId::from("docker/build-push-action"),
            Specifier::parse("^5"),
        );
        manifest.set(ActionId::from("actions/checkout"), Specifier::parse("^4"));
        manifest.set(
            ActionId::from("actions-rust-lang/rustfmt"),
            Specifier::parse("^1"),
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
    fn save_no_gx_section() {
        let file = NamedTempFile::new().unwrap();
        let store = Store::new(file.path());

        let mut manifest = Manifest::default();
        manifest.set(ActionId::from("actions/checkout"), Specifier::parse("^4"));
        store.save(&manifest).unwrap();

        let content = fs::read_to_string(file.path()).unwrap();
        assert!(
            !content.contains("[gx]"),
            "Saved file must NOT contain [gx] section, got:\n{content}"
        );
        assert!(
            !content.contains("min_version"),
            "Saved file must NOT contain min_version, got:\n{content}"
        );
        assert!(
            content.contains("[actions]"),
            "Saved file must contain [actions] section, got:\n{content}"
        );
    }

    #[test]
    fn build_manifest_document_with_overrides() {
        let mut manifest = Manifest::default();
        manifest.set(ActionId::from("actions/checkout"), Specifier::parse("^4"));
        manifest.add_override(
            ActionId::from("actions/checkout"),
            ActionOverride {
                workflow: WorkflowPath::new(".github/workflows/ci.yml"),
                job: None,
                step: None,
                version: Specifier::parse("^3"),
            },
        );

        let output = build_manifest_document(&manifest).to_string();

        assert!(output.contains("[actions]"));
        assert!(output.contains("[actions.overrides]"));
        assert!(output.contains("\"actions/checkout\" = \"^4\""));
        assert!(!output.contains("[gx]"));
    }
}
