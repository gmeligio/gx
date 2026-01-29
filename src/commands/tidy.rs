use anyhow::Result;
use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::config::Config;
use crate::github::GitHubClient;
use crate::lock::LockFile;
use crate::manifest::Manifest;
use crate::version::find_highest_version;
use crate::workflow::{ActionLocation, ExtractedAction, UpdateResult, WorkflowUpdater};

/// Represents a usage of an action at a specific location
#[derive(Debug, Clone)]
struct ActionUsage {
    version: String,
    location: ActionLocation,
}

/// Groups action usages by their hierarchical location
#[derive(Debug, Default)]
struct ActionVersionTree {
    usages: HashMap<String, Vec<ActionUsage>>,
}

impl ActionVersionTree {
    fn add(&mut self, action: &ExtractedAction) {
        self.usages
            .entry(action.name.clone())
            .or_default()
            .push(ActionUsage {
                version: action.version.clone(),
                location: action.location.clone(),
            });
    }

    fn unique_versions(&self, action: &str) -> Vec<String> {
        self.usages
            .get(action)
            .map(|usages| {
                usages
                    .iter()
                    .map(|u| u.version.clone())
                    .collect::<HashSet<_>>()
                    .into_iter()
                    .collect()
            })
            .unwrap_or_default()
    }

    fn workflow_versions(&self, action: &str, workflow: &str) -> Vec<String> {
        self.usages
            .get(action)
            .map(|usages| {
                usages
                    .iter()
                    .filter(|u| u.location.workflow == workflow)
                    .map(|u| u.version.clone())
                    .collect::<HashSet<_>>()
                    .into_iter()
                    .collect()
            })
            .unwrap_or_default()
    }

    fn job_versions(&self, action: &str, workflow: &str, job: &str) -> Vec<String> {
        self.usages
            .get(action)
            .map(|usages| {
                usages
                    .iter()
                    .filter(|u| u.location.workflow == workflow && u.location.job == job)
                    .map(|u| u.version.clone())
                    .collect::<HashSet<_>>()
                    .into_iter()
                    .collect()
            })
            .unwrap_or_default()
    }

    fn workflows_using(&self, action: &str) -> Vec<String> {
        self.usages
            .get(action)
            .map(|usages| {
                usages
                    .iter()
                    .map(|u| u.location.workflow.clone())
                    .collect::<HashSet<_>>()
                    .into_iter()
                    .collect()
            })
            .unwrap_or_default()
    }

    fn jobs_in_workflow(&self, action: &str, workflow: &str) -> Vec<String> {
        self.usages
            .get(action)
            .map(|usages| {
                usages
                    .iter()
                    .filter(|u| u.location.workflow == workflow)
                    .map(|u| u.location.job.clone())
                    .collect::<HashSet<_>>()
                    .into_iter()
                    .collect()
            })
            .unwrap_or_default()
    }

