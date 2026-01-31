use anyhow::Result;
use log::{debug, info};
use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::domain::{
    ActionId, CommitSha, LockKey, ResolutionResult, ResolutionService, Version, is_commit_sha,
    select_highest_version,
};
use crate::infrastructure::{
    ExtractedAction, GitHubClient, LockStore, ManifestStore, UpdateResult, WorkflowParser,
    WorkflowWriter,
};

/// Tracks a version correction when SHA doesn't match the version comment
#[derive(Debug)]
struct VersionCorrection {
    action: ActionId,
    old_version: Version,
    new_version: Version,
    sha: CommitSha,
}

/// Groups action versions across all workflows
#[derive(Debug, Default)]
struct ActionVersions {
    /// Maps action ID to set of versions found in workflows
    versions: HashMap<ActionId, HashSet<Version>>,
    /// Maps action ID to SHA if present in workflow (first one wins)
    shas: HashMap<ActionId, CommitSha>,
}

impl ActionVersions {
    fn add(&mut self, action: &ExtractedAction) {
        self.versions
            .entry(action.id.clone())
            .or_default()
            .insert(action.version.clone());

        // Store SHA if present (first one wins for consistency)
        if let Some(sha) = &action.sha {
            self.shas.entry(action.id.clone()).or_insert(sha.clone());
        }
    }

    fn unique_versions(&self, action: &ActionId) -> Vec<Version> {
        self.versions
            .get(action)
            .map(|v| v.iter().cloned().collect())
            .unwrap_or_default()
    }

    fn action_ids(&self) -> Vec<ActionId> {
        self.versions.keys().cloned().collect()
    }

    fn get_sha(&self, action: &ActionId) -> Option<&CommitSha> {
        self.shas.get(action)
    }
}

