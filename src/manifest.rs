use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

#[derive(Debug)]
pub struct ManifestPathNotInitialized;

impl std::fmt::Display for ManifestPathNotInitialized {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Manifest path not initialized. Use load_from_repo or load to create a manifest with a path."
        )
    }
}

impl std::error::Error for ManifestPathNotInitialized {}

/// The main manifest structure mapping actions to versions
#[derive(Debug, Deserialize, Serialize)]
pub struct Manifest {
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub actions: HashMap<String, String>,
    #[serde(skip)]
    path: Option<std::path::PathBuf>,
}

impl Manifest {
    pub fn path(&self) -> Result<&Path> {
        self.path
            .as_ref()
            .map(|p| p.as_path())
            .ok_or_else(|| anyhow!(ManifestPathNotInitialized))
    }

    pub fn load(path: &Path) -> Result<Self> {
        let content = fs::read_to_string(path)
            .with_context(|| format!("Failed to read manifest file: {}", path.display()))?;

        let mut manifest: Manifest = toml::from_str(&content)
            .with_context(|| format!("Failed to parse manifest file: {}", path.display()))?;

        manifest.path = Some(path.to_path_buf());

        Ok(manifest)
    }

    pub fn load_from_repo(repo_root: &Path) -> Result<Self> {
        const MANIFEST_FILE_NAME: &str = "gx.toml";
        let manifest_path = repo_root.join(".github").join(MANIFEST_FILE_NAME);
        Self::load(&manifest_path)
    }

    pub fn load_or_default(path: &Path) -> Result<Self> {
        if path.exists() {
            Self::load(path)
        } else {
            let mut manifest = Self::default();
            manifest.path = Some(path.to_path_buf());
            Ok(manifest)
        }
    }

    pub fn load_from_repo_or_default(repo_root: &Path) -> Result<Self> {
        const MANIFEST_FILE_NAME: &str = "gx.toml";
        let manifest_path = repo_root.join(".github").join(MANIFEST_FILE_NAME);
        Self::load_or_default(&manifest_path)
    }

    pub fn save(&self) -> Result<()> {
        let path = self.path()?;
        let content =
            toml::to_string_pretty(self).context("Failed to serialize manifest to TOML")?;

        fs::write(path, content)
            .with_context(|| format!("Failed to write manifest file: {}", path.display()))?;

        println!("\nManifest updated: {}", path.display());
        Ok(())
    }
}

impl Default for Manifest {
    fn default() -> Self {
        Self {
            actions: HashMap::new(),
            path: None,
        }
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

        let manifest = Manifest::load(file.path()).unwrap();

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

        let manifest = Manifest::load(file.path()).unwrap();
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

        let manifest = Manifest::load_or_default(file.path()).unwrap();
        assert_eq!(
            manifest.actions.get("actions/checkout"),
            Some(&"v4".to_string())
        );
    }

    #[test]
    fn test_load_or_default_missing() {
        let manifest = Manifest::load_or_default(Path::new("/nonexistent/path/gx.toml")).unwrap();
        assert!(manifest.actions.is_empty());
    }

    #[test]
    fn test_save_and_load() {
        let mut manifest = Manifest::default();
        manifest
            .actions
            .insert("actions/checkout".to_string(), "v4".to_string());
        manifest
            .actions
            .insert("actions/setup-node".to_string(), "v3".to_string());

        let file = NamedTempFile::new().unwrap();
        manifest.path = Some(file.path().to_path_buf());
        manifest.save().unwrap();

        let loaded = Manifest::load(file.path()).unwrap();
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
        let manifest = Manifest::default();
        let result = manifest.path();
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("Manifest path not initialized"));
    }

    #[test]
    fn test_save_without_path_fails() {
        let manifest = Manifest::default();
        let result = manifest.save();
        assert!(result.is_err());
    }
}