    fn steps_in_job(&self, action: &str, workflow: &str, job: &str) -> Vec<(usize, String)> {
        self.usages
            .get(action)
            .map(|usages| {
                usages
                    .iter()
                    .filter(|u| u.location.workflow == workflow && u.location.job == job)
                    .map(|u| (u.location.step_index, u.version.clone()))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn action_names(&self) -> Vec<String> {
        self.usages.keys().cloned().collect()
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

    // Build version tree from extracted actions
    let mut tree = ActionVersionTree::default();
    for action in &extracted {
        tree.add(action);
    }

    let workflow_actions: HashSet<String> = tree.action_names().into_iter().collect();

    // Load current manifest and lock file
    let mut manifest = Manifest::load_from_repo_or_default(repo_root)?;
    let mut lock = LockFile::load_from_repo_or_default(repo_root)?;

    let manifest_actions: HashSet<String> = manifest.actions.keys().cloned().collect();

    // Find differences
    let missing: Vec<_> = workflow_actions.difference(&manifest_actions).collect();
    let unused: Vec<_> = manifest_actions.difference(&workflow_actions).collect();

    let mut manifest_changed = false;

    // Remove unused actions from manifest and lock file
    if !unused.is_empty() {
        println!("\nRemoving unused actions from manifest:");
        for action in &unused {
            println!("  - {}", action);
            manifest.actions.remove(*action);
            remove_action_from_overrides(&mut manifest, action);
        }
        manifest_changed = true;
    }

    // Add missing actions to manifest (with hierarchical overrides if needed)
    if !missing.is_empty() {
        println!("\nAdding missing actions to manifest:");
        for action_name in &missing {
            let versions = tree.unique_versions(action_name);

            if versions.len() == 1 {
                // Single version across all locations -> global
                let version = &versions[0];
                manifest
                    .actions
                    .insert((*action_name).clone(), version.clone());
                println!("  + {}@{}", action_name, version);
            } else {
                // Multiple versions -> need hierarchical overrides
                add_with_hierarchical_overrides(&mut manifest, &tree, action_name, &versions);
            }
        }
        manifest_changed = true;
    }

    // Update existing actions only if manifest has SHA but workflow has tag
    // (This happens when upgrading from SHA to semantic version via comment)
    let existing: Vec<_> = workflow_actions.intersection(&manifest_actions).collect();
    if !existing.is_empty() {
        let mut updated_actions = Vec::new();

        for action_name in &existing {
            let versions = tree.unique_versions(action_name);

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

    // Clean up empty workflow overrides
    clean_empty_overrides(&mut manifest);

    // Save manifest if changed
    if manifest_changed {
        manifest.save()?;
    } else if missing.is_empty() && unused.is_empty() {
        println!("\nManifest is already in sync with workflows.");
    }

    // Update lock file with resolved commit SHAs
    update_lock_file(&mut lock, &manifest, &tree, &config)?;

    // Remove unused entries from lock file
    lock.remove_unused(&manifest.actions);

    // Save lock file
    lock.save_to_repo(repo_root)?;

    // Apply manifest versions to workflows
    if manifest.actions.is_empty() {
        println!("\nNo actions defined in {}", manifest.path()?.display());
        return Ok(());
    }

    let results = updater.update_all(&manifest.actions)?;
    print_update_results(&results);

    Ok(())
}

fn add_with_hierarchical_overrides(
    manifest: &mut Manifest,
    tree: &ActionVersionTree,
    action_name: &str,
    versions: &[String],
) {
    println!("  + {} (multiple versions):", action_name);

    // Find highest semver for global default
    let version_refs: Vec<&str> = versions.iter().map(|s| s.as_str()).collect();
    let global_version = find_highest_version(&version_refs)
        .unwrap_or(&versions[0])
        .to_string();

    manifest
        .actions
        .insert(action_name.to_string(), global_version.clone());
    println!("      global: {}", global_version);

    // Process each workflow
    for workflow in tree.workflows_using(action_name) {
        let wf_versions = tree.workflow_versions(action_name, &workflow);

        if wf_versions.len() == 1 {
            // All uses in this workflow have same version
            let wf_version = &wf_versions[0];
            if wf_version != &global_version {
                manifest
                    .workflow_mut(&workflow)
                    .actions
                    .insert(action_name.to_string(), wf_version.clone());
                println!("      {}: {}", workflow, wf_version);
            }
        } else {
            // Multiple versions in workflow -> check jobs
            for job in tree.jobs_in_workflow(action_name, &workflow) {
                let job_versions = tree.job_versions(action_name, &workflow, &job);

                if job_versions.len() == 1 {
                    // All uses in this job have same version
                    let job_version = &job_versions[0];
                    if job_version != &global_version {
                        manifest
                            .job_mut(&workflow, &job)
                            .actions
                            .insert(action_name.to_string(), job_version.clone());
                        println!("      {}/{}: {}", workflow, job, job_version);
                    }
                } else {
                    // Multiple versions in job -> step-level overrides
                    for (step_idx, step_version) in tree.steps_in_job(action_name, &workflow, &job)
                    {
                        if step_version != global_version {
                            manifest
                                .step_mut(&workflow, &job, step_idx)
                                .actions
                                .insert(action_name.to_string(), step_version.clone());
                            println!(
                                "      {}/{}/step[{}]: {}",
                                workflow, job, step_idx, step_version
                            );
                        }
                    }
                }
            }
        }
    }
}

fn remove_action_from_overrides(manifest: &mut Manifest, action: &str) {
    for workflow_override in manifest.workflows.values_mut() {
        workflow_override.actions.remove(action);
        for job_override in workflow_override.jobs.values_mut() {
            job_override.actions.remove(action);
            for step_override in job_override.steps.values_mut() {
                step_override.actions.remove(action);
            }
        }
    }
}

fn clean_empty_overrides(manifest: &mut Manifest) {
    for workflow_override in manifest.workflows.values_mut() {
        for job_override in workflow_override.jobs.values_mut() {
            job_override
                .steps
                .retain(|_, step| !step.actions.is_empty());
        }
        workflow_override
            .jobs
            .retain(|_, job| !job.actions.is_empty() || !job.steps.is_empty());
    }
    manifest
        .workflows
        .retain(|_, wf| !wf.actions.is_empty() || !wf.jobs.is_empty());
}

fn update_lock_file(
    lock: &mut LockFile,
    manifest: &Manifest,
    _tree: &ActionVersionTree,
    config: &Config,
) -> Result<()> {
    let github = GitHubClient::new(config.github_token.clone())?;

    if !config.has_github_token() {
        println!("  Note: GITHUB_TOKEN not set. Skipping SHA resolution for new actions.");
        println!("  Set GITHUB_TOKEN to resolve version tags to commit SHAs.");
        return Ok(());
    }

    // Collect all action@version combinations that need to be locked
    let mut to_resolve: HashMap<String, HashSet<String>> = HashMap::new();

    // Add from global actions
    for (action, version) in &manifest.actions {
        to_resolve
            .entry(action.clone())
            .or_default()
            .insert(version.clone());
    }

    // Add from workflow overrides
    for workflow_override in manifest.workflows.values() {
        for (action, version) in &workflow_override.actions {
            to_resolve
                .entry(action.clone())
                .or_default()
                .insert(version.clone());
        }
        for job_override in workflow_override.jobs.values() {
            for (action, version) in &job_override.actions {
                to_resolve
                    .entry(action.clone())
                    .or_default()
                    .insert(version.clone());
            }
            for step_override in job_override.steps.values() {
                for (action, version) in &step_override.actions {
                    to_resolve
                        .entry(action.clone())
                        .or_default()
                        .insert(version.clone());
                }
            }
        }
    }

    // Resolve each action@version to commit SHA
    for (action, versions) in to_resolve {
        for version in versions {
            // Skip if already in lock file
            if lock.has(&action, &version) {
                continue;
            }

            // Resolve via GitHub API
            println!("  Resolving {}@{} ...", action, version);
            match github.resolve_ref(&action, &version) {
                Ok(sha) => {
                    lock.set(&action, &version, sha);
                }
                Err(e) => {
                    eprintln!("  Warning: Failed to resolve {}@{}: {}", action, version, e);
                    eprintln!("  Skipping lock for this action.");
                }
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
    use std::path::PathBuf;

    fn make_action(
        name: &str,
        version: &str,
        workflow: &str,
        job: &str,
        step: usize,
    ) -> ExtractedAction {
        ExtractedAction {
            name: name.to_string(),
            version: version.to_string(),
            file: PathBuf::from(format!(".github/workflows/{}", workflow)),
            location: ActionLocation {
                workflow: workflow.to_string(),
                job: job.to_string(),
                step_index: step,
            },
        }
    }

    #[test]
    fn test_version_tree_single_version() {
        let mut tree = ActionVersionTree::default();
        tree.add(&make_action("actions/checkout", "v4", "ci.yml", "build", 0));
        tree.add(&make_action(
            "actions/checkout",
            "v4",
            "deploy.yml",
            "deploy",
            0,
        ));

        let versions = tree.unique_versions("actions/checkout");
        assert_eq!(versions.len(), 1);
        assert!(versions.contains(&"v4".to_string()));
    }

    #[test]
    fn test_version_tree_multiple_versions() {
        let mut tree = ActionVersionTree::default();
        tree.add(&make_action("actions/checkout", "v4", "ci.yml", "build", 0));
        tree.add(&make_action(
            "actions/checkout",
            "v3",
            "deploy.yml",
            "deploy",
            0,
        ));

        let versions = tree.unique_versions("actions/checkout");
        assert_eq!(versions.len(), 2);
        assert!(versions.contains(&"v4".to_string()));
        assert!(versions.contains(&"v3".to_string()));
    }

    #[test]
    fn test_version_tree_workflow_versions() {
        let mut tree = ActionVersionTree::default();
        tree.add(&make_action("actions/checkout", "v4", "ci.yml", "build", 0));
        tree.add(&make_action("actions/checkout", "v4", "ci.yml", "test", 0));
        tree.add(&make_action(
            "actions/checkout",
            "v3",
            "deploy.yml",
            "deploy",
            0,
        ));

        let ci_versions = tree.workflow_versions("actions/checkout", "ci.yml");
        assert_eq!(ci_versions.len(), 1);
        assert!(ci_versions.contains(&"v4".to_string()));

        let deploy_versions = tree.workflow_versions("actions/checkout", "deploy.yml");
        assert_eq!(deploy_versions.len(), 1);
        assert!(deploy_versions.contains(&"v3".to_string()));
    }

    #[test]
    fn test_version_tree_job_versions() {
        let mut tree = ActionVersionTree::default();
        tree.add(&make_action("actions/checkout", "v4", "ci.yml", "build", 0));
        tree.add(&make_action("actions/checkout", "v3", "ci.yml", "test", 0));

        let build_versions = tree.job_versions("actions/checkout", "ci.yml", "build");
        assert_eq!(build_versions.len(), 1);
        assert!(build_versions.contains(&"v4".to_string()));

        let test_versions = tree.job_versions("actions/checkout", "ci.yml", "test");
        assert_eq!(test_versions.len(), 1);
        assert!(test_versions.contains(&"v3".to_string()));
    }

    #[test]
    fn test_remove_action_from_overrides() {
        let mut manifest = Manifest::default();
        manifest
            .actions
            .insert("actions/checkout".to_string(), "v4".to_string());
        manifest
            .workflow_mut("ci.yml")
            .actions
            .insert("actions/checkout".to_string(), "v3".to_string());
        manifest
            .job_mut("ci.yml", "test")
            .actions
            .insert("actions/checkout".to_string(), "v2".to_string());

        remove_action_from_overrides(&mut manifest, "actions/checkout");

        assert!(manifest.workflows.get("ci.yml").unwrap().actions.is_empty());
        assert!(
            manifest
                .workflows
                .get("ci.yml")
                .unwrap()
                .jobs
                .get("test")
                .unwrap()
                .actions
                .is_empty()
        );
    }

    #[test]
    fn test_clean_empty_overrides() {
        let mut manifest = Manifest::default();
        // Create empty overrides
        manifest.workflow_mut("ci.yml");
        manifest.job_mut("ci.yml", "test");

        clean_empty_overrides(&mut manifest);

        assert!(manifest.workflows.is_empty());
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