/// Run the tidy command to synchronize workflow actions with the manifest.
///
/// # Errors
///
/// Returns an error if workflows cannot be read or files cannot be saved.
///
/// # Panics
///
/// Panics if an action in the intersection of workflow and manifest actions is not found
/// in the manifest (this should never happen due to the intersection logic).
pub fn run<M: ManifestStore, L: LockStore>(
    repo_root: &Path,
    mut manifest: M,
    mut lock: L,
) -> Result<()> {
    let parser = WorkflowParser::new(repo_root);
    let writer = WorkflowWriter::new(repo_root);

    let workflows = parser.find_workflows()?;
    if workflows.is_empty() {
        info!("No workflows found in .github/workflows/");
        return Ok(());
    }

    debug!("Scanning workflows...");
    for workflow in &workflows {
        debug!("{}", workflow.display());
    }

    let extracted = parser.extract_all()?;

    // Collect versions for each action
    let mut action_versions = ActionVersions::default();
    for action in &extracted {
        action_versions.add(action);
    }

    let workflow_actions: HashSet<ActionId> = action_versions.action_ids().into_iter().collect();

    let manifest_actions: HashSet<ActionId> =
        manifest.specs().iter().map(|s| s.id.clone()).collect();

    // Find differences
    let missing: Vec<_> = workflow_actions.difference(&manifest_actions).collect();
    let unused: Vec<_> = manifest_actions.difference(&workflow_actions).collect();

    // Remove unused actions from manifest
    if !unused.is_empty() {
        info!("Removing unused actions from manifest:");
        for action in &unused {
            info!("- {action}");
            manifest.remove(action);
        }
    }

    // Add missing actions to manifest (using highest version if multiple exist)
    if !missing.is_empty() {
        info!("Adding missing actions to manifest:");
        for action_id in &missing {
            let versions = action_versions.unique_versions(action_id);
            let version = select_version(&versions);
            manifest.set((*action_id).clone(), version.clone());
            info!("+ {action_id}@{version}");
        }
    }

    // Update existing actions only if manifest has SHA but workflow has tag
    // (This happens when upgrading from SHA to semantic version via comment)
    let existing: Vec<_> = workflow_actions.intersection(&manifest_actions).collect();
    if !existing.is_empty() {
        let mut updated_actions = Vec::new();

        for action_id in &existing {
            let versions = action_versions.unique_versions(action_id);

            if versions.len() == 1 {
                let workflow_version = &versions[0];
                let manifest_version = manifest.get(action_id).unwrap().clone();

                // Only update if:
                // 1. Versions differ, AND
                // 2. Manifest has a SHA (40 hex chars) and workflow has a semantic version
                let manifest_is_sha = is_commit_sha(manifest_version.as_str());
                let workflow_is_semver = workflow_version.as_str().starts_with('v')
                    || workflow_version
                        .as_str()
                        .chars()
                        .next()
                        .is_some_and(|c| c.is_ascii_digit());

                if workflow_version != &manifest_version && manifest_is_sha && workflow_is_semver {
                    manifest.set((*action_id).clone(), workflow_version.clone());
                    updated_actions.push(format!(
                        "{action_id}@{workflow_version} (was {manifest_version})"
                    ));
                }
            }
        }

        if !updated_actions.is_empty() {
            info!("Updating action versions in manifest:");
            for update in &updated_actions {
                info!("~ {update}");
            }
        }
    }

    // Update lock file with resolved commit SHAs and validate version comments
    let corrections = update_lock_file(&mut lock, &mut manifest, &action_versions)?;

    // Save manifest if changed (includes corrections)
    manifest.save()?;

    // Remove unused entries from lock file
    let keys_to_keep: Vec<LockKey> = manifest.specs().iter().map(LockKey::from).collect();
    lock.retain(&keys_to_keep);

    // Save lock file only if changed
    lock.save()?;

    // Apply manifest versions to workflows using SHAs from lock file
    if manifest.is_empty() {
        info!("No actions found in {}", manifest.path()?.display());
        return Ok(());
    }

    // Build update map with SHAs from lock file and version comments from manifest
    let update_map = lock.build_update_map(&keys_to_keep);

    // Convert ActionId keys to String keys for workflow writer
    let string_update_map: HashMap<ActionId, String> = update_map;
    let results = writer.update_all(&string_update_map)?;
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
fn select_version(versions: &[Version]) -> Version {
    select_highest_version(versions).unwrap_or_else(|| versions[0].clone())
}

fn update_lock_file<M: ManifestStore, L: LockStore>(
    lock: &mut L,
    manifest: &mut M,
    action_versions: &ActionVersions,
) -> Result<Vec<VersionCorrection>> {
    let mut corrections = Vec::new();

    let specs = manifest.specs();

    // Check if there are any actions that need resolving
    let needs_resolving = specs.iter().any(|spec| !lock.has(&LockKey::from(spec)));

    // Also check if any actions have SHAs that need validation
    let has_workflow_shas = specs
        .iter()
        .any(|spec| action_versions.get_sha(&spec.id).is_some());

    if !needs_resolving && !has_workflow_shas {
        return Ok(corrections);
    }

    let github = GitHubClient::from_env()?;
    let resolution_service = ResolutionService::new(github);

    // Process each action in manifest
    for spec in &specs {
        // Check if workflow has a SHA for this action
        if let Some(workflow_sha) = action_versions.get_sha(&spec.id) {
            // Validate that version comment matches the SHA and determine correct version
            let result = resolution_service.validate_and_correct(spec, workflow_sha);

            match result {
                ResolutionResult::Resolved(resolved) => {
                    lock.set(&resolved);
                }
                ResolutionResult::Corrected {
                    original,
                    corrected,
                } => {
                    corrections.push(VersionCorrection {
                        action: original.id.clone(),
                        old_version: original.version.clone(),
                        new_version: corrected.version.clone(),
                        sha: corrected.sha.clone(),
                    });

                    // Update manifest with correct version
                    manifest.set(corrected.id.clone(), corrected.version.clone());
                    lock.set(&corrected);
                }
                ResolutionResult::Unresolved { spec: s, reason } => {
                    debug!("Could not resolve {}@{}: {}", s.id, s.version, reason);
                }
            }
        } else {
            let key = LockKey::from(spec);
            if !lock.has(&key) {
                // Resolve via GitHub API when there is no workflow SHA
                debug!("Resolving {}@{}", spec.id, spec.version);
                let result = resolution_service.resolve(spec);

                match result {
                    ResolutionResult::Resolved(resolved) => {
                        lock.set(&resolved);
                    }
                    ResolutionResult::Unresolved { spec: s, reason } => {
                        debug!("Could not resolve {}@{}: {}", s.id, s.version, reason);
                    }
                    ResolutionResult::Corrected { corrected, .. } => {
                        lock.set(&corrected);
                    }
                }
            }
        }
    }

    Ok(corrections)
}

fn print_update_results(results: &[UpdateResult]) {
    if results.is_empty() {
        info!("Workflows are already up to date.");
    } else {
        info!("Updated workflows:");
        for result in results {
            info!("{}", result.file.display());
            for change in &result.changes {
                info!("~ {change}");
            }
        }
        info!("{} workflow(s) updated.", results.len());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::ActionLocation;
    use std::path::PathBuf;

    fn make_action(name: &str, version: &str, workflow: &str) -> ExtractedAction {
        ExtractedAction {
            id: ActionId::from(name),
            version: Version::from(version),
            sha: None,
            file: PathBuf::from(format!(".github/workflows/{workflow}")),
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

        let unique = versions.unique_versions(&ActionId::from("actions/checkout"));
        assert_eq!(unique.len(), 1);
        assert!(unique.contains(&Version::from("v4")));
    }

    #[test]
    fn test_action_versions_multiple_versions() {
        let mut versions = ActionVersions::default();
        versions.add(&make_action("actions/checkout", "v4", "ci.yml"));
        versions.add(&make_action("actions/checkout", "v3", "deploy.yml"));

        let unique = versions.unique_versions(&ActionId::from("actions/checkout"));
        assert_eq!(unique.len(), 2);
        assert!(unique.contains(&Version::from("v4")));
        assert!(unique.contains(&Version::from("v3")));
    }

    #[test]
    fn test_select_version_single() {
        let versions = vec![Version::from("v4")];
        assert_eq!(select_version(&versions), Version::from("v4"));
    }

    #[test]
    fn test_select_version_picks_highest() {
        let versions = vec![
            Version::from("v3"),
            Version::from("v4"),
            Version::from("v2"),
        ];
        assert_eq!(select_version(&versions), Version::from("v4"));
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
    fn test_action_versions_collects_sha() {
        let mut versions = ActionVersions::default();
        let mut action = make_action("actions/checkout", "v4", "ci.yml");
        action.sha = Some(CommitSha::from("abc123def456789012345678901234567890abcd"));
        versions.add(&action);

        assert_eq!(
            versions.get_sha(&ActionId::from("actions/checkout")),
            Some(&CommitSha::from("abc123def456789012345678901234567890abcd"))
        );
    }

    #[test]
    fn test_action_versions_sha_first_wins() {
        let mut versions = ActionVersions::default();

        let mut action1 = make_action("actions/checkout", "v4", "ci.yml");
        action1.sha = Some(CommitSha::from(
            "first_sha_12345678901234567890123456789012",
        ));
        versions.add(&action1);

        let mut action2 = make_action("actions/checkout", "v4", "deploy.yml");
        action2.sha = Some(CommitSha::from(
            "second_sha_1234567890123456789012345678901",
        ));
        versions.add(&action2);

        // First SHA should win
        assert_eq!(
            versions.get_sha(&ActionId::from("actions/checkout")),
            Some(&CommitSha::from(
                "first_sha_12345678901234567890123456789012"
            ))
        );
    }

    #[test]
    fn test_action_versions_no_sha() {
        let mut versions = ActionVersions::default();
        versions.add(&make_action("actions/checkout", "v4", "ci.yml"));

        assert!(
            versions
                .get_sha(&ActionId::from("actions/checkout"))
                .is_none()
        );
    }
}
