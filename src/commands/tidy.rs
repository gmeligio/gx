use anyhow::Result;
use log::{debug, info, warn};
use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::error::GitHubTokenRequired;
use crate::git::is_commit_sha;
use crate::github::GitHubClient;
use crate::lock::LockFile;
use crate::manifest::Manifest;
use crate::version::find_highest_version;
use crate::workflow::{ExtractedAction, UpdateResult, WorkflowUpdater};

/// Tracks a version correction when SHA doesn't match the version comment
#[derive(Debug)]
struct VersionCorrection {
    action: String,
    old_version: String,
    new_version: String,
    sha: String,
}

/// Groups action versions across all workflows
#[derive(Debug, Default)]
struct ActionVersions {
    /// Maps action name to set of versions found in workflows
    versions: HashMap<String, HashSet<String>>,
    /// Maps action name to SHA if present in workflow (first one wins)
    shas: HashMap<String, String>,
}

impl ActionVersions {
    fn add(&mut self, action: &ExtractedAction) {
        self.versions
            .entry(action.name.clone())
            .or_default()
            .insert(action.version.clone());

        // Store SHA if present (first one wins for consistency)
        if let Some(sha) = &action.sha {
            self.shas.entry(action.name.clone()).or_insert(sha.clone());
        }
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

    fn get_sha(&self, action: &str) -> Option<&String> {
        self.shas.get(action)
    }
}

pub fn run(repo_root: &Path) -> Result<()> {
    let updater = WorkflowUpdater::new(repo_root);

    let workflows = updater.find_workflows()?;
    if workflows.is_empty() {
        info!("No workflows found in .github/workflows/");
        return Ok(());
    }

    debug!("Scanning workflows...");
    for workflow in &workflows {
        debug!("{}", workflow.display());
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

    // Remove unused actions from manifest
    if !unused.is_empty() {
        info!("Removing unused actions from manifest:");
        for action in &unused {
            info!("- {}", action);
            manifest.remove(action);
        }
    }

    // Add missing actions to manifest (using highest version if multiple exist)
    if !missing.is_empty() {
        info!("Adding missing actions to manifest:");
        for action_name in &missing {
            let versions = action_versions.unique_versions(action_name);
            let version = select_version(&versions);
            manifest.set((*action_name).clone(), version.clone());
            info!("+ {}@{}", action_name, version);
        }
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
                let manifest_is_sha = is_commit_sha(&manifest_version);
                let workflow_is_semver = workflow_version.starts_with('v')
                    || workflow_version
                        .chars()
                        .next()
                        .map(|c| c.is_ascii_digit())
                        .unwrap_or(false);

                if workflow_version != &manifest_version && manifest_is_sha && workflow_is_semver {
                    manifest.set((*action_name).clone(), workflow_version.clone());
                    updated_actions.push(format!(
                        "{}@{} (was {})",
                        action_name, workflow_version, manifest_version
                    ));
                }
            }
        }

        if !updated_actions.is_empty() {
            info!("Updating action versions in manifest:");
            for update in &updated_actions {
                info!("~ {}", update);
            }
        }
    }

    // Update lock file with resolved commit SHAs and validate version comments
    let corrections = update_lock_file(&mut lock, &mut manifest, &action_versions)?;

    // Save manifest if changed (includes corrections)
    manifest.save_if_changed()?;

    // Remove unused entries from lock file
    lock.remove_unused(&manifest.actions);

    // Save lock file only if changed
    lock.save_if_changed()?;

    // Apply manifest versions to workflows using SHAs from lock file
    if manifest.actions.is_empty() {
        info!("No actions defined in {}", manifest.path()?.display());
        return Ok(());
    }

    // Build update map with SHAs from lock file and version comments from manifest
    let update_map = lock.build_update_map(&manifest.actions);
    let results = updater.update_all(&update_map)?;
    print_update_results(&results);

