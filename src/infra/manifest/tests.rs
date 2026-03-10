use super::{
    FileManifest, Manifest, ManifestError, create_manifest, parse_lint_config, parse_manifest,
};
use crate::domain::{ActionId, ActionOverride, ManifestDiff, Specifier, Version};
use std::fs;
use std::io::Write;
use std::path::Path;
use tempfile::NamedTempFile;

#[test]
fn test_parse_manifest_missing_returns_empty() {
    let result = parse_manifest(Path::new("/nonexistent/gx.toml")).unwrap();
    assert!(result.value.is_empty());
    assert!(!result.migrated);
}

#[test]
fn test_parse_manifest_reads_file() {
    // v1 format
    let content = "[actions]\n\"actions/checkout\" = \"v4\"\n";
    let mut file = NamedTempFile::new().unwrap();
    file.write_all(content.as_bytes()).unwrap();
    let loaded = parse_manifest(file.path()).unwrap();
    assert_eq!(
        loaded.value.get(&ActionId::from("actions/checkout")),
        Some(&Specifier::from_v1("v4"))
    );
}

#[test]
fn test_load_manifest_with_overrides() {
    // v1 format — values like "v4", "v3", "v2" converted via from_v1
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

    let loaded = parse_manifest(file.path()).unwrap();

    assert_eq!(
        loaded.value.get(&ActionId::from("actions/checkout")),
        Some(&Specifier::from_v1("v4"))
    );

    let overrides = loaded
        .value
        .overrides_for(&ActionId::from("actions/checkout"));
    assert_eq!(overrides.len(), 2);
    assert_eq!(overrides[0].workflow, ".github/workflows/deploy.yml");
    assert_eq!(overrides[0].version.as_str(), "^3");
    assert_eq!(overrides[1].job.as_deref(), Some("legacy-build"));
    assert_eq!(overrides[1].version.as_str(), "^2");
}

#[test]
fn test_save_and_load_roundtrip_with_overrides() {
    let file = NamedTempFile::new().unwrap();
    let store = FileManifest::new(file.path());

    let mut manifest = Manifest::default();
    manifest.set(ActionId::from("actions/checkout"), Specifier::parse("^4"));
    manifest.add_override(
        ActionId::from("actions/checkout"),
        ActionOverride {
            workflow: ".github/workflows/deploy.yml".to_string(),
            job: None,
            step: None,
            version: Specifier::parse("^3"),
        },
    );

    store.save(&manifest).unwrap();
    let content = fs::read_to_string(file.path()).unwrap();
    assert!(
        content.contains("actions.overrides"),
        "Expected overrides section, got:\n{content}"
    );

    let loaded = parse_manifest(file.path()).unwrap();
    let overrides = loaded
        .value
        .overrides_for(&ActionId::from("actions/checkout"));
    assert_eq!(overrides.len(), 1);
    assert_eq!(overrides[0].workflow, ".github/workflows/deploy.yml");
    assert_eq!(overrides[0].version.as_str(), "^3");
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
    manifest.set(ActionId::from("actions/checkout"), Specifier::parse("^4"));

    store.save(&manifest).unwrap();
    let content = fs::read_to_string(file.path()).unwrap();
    assert!(!content.contains("overrides"), "got:\n{content}");
}

