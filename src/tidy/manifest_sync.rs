use crate::domain::action::identity::{ActionId, CommitSha, Version};
use crate::domain::action::spec::Spec as ActionSpec;
use crate::domain::action::specifier::Specifier;
use crate::domain::action::tag_selection::select_most_specific_tag;
use crate::domain::event::Event as SyncEvent;
use crate::domain::file::actions::ActionSet as WorkflowActionSet;
use crate::domain::manifest::Manifest;
use crate::domain::resolution::{ActionResolver, VersionRegistry};
use std::collections::HashSet;

/// Remove unused actions from manifest and add missing ones.
/// Returns events for each added action.
pub(super) fn sync_manifest_actions(
    manifest: &mut Manifest,
    action_set: &WorkflowActionSet,
) -> Vec<SyncEvent> {
    let mut events = Vec::new();

    let workflow_actions: HashSet<ActionId> = action_set.action_ids().cloned().collect();
    let manifest_actions: HashSet<ActionId> = manifest.specs().map(|s| s.id.clone()).collect();

    // Remove unused actions from manifest
    let unused: Vec<_> = manifest_actions.difference(&workflow_actions).collect();
    for action in &unused {
        manifest.remove(action);
    }

    // Add missing actions to manifest. The dominant version is written as-is:
    // a `Pinned` ref votes with its tag comment and a plain `Ref` with its
    // tag/branch, so the manifest records a human-readable specifier directly.
    // A bare `ParsedRef::Sha` votes with its SHA string, which is written
    // verbatim — the on-disk manifest then carries the SHA as its specifier,
    // and `upgrade_sha_versions_to_tags` promotes it to a tag on a later pass.
    let missing: Vec<_> = workflow_actions.difference(&manifest_actions).collect();
    for action_id in missing {
        let version = select_dominant_version(action_id, action_set);

        let spec_version = Specifier::from_v1(version.as_str());
        manifest.set((*action_id).clone(), spec_version.clone());
        let spec = ActionSpec::new((*action_id).clone(), spec_version.clone());
        events.push(SyncEvent::ActionAdded(spec));
    }

    events
}

/// Upgrade SHA versions in manifest to tags.
/// Returns events for each SHA that was upgraded.
pub(super) fn upgrade_sha_versions_to_tags<R: VersionRegistry>(
    manifest: &mut Manifest,
    resolver: &ActionResolver<'_, R>,
) -> Vec<SyncEvent> {
    let mut events = Vec::new();

    // Collect only SHA specs (avoid cloning the full Vec when most specs are tags)
    let sha_specs: Vec<(ActionId, CommitSha)> = manifest
        .specs()
        .filter(|s| s.specifier.is_sha())
        .map(|s| (s.id.clone(), CommitSha::from(s.specifier.as_str())))
        .collect();

    for (id, sha) in &sha_specs {
        match resolver.registry().describe_sha(id, sha) {
            Ok(desc) => {
                if let Some(best_tag) = select_most_specific_tag(&desc.tags) {
                    manifest.set(id.clone(), Specifier::from_v1(best_tag.as_str()));
                    events.push(SyncEvent::ShaUpgraded {
                        id: id.clone(),
                        tag: best_tag.clone(),
                    });
                }
            }
            Err(_e) => {
                // Silently skip if SHA cannot be upgraded
            }
        }
    }

    events
}

/// Select the highest version from a non-empty slice of versions.
pub(super) fn select_version(versions: &[Version]) -> Version {
    #[expect(
        clippy::indexing_slicing,
        reason = "function is only called with non-empty slices"
    )]
    Version::highest(versions).unwrap_or_else(|| versions[0].clone())
}

/// Select the dominant version from usage counts and available versions.
pub(super) fn select_dominant_version(
    action_id: &ActionId,
    action_set: &WorkflowActionSet,
) -> Version {
    action_set.dominant_version(action_id).unwrap_or_else(|| {
        let versions: Vec<Version> = action_set.versions_for(action_id).cloned().collect();
        select_version(&versions)
    })
}

#[cfg(test)]
mod tests {
    use super::{Version, select_version, upgrade_sha_versions_to_tags};
    use crate::domain::action::identity::ActionId;
    use crate::domain::action::specifier::Specifier;
    use crate::domain::manifest::Manifest;
    use crate::domain::resolution::ActionResolver;
    use crate::domain::resolution::testutil::{AuthRequiredRegistry, FakeRegistry};

    #[test]
    fn select_version_single() {
        let versions = vec![Version::from("v4")];
        assert_eq!(select_version(&versions), Version::from("v4"));
    }

    #[test]
    fn select_version_picks_highest() {
        let versions = vec![
            Version::from("v3"),
            Version::from("v4"),
            Version::from("v2"),
        ];
        assert_eq!(select_version(&versions), Version::from("v4"));
    }

    // ---------------------------------------------------------------------------
    // SHA-to-tag upgrade tests (migrated from tidy/tests.rs)
    // ---------------------------------------------------------------------------

    /// Manifest SHA specifier is upgraded to the most specific tag via the registry.
    #[test]
    fn sha_to_tag_upgrade_via_registry() {
        let sha = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let mut manifest = Manifest::default();
        manifest.set(ActionId::from("actions/checkout"), Specifier::from_v1(sha));

        let registry = FakeRegistry::new().with_all_tags("actions/checkout", vec!["v4", "v4.0.0"]);
        let resolver = ActionResolver::new(&registry);
        upgrade_sha_versions_to_tags(&mut manifest, &resolver);

        assert_eq!(
            manifest.get(&ActionId::from("actions/checkout")),
            Some(&Specifier::from_v1("v4.0.0")),
            "SHA must be upgraded to most specific tag"
        );
    }

    /// Without a token, SHA stays unchanged — registry returns `AuthRequired` gracefully.
    #[test]
    fn sha_to_tag_upgrade_graceful_without_token() {
        let sha = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let mut manifest = Manifest::default();
        manifest.set(ActionId::from("actions/checkout"), Specifier::from_v1(sha));

        let resolver = ActionResolver::new(&AuthRequiredRegistry);
        upgrade_sha_versions_to_tags(&mut manifest, &resolver);

        // SHA must stay unchanged when no token available
        assert_eq!(
            manifest.get(&ActionId::from("actions/checkout")),
            Some(&Specifier::from_v1(sha)),
            "SHA must stay unchanged without a token"
        );
    }
}
