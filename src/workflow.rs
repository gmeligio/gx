use anyhow::{Context, Result};
use glob::glob;
use regex::Regex;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

pub struct WorkflowUpdater {
    workflows_dir: PathBuf,
}

pub struct UpdateResult {
    pub file: PathBuf,
    pub changes: Vec<String>,
}

impl WorkflowUpdater {
    pub fn new(repo_root: &Path) -> Self {
        Self {
            workflows_dir: repo_root.join(".github").join("workflows"),
        }
    }

    pub fn find_workflows(&self) -> Result<Vec<PathBuf>> {
        let mut workflows = Vec::new();

        for extension in &["yml", "yaml"] {
            let pattern = self
                .workflows_dir
                .join(format!("*.{}", extension))
                .to_string_lossy()
                .to_string();

            for entry in glob(&pattern).context("Failed to read glob pattern")? {
                match entry {
                    Ok(path) => workflows.push(path),
                    Err(e) => eprintln!("Warning: Error reading path: {}", e),
                }
            }
        }

        Ok(workflows)
    }

    pub fn update_workflow(
        &self,
        workflow_path: &Path,
        actions: &HashMap<String, String>,
    ) -> Result<UpdateResult> {
        let content = fs::read_to_string(workflow_path)
            .with_context(|| format!("Failed to read workflow: {}", workflow_path.display()))?;

        let mut updated_content = content.clone();
        let mut changes = Vec::new();

        for (action, version) in actions {
            let escaped_action = regex::escape(action);
            let pattern = format!(r"(uses:\s*{})@[^\s#]+", escaped_action);
            let re = Regex::new(&pattern)?;

            if re.is_match(&updated_content) {
                let replacement = format!("${{1}}@{}", version);
                let new_content = re.replace_all(&updated_content, replacement.as_str());

                if new_content != updated_content {
                    changes.push(format!("{}@{}", action, version));
                    updated_content = new_content.to_string();
                }
            }
        }

        if !changes.is_empty() {
            fs::write(workflow_path, &updated_content).with_context(|| {
                format!("Failed to write workflow: {}", workflow_path.display())
            })?;
        }

        Ok(UpdateResult {
            file: workflow_path.to_path_buf(),
            changes,
        })
    }

    pub fn update_all(&self, actions: &HashMap<String, String>) -> Result<Vec<UpdateResult>> {
        let workflows = self.find_workflows()?;
        let mut results = Vec::new();

        for workflow in workflows {
            let result = self.update_workflow(&workflow, actions)?;
            if !result.changes.is_empty() {
                results.push(result);
            }
        }

        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    fn create_test_workflow(dir: &Path, name: &str, content: &str) -> PathBuf {
        let workflows_dir = dir.join(".github").join("workflows");
        fs::create_dir_all(&workflows_dir).unwrap();
        let file_path = workflows_dir.join(name);
        let mut file = fs::File::create(&file_path).unwrap();
        file.write_all(content.as_bytes()).unwrap();
        file_path
    }

    #[test]
    fn test_find_workflows() {
        let temp_dir = TempDir::new().unwrap();
        create_test_workflow(temp_dir.path(), "ci.yml", "name: CI");
        create_test_workflow(temp_dir.path(), "deploy.yaml", "name: Deploy");

        let updater = WorkflowUpdater::new(temp_dir.path());
        let workflows = updater.find_workflows().unwrap();

        assert_eq!(workflows.len(), 2);
    }

    #[test]
    fn test_update_workflow() {
        let temp_dir = TempDir::new().unwrap();
        let content = r#"name: CI
on: push
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      - uses: actions/setup-node@v3
"#;
        let workflow_path = create_test_workflow(temp_dir.path(), "ci.yml", content);

        let updater = WorkflowUpdater::new(temp_dir.path());
        let mut actions = HashMap::new();
        actions.insert("actions/checkout".to_string(), "v4".to_string());

        let result = updater.update_workflow(&workflow_path, &actions).unwrap();

        assert_eq!(result.changes.len(), 1);
        assert!(result.changes[0].contains("actions/checkout@v4"));

        let updated = fs::read_to_string(&workflow_path).unwrap();
        assert!(updated.contains("actions/checkout@v4"));
        assert!(updated.contains("actions/setup-node@v3")); // unchanged
    }
}
