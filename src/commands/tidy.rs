use anyhow::Result;
use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::config::Config;
use crate::github::GitHubClient;
use crate::lock::LockFile;
use crate::manifest::Manifest;
use crate::version::find_highest_version;
use crate::workflow::{ExtractedAction, UpdateResult, WorkflowUpdater};

/// Groups action versions across all workflows
#[derive(Debug, Default)]
struct ActionVersions {
    /// Maps action name to set of versions found in workflows
    versions: HashMap<String, HashSet<String>>,
}

impl ActionVersions {
    fn add(&mut self, action: &ExtractedAction) {
        self.versions
            .entry(action.name.clone())
            .or_default()
            .insert(action.version.clone());
    }

    fn unique_versions(&self, action: &str) -> Vec<String> {
        self.versions
            .get(action)
            .map(|v| v.iter().cloned().collect())
            .unwrap_or_default()
    }

    fn action_names(&self) -> Vec<String> {
        self.versions.keys().cloned().collect()
    }
}

pub fn run(repo_root: &Path) -> Result<()> {
    let config = Config::from_env();
    let updater = WorkflowUpdater::new(repo_root);

    println!("Scanning workflows...");

    let workflows = updater.find_workflows()?;
    if workflows.is_empty() {
        println!("No workflows found in .github/workflows/");
        return Ok(());
    }

    for workflow in &workflows {
        println!("  {}", workflow.display());
    }

    let extracted = updater.extract_all()?;

    // Collect versions for each action
    let mut action_versions = ActionVersions::default();
    for action in &extracted {
        action_versions.add(action);
    }

    let workflow_actions: HashSet<String> = action_versions.action_names().into_iter().collect();

    // Load current manifest and lock file
    let mut manifest = Manifest::load_from_repo_or_default(repo_root)?;
    let mut lock = LockFile::load_from_repo_or_default(repo_root)?;

    let manifest_actions: HashSet<String> = manifest.actions.keys().cloned().collect();

    // Find differences
    let missing: Vec<_> = workflow_actions.difference(&manifest_actions).collect();
    let unused: Vec<_> = manifest_actions.difference(&workflow_actions).collect();

    let mut manifest_changed = false;

    // Remove unused actions from manifest
    if !unused.is_empty() {
        println!("\nRemoving unused actions from manifest:");
        for action in &unused {
            println!("  - {}", action);
            manifest.actions.remove(*action);
        }
        manifest_changed = true;
    }

    // Add missing actions to manifest (using highest version if multiple exist)
    if !missing.is_empty() {
        println!("\nAdding missing actions to manifest:");
        for action_name in &missing {
            let versions = action_versions.unique_versions(action_name);
            let version = select_version(&versions);
            manifest
                .actions
                .insert((*action_name).clone(), version.clone());
            println!("  + {}@{}", action_name, version);
        }
        manifest_changed = true;
    }

    // Update existing actions only if manifest has SHA but workflow has tag
    // (This happens when upgrading from SHA to semantic version via comment)
    let existing: Vec<_> = workflow_actions.intersection(&manifest_actions).collect();
    if !existing.is_empty() {
        let mut updated_actions = Vec::new();

        for action_name in &existing {
            let versions = action_versions.unique_versions(action_name);

            if versions.len() == 1 {
                let workflow_version = &versions[0];
                let manifest_version = manifest.actions.get(*action_name).unwrap().clone();

                // Only update if:
                // 1. Versions differ, AND
                // 2. Manifest has a SHA (40 hex chars) and workflow has a semantic version
                let manifest_is_sha = manifest_version.len() >= 40
                    && manifest_version.chars().all(|c| c.is_ascii_hexdigit());
                let workflow_is_semver = workflow_version.starts_with('v')
                    || workflow_version
                        .chars()
                        .next()
                        .map(|c| c.is_ascii_digit())
                        .unwrap_or(false);

                if workflow_version != &manifest_version && manifest_is_sha && workflow_is_semver {
                    manifest
                        .actions
                        .insert((*action_name).clone(), workflow_version.clone());
                    updated_actions.push(format!(
                        "{}@{} (was {})",
                        action_name, workflow_version, manifest_version
                    ));
                    manifest_changed = true;
                }
            }
        }

        if !updated_actions.is_empty() {
            println!("\nUpdating action versions in manifest:");
            for update in &updated_actions {
                println!("  ~ {}", update);
            }
        }
    }

    // Save manifest if changed
    if manifest_changed {
        manifest.save()?;
    } else if missing.is_empty() && unused.is_empty() {
        println!("\nManifest is already in sync with workflows.");
    }

    // Update lock file with resolved commit SHAs
    update_lock_file(&mut lock, &manifest, &config)?;

    // Remove unused entries from lock file
    lock.remove_unused(&manifest.actions);

    // Save lock file
    lock.save_to_repo(repo_root)?;

    // Apply manifest versions to workflows using SHAs from lock file
    if manifest.actions.is_empty() {
        println!("\nNo actions defined in {}", manifest.path()?.display());
        return Ok(());
    }

    // Build update map with SHAs from lock file and version comments from manifest
    let update_map = lock.build_update_map(&manifest.actions);
    let results = updater.update_all(&update_map)?;
    print_update_results(&results);

    Ok(())
}

