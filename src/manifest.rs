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

/// Step-level override for a specific step index
#[derive(Debug, Default, Clone, Deserialize, Serialize, PartialEq)]
pub struct StepOverride {
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub actions: HashMap<String, String>,
}

/// Job-level configuration with optional step overrides
/// Steps are keyed by string index (e.g., "0", "1") for TOML compatibility
#[derive(Debug, Default, Clone, Deserialize, Serialize, PartialEq)]
pub struct JobOverride {
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub actions: HashMap<String, String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub steps: HashMap<String, StepOverride>,
}

/// Workflow-level configuration with optional job overrides
#[derive(Debug, Default, Clone, Deserialize, Serialize, PartialEq)]
pub struct WorkflowOverride {
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub actions: HashMap<String, String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub jobs: HashMap<String, JobOverride>,
}

/// The main manifest structure with global actions and workflow overrides
#[derive(Debug, Deserialize, Serialize)]
pub struct Manifest {
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub actions: HashMap<String, String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub workflows: HashMap<String, WorkflowOverride>,
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

    pub fn merge(&mut self, other: &HashMap<String, String>) {
        for (action, version) in other {
            // Only add if not already present (existing entries take precedence)
            self.actions
                .entry(action.clone())
                .or_insert_with(|| version.clone());
        }
    }

    /// Check if the manifest has any workflow-level overrides
    pub fn has_overrides(&self) -> bool {
        !self.workflows.is_empty()
    }

    /// Get workflow override or create default
    pub fn workflow_mut(&mut self, workflow: &str) -> &mut WorkflowOverride {
        self.workflows
            .entry(workflow.to_string())
            .or_insert_with(WorkflowOverride::default)
    }

    /// Get job override within a workflow, or create default
    pub fn job_mut(&mut self, workflow: &str, job: &str) -> &mut JobOverride {
        self.workflow_mut(workflow)
            .jobs
            .entry(job.to_string())
            .or_insert_with(JobOverride::default)
    }

    /// Get step override within a job, or create default
    pub fn step_mut(&mut self, workflow: &str, job: &str, step: usize) -> &mut StepOverride {
        self.job_mut(workflow, job)
            .steps
            .entry(step.to_string())
            .or_insert_with(StepOverride::default)
    }
}

impl Default for Manifest {
    fn default() -> Self {
        Self {
            actions: HashMap::new(),
            workflows: HashMap::new(),
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
    fn test_merge_new_actions() {
        let mut manifest = Manifest::default();
        manifest
            .actions
            .insert("actions/checkout".to_string(), "v4".to_string());

        let mut new_actions = HashMap::new();
        new_actions.insert("actions/setup-node".to_string(), "v3".to_string());

        manifest.merge(&new_actions);

        assert_eq!(manifest.actions.len(), 2);
        assert_eq!(
            manifest.actions.get("actions/setup-node"),
            Some(&"v3".to_string())
        );
    }

    #[test]
    fn test_merge_existing_preserved() {
        let mut manifest = Manifest::default();
        manifest
            .actions
            .insert("actions/checkout".to_string(), "v4".to_string());

        let mut new_actions = HashMap::new();
        new_actions.insert("actions/checkout".to_string(), "v3".to_string()); // different version

        manifest.merge(&new_actions);

        // Existing entry should be preserved
        assert_eq!(
            manifest.actions.get("actions/checkout"),
            Some(&"v4".to_string())
        );
    }

    #[test]
    fn test_parse_manifest_with_workflow_overrides() {
        let content = r#"
[actions]
"actions/checkout" = "v4"

[workflows."ci.yml".actions]
"actions/checkout" = "v3"
"#;

        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();

        let manifest = Manifest::load(file.path()).unwrap();

        assert_eq!(
            manifest.actions.get("actions/checkout"),
            Some(&"v4".to_string())
        );
        assert_eq!(
            manifest
                .workflows
                .get("ci.yml")
                .unwrap()
                .actions
                .get("actions/checkout"),
            Some(&"v3".to_string())
        );
    }

    #[test]
    fn test_parse_manifest_with_job_overrides() {
        let content = r#"
[actions]
"actions/checkout" = "v4"

[workflows."ci.yml".jobs."test".actions]
"actions/checkout" = "v2"
"#;

        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();

        let manifest = Manifest::load(file.path()).unwrap();

        assert_eq!(
            manifest
                .workflows
                .get("ci.yml")
                .unwrap()
                .jobs
                .get("test")
                .unwrap()
                .actions
                .get("actions/checkout"),
            Some(&"v2".to_string())
        );
    }

    #[test]
    fn test_parse_manifest_with_step_overrides() {
        let content = r#"
[actions]
"actions/checkout" = "v4"

[workflows."ci.yml".jobs."test".steps."0".actions]
"actions/checkout" = "v1"
"#;

        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();

        let manifest = Manifest::load(file.path()).unwrap();

        assert_eq!(
            manifest
                .workflows
                .get("ci.yml")
                .unwrap()
                .jobs
                .get("test")
                .unwrap()
                .steps
                .get("0")
                .unwrap()
                .actions
                .get("actions/checkout"),
            Some(&"v1".to_string())
        );
    }

    #[test]
    fn test_workflow_mut_creates_entry() {
        let mut manifest = Manifest::default();
        manifest
            .workflow_mut("ci.yml")
            .actions
            .insert("actions/checkout".to_string(), "v3".to_string());

        assert!(manifest.workflows.contains_key("ci.yml"));
        assert_eq!(
            manifest
                .workflows
                .get("ci.yml")
                .unwrap()
                .actions
                .get("actions/checkout"),
            Some(&"v3".to_string())
        );
    }

    #[test]
    fn test_job_mut_creates_entry() {
        let mut manifest = Manifest::default();
        manifest
            .job_mut("ci.yml", "build")
            .actions
            .insert("actions/checkout".to_string(), "v2".to_string());

        assert!(
            manifest
                .workflows
                .get("ci.yml")
                .unwrap()
                .jobs
                .contains_key("build")
        );
    }

    #[test]
    fn test_step_mut_creates_entry() {
        let mut manifest = Manifest::default();
        manifest
            .step_mut("ci.yml", "build", 0)
            .actions
            .insert("actions/checkout".to_string(), "v1".to_string());

        assert!(
            manifest
                .workflows
                .get("ci.yml")
                .unwrap()
                .jobs
                .get("build")
                .unwrap()
                .steps
                .contains_key("0")
        );
    }

    #[test]
    fn test_save_and_load_with_overrides() {
        let mut manifest = Manifest::default();
        manifest
            .actions
            .insert("actions/checkout".to_string(), "v4".to_string());
        manifest
            .workflow_mut("ci.yml")
            .actions
            .insert("actions/checkout".to_string(), "v3".to_string());

        let file = NamedTempFile::new().unwrap();
        manifest.path = Some(file.path().to_path_buf());
        manifest.save().unwrap();

        let loaded = Manifest::load(file.path()).unwrap();
        assert_eq!(
            loaded.actions.get("actions/checkout"),
            Some(&"v4".to_string())
        );
        assert_eq!(
            loaded
                .workflows
                .get("ci.yml")
                .unwrap()
                .actions
                .get("actions/checkout"),
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