    // Print summary of version corrections
    if !corrections.is_empty() {
        info!("Version corrections:");
        for c in &corrections {
            info!(
                "{} {} -> {} (SHA {} points to {})",
                c.action, c.old_version, c.new_version, c.sha, c.new_version
            );
        }
    }

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

fn update_lock_file(
    lock: &mut LockFile,
    manifest: &mut Manifest,
    action_versions: &ActionVersions,
) -> Result<Vec<VersionCorrection>> {
    let mut corrections = Vec::new();

    // Check if there are any actions that need resolving
    let needs_resolving = manifest
        .actions
        .iter()
        .any(|(action, version)| !lock.has(action, version));

    // Also check if any actions have SHAs that need validation
    let has_workflow_shas = manifest
        .actions
        .keys()
        .any(|action| action_versions.get_sha(action).is_some());

    if !needs_resolving && !has_workflow_shas {
        return Ok(corrections);
    }

    let github = GitHubClient::from_env()?;

    // Process each action in manifest
    for (action, version) in manifest.actions.clone().iter() {
        // Check if workflow has a SHA for this action
        if let Some(workflow_sha) = action_versions.get_sha(action) {
            // Validate that version comment matches the SHA and determine correct version
            let final_version = match github.get_tags_for_sha(action, workflow_sha) {
                Ok(tags) => {
                    if tags.iter().any(|t| t == version) {
                        // Version matches SHA, use as-is
                        version.clone()
                    } else if let Some(correct_version) = select_best_tag(&tags) {
                        // Version comment doesn't match SHA - use the correct version
                        info!(
                            "Corrected {} version: {} -> {} (SHA {} points to {})",
                            action, version, correct_version, workflow_sha, correct_version
                        );

                        corrections.push(VersionCorrection {
                            action: action.clone(),
                            old_version: version.clone(),
                            new_version: correct_version.clone(),
                            sha: workflow_sha.clone(),
                        });

                        // Update manifest with correct version
                        manifest.set(action.clone(), correct_version.clone());
                        correct_version
                    } else {
                        warn!(
                            "No tags found for {} SHA {}, keeping version {}",
                            action, workflow_sha, version
                        );
                        version.clone()
                    }
                }
                Err(e) => {
                    // Log warning but continue - don't fail the whole operation
                    if e.downcast_ref::<GitHubTokenRequired>().is_some() {
                        warn!("GITHUB_TOKEN not set. Cannot validate {} SHA.", action);
                        warn!("Set GITHUB_TOKEN to resolve version tags to commit SHAs.");
                    } else {
                        warn!("Could not validate {} SHA: {}", action, e);
                    }
                    version.clone()
                }
            };

            // Set lock entry with the validated/corrected version
            lock.set(action, &final_version, workflow_sha.clone());
        } else if !lock.has(action, version) {
            // No workflow SHA - resolve via GitHub API
            debug!("Resolving {}@{} ...", action, version);
            match github.resolve_ref(action, version) {
                Ok(sha) => {
                    lock.set(action, version, sha);
                }
                Err(e) => {
                    if e.downcast_ref::<GitHubTokenRequired>().is_some() {
                        warn!(
                            "GITHUB_TOKEN not set. Cannot resolve {}@{} to commit SHA.",
                            action, version
                        );
                        warn!("Set GITHUB_TOKEN to resolve version tags to commit SHAs.");
                    } else {
                        warn!("Could not resolve {}@{}: {}", action, version, e);
                    }
                }
            }
        }
    }

    Ok(corrections)
}

/// Select the best tag from a list (prefers shorter semver-like tags)
fn select_best_tag(tags: &[String]) -> Option<String> {
    if tags.is_empty() {
        return None;
    }

    // Prefer tags that look like semver (v1, v1.2, v1.2.3)
    // Sort by: semver-like first, then by length (shorter is better for major version tags)
    let mut sorted_tags: Vec<_> = tags.iter().collect();
    sorted_tags.sort_by(|a, b| {
        let a_is_semver =
            a.starts_with('v') && a.chars().nth(1).is_some_and(|c| c.is_ascii_digit());
        let b_is_semver =
            b.starts_with('v') && b.chars().nth(1).is_some_and(|c| c.is_ascii_digit());

        match (a_is_semver, b_is_semver) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.len().cmp(&b.len()),
        }
    });

    sorted_tags.first().map(|s| (*s).clone())
}

fn print_update_results(results: &[UpdateResult]) {
    if results.is_empty() {
        info!("Workflows are already up to date.");
    } else {
        info!("Updated workflows:");
        for result in results {
            info!("{}", result.file.display());
            for change in &result.changes {
                info!("- {}", change);
            }
        }
        info!("{} workflow(s) updated.", results.len());
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
            sha: None,
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

    #[test]
    fn test_select_best_tag_empty() {
        let tags: Vec<String> = vec![];
        assert!(select_best_tag(&tags).is_none());
    }

    #[test]
    fn test_select_best_tag_prefers_semver() {
        let tags = vec![
            "release-1".to_string(),
            "v4".to_string(),
            "latest".to_string(),
        ];
        assert_eq!(select_best_tag(&tags), Some("v4".to_string()));
    }

    #[test]
    fn test_select_best_tag_prefers_shorter_semver() {
        // Both are semver-like, should prefer shorter (v4 over v4.0.0)
        let tags = vec!["v4.0.0".to_string(), "v4".to_string(), "v4.0".to_string()];
        assert_eq!(select_best_tag(&tags), Some("v4".to_string()));
    }

    #[test]
    fn test_select_best_tag_single() {
        let tags = vec!["v5".to_string()];
        assert_eq!(select_best_tag(&tags), Some("v5".to_string()));
    }

    #[test]
    fn test_action_versions_collects_sha() {
        let mut versions = ActionVersions::default();
        let mut action = make_action("actions/checkout", "v4", "ci.yml");
        action.sha = Some("abc123def456789012345678901234567890abcd".to_string());
        versions.add(&action);

        assert_eq!(
            versions.get_sha("actions/checkout"),
            Some(&"abc123def456789012345678901234567890abcd".to_string())
        );
    }

    #[test]
    fn test_action_versions_sha_first_wins() {
        let mut versions = ActionVersions::default();

        let mut action1 = make_action("actions/checkout", "v4", "ci.yml");
        action1.sha = Some("first_sha_12345678901234567890123456789012".to_string());
        versions.add(&action1);

        let mut action2 = make_action("actions/checkout", "v4", "deploy.yml");
        action2.sha = Some("second_sha_1234567890123456789012345678901".to_string());
        versions.add(&action2);

        // First SHA should win
        assert_eq!(
            versions.get_sha("actions/checkout"),
            Some(&"first_sha_12345678901234567890123456789012".to_string())
        );
    }

    #[test]
    fn test_action_versions_no_sha() {
        let mut versions = ActionVersions::default();
        versions.add(&make_action("actions/checkout", "v4", "ci.yml"));

        assert!(versions.get_sha("actions/checkout").is_none());
    }
}
