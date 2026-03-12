use super::Error as ManifestError;
use crate::config::Rule;
use crate::domain::action::identity::ActionId;
use crate::domain::action::spec::Spec as ActionSpec;
use crate::domain::action::specifier::Specifier;
use crate::domain::manifest::Manifest;
use crate::domain::manifest::overrides::ActionOverride;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::fmt::Write;
use std::path::Path;

// ---- TOML wire types ----

#[derive(Debug, Default, Deserialize, Serialize)]
pub(super) struct GxSection {
    #[serde(default)]
    pub(super) min_version: String,
}

#[derive(Debug, Default, Deserialize, Serialize)]
pub(super) struct TomlOverride {
    pub(super) workflow: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) job: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) step: Option<usize>,
    pub(super) version: String,
}

/// The [actions] section: flat string entries + optional [actions.overrides] sub-table.
#[derive(Debug, Default, Deserialize, Serialize)]
pub(super) struct TomlActions {
    #[serde(default, flatten)]
    pub(super) versions: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub(super) overrides: BTreeMap<String, Vec<TomlOverride>>,
}

#[derive(Debug, Default, Deserialize, Serialize)]
pub(super) struct ManifestData {
    #[serde(default)]
    pub(super) gx: Option<GxSection>,
    #[serde(default)]
    pub(super) actions: TomlActions,
    #[serde(default)]
    pub(super) lint: LintData,
}

/// The [lint] section of the manifest.
#[derive(Debug, Default, Deserialize, Serialize)]
pub(super) struct LintData {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub(super) rules: BTreeMap<String, Rule>,
}

// ---- conversion ----

pub(super) fn manifest_from_data(
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

            converted.push(ActionOverride {
                workflow: exc.workflow,
                job: exc.job,
                step: exc.step,
                version: specifier,
            });
        }
        overrides.insert(id, converted);
    }

    Ok(Manifest::with_overrides(actions, overrides))
}

pub(super) fn manifest_to_data(manifest: &Manifest) -> ManifestData {
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
        gx: Some(GxSection {
            min_version: env!("CARGO_PKG_VERSION").to_string(),
        }),
        actions: TomlActions {
            versions,
            overrides,
        },
        lint: LintData::default(),
    }
}

// ---- Formatting ----

/// Formats the manifest data as TOML with proper inline table syntax for overrides.
pub(super) fn format_manifest_toml(data: &ManifestData) -> String {
    let mut output = String::new();

    // Write [gx] section first if present
    if let Some(gx) = &data.gx {
        output.push_str("[gx]\n");
        writeln!(output, "min_version = \"{}\"", gx.min_version).ok();
        output.push('\n');
    }

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

#[cfg(test)]
mod tests {
    use super::Manifest;
    use crate::domain::action::identity::ActionId;
    use crate::domain::action::specifier::Specifier;
    use crate::infra::manifest::{Store, parse};
    use std::fs;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_file_manifest_save_and_load_roundtrip() {
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
    fn test_file_manifest_load_existing_toml() {
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
    fn test_file_manifest_save_sorts_actions_alphabetically() {
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
    fn test_save_writes_gx_section() {
        let file = NamedTempFile::new().unwrap();
        let store = Store::new(file.path());

        let mut manifest = Manifest::default();
        manifest.set(ActionId::from("actions/checkout"), Specifier::parse("^4"));
        store.save(&manifest).unwrap();

        let content = fs::read_to_string(file.path()).unwrap();
        assert!(
            content.contains("[gx]"),
            "Saved file must contain [gx] section, got:\n{content}"
        );
        assert!(
            content.contains("min_version"),
            "Saved file must contain min_version, got:\n{content}"
        );
        // [gx] section should appear before [actions]
        let gx_pos = content.find("[gx]").unwrap();
        let actions_pos = content.find("[actions]").unwrap();
        assert!(gx_pos < actions_pos, "[gx] must appear before [actions]");
    }
}
