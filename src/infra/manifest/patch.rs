use super::Error as ManifestError;
use crate::domain::action::identity::ActionId;
use crate::domain::manifest::overrides::ActionOverride;
use crate::domain::plan::ManifestDiff;
use std::fs;
use std::path::Path;
use toml_edit::DocumentMut;

/// Apply a `ManifestDiff` to an existing manifest file using `toml_edit` for surgical patching.
///
/// The file must already exist. For creating a new manifest from scratch, use `create`.
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

    // Remove [gx] section if present (migration from old format)
    doc.remove("gx");

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
        if overrides_table.get(ActionId::as_str(id)).is_none() {
            overrides_table.insert(id.as_str(), toml_edit::value(toml_edit::Array::new()));
        }
        let arr = overrides_table[id.as_str()]
            .as_array_mut()
            .expect("override entry is always an array");

        // Build the inline table for this override entry
        let mut inline = toml_edit::InlineTable::new();
        inline.insert("workflow", ovr.workflow.as_str().into());
        if let Some(job) = &ovr.job {
            inline.insert("job", toml_edit::Value::from(job.as_str()));
        }
        if let Some(step) = ovr.step {
            inline.insert(
                "step",
                i64::try_from(step).expect("step index overflow").into(),
            );
        }
        inline.insert("version", ovr.version.as_str().into());

        arr.push(inline);
    }
    overrides_table.sort_values();
}

#[cfg(test)]
mod tests {
    use super::apply_manifest_diff;
    use crate::domain::action::identity::ActionId;
    use crate::domain::action::specifier::Specifier;
    use crate::domain::manifest::overrides::ActionOverride;
    use crate::domain::plan::ManifestDiff;
    use std::fs;
    use std::io::Write;
    use tempfile::NamedTempFile;

    use crate::infra::manifest::parse;

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
        let content = "[actions]\n\"actions/checkout\" = \"^4\"\n";
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();

        let diff = ManifestDiff {
            added: vec![(ActionId::from("actions/setup-node"), Specifier::parse("^3"))],
            ..Default::default()
        };
        apply_manifest_diff(file.path(), &diff).unwrap();

        let after = fs::read_to_string(file.path()).unwrap();
        assert!(
            after.contains("\"actions/checkout\" = \"^4\""),
            "Existing entry must be preserved, got:\n{after}"
        );
        assert!(
            after.contains("\"actions/setup-node\" = \"^3\""),
            "New entry must be added, got:\n{after}"
        );

        // Round-trip
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
        let content = "[actions]\n\"actions/checkout\" = \"^4\"\n";
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();

        let diff = ManifestDiff {
            overrides_added: vec![(
                ActionId::from("actions/checkout"),
                ActionOverride {
                    workflow: ".github/workflows/deploy.yml".to_string(),
                    job: None,
                    step: None,
                    version: Specifier::parse("^3"),
                },
            )],
            ..Default::default()
        };
        apply_manifest_diff(file.path(), &diff).unwrap();

        // Round-trip (v1 format since no [gx] section — "^4" parsed via from_v1 yields Ref("^4") but that's fine)
        let loaded = parse(file.path()).unwrap();
        let overrides = loaded
            .value
            .overrides_for(&ActionId::from("actions/checkout"));
        assert_eq!(overrides.len(), 1);
        assert_eq!(overrides[0].workflow, ".github/workflows/deploy.yml");
        assert_eq!(overrides[0].version.as_str(), "^3");
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
                    version: Specifier::parse("^2"),
                },
            )],
            ..Default::default()
        };
        apply_manifest_diff(file.path(), &diff).unwrap();

        let loaded = parse(file.path()).unwrap();
        let overrides = loaded
            .value
            .overrides_for(&ActionId::from("actions/checkout"));
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
                    version: Specifier::parse("^3"),
                }],
            )],
            ..Default::default()
        };
        apply_manifest_diff(file.path(), &diff).unwrap();

        let loaded = parse(file.path()).unwrap();
        assert!(
            loaded
                .value
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
                    version: Specifier::parse("^3"),
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
"actions/checkout" = "^4"
"actions/setup-node" = "^3"
"#;
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();

        let diff = ManifestDiff {
            added: vec![(ActionId::from("actions/cache"), Specifier::parse("^3"))],
            removed: vec![ActionId::from("actions/setup-node")],
            overrides_added: vec![(
                ActionId::from("actions/checkout"),
                ActionOverride {
                    workflow: ".github/workflows/windows.yml".to_string(),
                    job: None,
                    step: None,
                    version: Specifier::parse("^3"),
                },
            )],
            ..Default::default()
        };
        apply_manifest_diff(file.path(), &diff).unwrap();

        let loaded = parse(file.path()).unwrap();
        assert!(
            loaded
                .value
                .get(&ActionId::from("actions/checkout"))
                .is_some()
        );
        assert!(loaded.value.get(&ActionId::from("actions/cache")).is_some());
        assert!(
            loaded
                .value
                .get(&ActionId::from("actions/setup-node"))
                .is_none()
        );
        let overrides = loaded
            .value
            .overrides_for(&ActionId::from("actions/checkout"));
        assert_eq!(overrides.len(), 1);
        assert_eq!(overrides[0].workflow, ".github/workflows/windows.yml");
    }
}