#[test]
fn test_save_and_load_roundtrip_generates_correct_toml_format() {
    let file = NamedTempFile::new().unwrap();
    let store = FileManifest::new(file.path());

    let mut manifest = Manifest::default();
    manifest.set(ActionId::from("actions/checkout"), Specifier::parse("^4"));
    manifest.add_override(
        ActionId::from("actions/checkout"),
        ActionOverride {
            workflow: ".github/workflows/windows.yml".to_string(),
            job: Some("test_windows".to_string()),
            step: Some(0),
            version: Specifier::parse("^5"),
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
    let overrides = loaded
        .value
        .overrides_for(&ActionId::from("actions/checkout"));
    assert_eq!(overrides.len(), 1);
    assert_eq!(overrides[0].workflow, ".github/workflows/windows.yml");
    assert_eq!(overrides[0].job.as_deref(), Some("test_windows"));
    assert_eq!(overrides[0].step, Some(0));
    assert_eq!(overrides[0].version.as_str(), "^5");
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

// ========== Step 13: create_manifest tests ==========

#[test]
fn test_create_manifest_from_diff_with_3_actions() {
    let file = NamedTempFile::new().unwrap();

    let diff = ManifestDiff {
        added: vec![
            (ActionId::from("actions/checkout"), Specifier::parse("^4")),
            (ActionId::from("actions/setup-node"), Specifier::parse("^3")),
            (ActionId::from("actions/cache"), Specifier::parse("^3")),
        ],
        ..Default::default()
    };
    create_manifest(file.path(), &diff).unwrap();

    let content = fs::read_to_string(file.path()).unwrap();
    assert!(content.contains("[actions]"));

    let loaded = parse_manifest(file.path()).unwrap();
    // create_manifest writes v2 format (with [gx] section), so values are parsed as v2
    assert_eq!(
        loaded.value.get(&ActionId::from("actions/checkout")),
        Some(&Specifier::parse("^4"))
    );
    assert_eq!(
        loaded.value.get(&ActionId::from("actions/setup-node")),
        Some(&Specifier::parse("^3"))
    );
    assert_eq!(
        loaded.value.get(&ActionId::from("actions/cache")),
        Some(&Specifier::parse("^3"))
    );
}

#[test]
fn test_create_manifest_with_overrides() {
    let file = NamedTempFile::new().unwrap();

    let diff = ManifestDiff {
        added: vec![(ActionId::from("actions/checkout"), Specifier::parse("^4"))],
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
    create_manifest(file.path(), &diff).unwrap();

    let content = fs::read_to_string(file.path()).unwrap();
    assert!(content.contains("[actions]"));
    assert!(content.contains("[actions.overrides]"));

    let loaded = parse_manifest(file.path()).unwrap();
    assert_eq!(
        loaded.value.get(&ActionId::from("actions/checkout")),
        Some(&Specifier::parse("^4"))
    );
    let overrides = loaded
        .value
        .overrides_for(&ActionId::from("actions/checkout"));
    assert_eq!(overrides.len(), 1);
    assert_eq!(overrides[0].workflow, ".github/workflows/windows.yml");
}

// ========== Phase 4.2: v1 migration and version guard tests ==========

#[test]
fn test_v1_to_v2_migration_sets_migrated_flag() {
    // v1 format: no [gx] section, values like "v4" style
    let content = r#"
[actions]
"actions/checkout" = "v4"
"actions/setup-node" = "v3"
"#;
    let mut file = NamedTempFile::new().unwrap();
    file.write_all(content.as_bytes()).unwrap();

    let parsed = parse_manifest(file.path()).unwrap();
    assert!(parsed.migrated, "v1 format should set migrated = true");
    // from_v1("v4") = ^4
    assert_eq!(
        parsed.value.get(&ActionId::from("actions/checkout")),
        Some(&Specifier::from_v1("v4"))
    );
    assert_eq!(
        Specifier::from_v1("v4"),
        Specifier::parse("^4"),
        "from_v1 v4 should equal ^4"
    );
}

#[test]
fn test_v2_format_not_migrated() {
    // v2 format: has [gx] section
    let content = format!(
        r#"
[gx]
min_version = "{}"

[actions]
"actions/checkout" = "^4"
"#,
        env!("CARGO_PKG_VERSION")
    );
    let mut file = NamedTempFile::new().unwrap();
    file.write_all(content.as_bytes()).unwrap();

    let parsed = parse_manifest(file.path()).unwrap();
    assert!(!parsed.migrated, "v2 format should not set migrated");
    assert_eq!(
        parsed.value.get(&ActionId::from("actions/checkout")),
        Some(&Specifier::parse("^4"))
    );
}

#[test]
fn test_version_guard_returns_error_when_version_too_old() {
    let content = r#"
[gx]
min_version = "99.0.0"

[actions]
"actions/checkout" = "^4"
"#;
    let mut file = NamedTempFile::new().unwrap();
    file.write_all(content.as_bytes()).unwrap();

    let result = parse_manifest(file.path());
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        matches!(err, ManifestError::VersionRequired { .. }),
        "Expected VersionRequired error, got: {err}"
    );
    assert!(
        err.to_string().contains("99.0.0"),
        "Error should mention required version, got: {err}"
    );
}

#[test]
fn test_version_guard_passes_when_version_sufficient() {
    // Use the current binary version — should always pass
    let content = format!(
        r#"
[gx]
min_version = "{}"

[actions]
"actions/checkout" = "^4"
"#,
        env!("CARGO_PKG_VERSION")
    );
    let mut file = NamedTempFile::new().unwrap();
    file.write_all(content.as_bytes()).unwrap();

    let result = parse_manifest(file.path());
    assert!(result.is_ok(), "Should not error when version matches");
}

// Keep Version import used only in tests that still use the old Version type
// (none remain in this file, but the import is present for compatibility)
#[allow(dead_code)]
fn _use_version() -> Version {
    Version::from("v4")
}