/// Select the best version from a list of versions.
/// Prefers the highest semantic version if available.
fn select_version(versions: &[String]) -> String {
    let version_refs: Vec<&str> = versions.iter().map(|s| s.as_str()).collect();
    find_highest_version(&version_refs)
        .unwrap_or(&versions[0])
        .to_string()
}

fn update_lock_file(lock: &mut LockFile, manifest: &Manifest, config: &Config) -> Result<()> {
    let github = GitHubClient::new(config.github_token.clone())?;

    if !config.has_github_token() {
        println!("  Note: GITHUB_TOKEN not set. Skipping SHA resolution for new actions.");
        println!("  Set GITHUB_TOKEN to resolve version tags to commit SHAs.");
        return Ok(());
    }

    // Resolve each action@version to commit SHA
    for (action, version) in &manifest.actions {
        // Skip if already in lock file
        if lock.has(action, version) {
            continue;
        }

        // Resolve via GitHub API
        println!("  Resolving {}@{} ...", action, version);
        match github.resolve_ref(action, version) {
            Ok(sha) => {
                lock.set(action, version, sha);
            }
            Err(e) => {
                eprintln!("  Warning: Failed to resolve {}@{}: {}", action, version, e);
                eprintln!("  Skipping lock for this action.");
            }
        }
    }

    Ok(())
}

fn print_update_results(results: &[UpdateResult]) {
    if results.is_empty() {
        println!("\nWorkflows are already up to date.");
    } else {
        println!("\nUpdated workflows:");
        for result in results {
            println!("  {}", result.file.display());
            for change in &result.changes {
                println!("    - {}", change);
            }
        }
        println!("\n{} workflow(s) updated.", results.len());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::ActionLocation;
    use std::path::PathBuf;

    fn make_action(name: &str, version: &str, workflow: &str) -> ExtractedAction {
        ExtractedAction {
            name: name.to_string(),
            version: version.to_string(),
            file: PathBuf::from(format!(".github/workflows/{}", workflow)),
            location: ActionLocation {
                workflow: workflow.to_string(),
                job: "build".to_string(),
                step_index: 0,
            },
        }
    }

    #[test]
    fn test_action_versions_single_version() {
        let mut versions = ActionVersions::default();
        versions.add(&make_action("actions/checkout", "v4", "ci.yml"));
        versions.add(&make_action("actions/checkout", "v4", "deploy.yml"));

        let unique = versions.unique_versions("actions/checkout");
        assert_eq!(unique.len(), 1);
        assert!(unique.contains(&"v4".to_string()));
    }

    #[test]
    fn test_action_versions_multiple_versions() {
        let mut versions = ActionVersions::default();
        versions.add(&make_action("actions/checkout", "v4", "ci.yml"));
        versions.add(&make_action("actions/checkout", "v3", "deploy.yml"));

        let unique = versions.unique_versions("actions/checkout");
        assert_eq!(unique.len(), 2);
        assert!(unique.contains(&"v4".to_string()));
        assert!(unique.contains(&"v3".to_string()));
    }

    #[test]
    fn test_select_version_single() {
        let versions = vec!["v4".to_string()];
        assert_eq!(select_version(&versions), "v4");
    }

    #[test]
    fn test_select_version_picks_highest() {
        let versions = vec!["v3".to_string(), "v4".to_string(), "v2".to_string()];
        assert_eq!(select_version(&versions), "v4");
    }

    #[test]
    fn test_print_results_with_empty_results() {
        let results: Vec<UpdateResult> = vec![];
        print_update_results(&results);
    }

    #[test]
    fn test_print_results_with_updates() {
        let results = vec![UpdateResult {
            file: PathBuf::from("test.yml"),
            changes: vec!["actions/checkout@v4".to_string()],
        }];
        print_update_results(&results);
    }
}
