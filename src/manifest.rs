use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

#[derive(Debug, Deserialize)]
pub struct Manifest {
    #[serde(default)]
    pub actions: HashMap<String, String>,
}

impl Manifest {
    pub fn load(path: &Path) -> Result<Self> {
        let content = fs::read_to_string(path)
            .with_context(|| format!("Failed to read manifest file: {}", path.display()))?;

        let manifest: Manifest = toml::from_str(&content)
            .with_context(|| format!("Failed to parse manifest file: {}", path.display()))?;

        Ok(manifest)
    }

    pub fn load_from_repo(repo_root: &Path) -> Result<Self> {
        let manifest_path = repo_root.join(".github").join("gx.toml");
        Self::load(&manifest_path)
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
}
