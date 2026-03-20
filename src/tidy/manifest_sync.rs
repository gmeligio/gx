use crate::domain::action::identity::{ActionId, CommitSha, Version};
use crate::domain::action::spec::Spec as ActionSpec;
use crate::domain::action::specifier::Specifier;
use crate::domain::action::tag_selection::{ShaIndex, select_most_specific_tag};
use crate::domain::event::Event as SyncEvent;
use crate::domain::manifest::Manifest;
use crate::domain::resolution::{ActionResolver, VersionRegistry};
use crate::domain::workflow_actions::{ActionSet as WorkflowActionSet, Located as LocatedAction};
use std::collections::HashSet;

/// Remove unused actions from manifest and add missing ones.
/// Returns events for each added action.
pub(super) fn sync_manifest_actions<R: VersionRegistry>(
    manifest: &mut Manifest,
    located: &[LocatedAction],
    action_set: &WorkflowActionSet,
    resolver: &ActionResolver<'_, R>,
    sha_index: &mut ShaIndex,
) -> Vec<SyncEvent> {
    let mut events = Vec::new();

    let workflow_actions: HashSet<ActionId> = action_set.action_ids().cloned().collect();
    let manifest_actions: HashSet<ActionId> = manifest.specs().map(|s| s.id.clone()).collect();

    // Remove unused actions from manifest
    let unused: Vec<_> = manifest_actions.difference(&workflow_actions).collect();
    for action in &unused {
        manifest.remove(action);
    }

    // Add missing actions to manifest
    let missing: Vec<_> = workflow_actions.difference(&manifest_actions).collect();
    for action_id in missing {
        let version = select_dominant_version(action_id, action_set);

        let corrected_version = if version.is_sha() {
            let located_with_version = located.iter().find(|loc| {
                &loc.action.id == action_id
                    && loc.action.version == version
                    && loc.action.sha.is_some()
            });

            located_with_version.map_or_else(
                || version.clone(),
                |located_action| {
                    located_action.action.sha.as_ref().map_or_else(
                        || version.clone(),
                        |sha| {
                            let (corrected, was_corrected) =
                                resolver.correct_version(action_id, sha, &version, sha_index);
                            if was_corrected {
                                events.push(SyncEvent::VersionCorrected {
                                    id: (*action_id).clone(),
                                    corrected: corrected.clone(),
                                    sha_points_to: corrected.clone(),
                                });
                            }
                            corrected
                        },
                    )
                },
            )
        } else {
            version.clone()
        };

        let spec_version = Specifier::from_v1(corrected_version.as_str());
        manifest.set((*action_id).clone(), spec_version.clone());
        let spec = ActionSpec::new((*action_id).clone(), spec_version.clone());
        events.push(SyncEvent::ActionAdded(spec));
    }

    events
}

/// Upgrade SHA versions in manifest to tags via `ShaIndex`.
/// Returns events for each SHA that was upgraded.
pub(super) fn upgrade_sha_versions_to_tags<R: VersionRegistry>(
    manifest: &mut Manifest,
    resolver: &ActionResolver<'_, R>,
    sha_index: &mut ShaIndex,
) -> Vec<SyncEvent> {
    let mut events = Vec::new();

    // Collect only SHA specs (avoid cloning the full Vec when most specs are tags)
    let sha_specs: Vec<(ActionId, CommitSha)> = manifest
        .specs()
        .filter(|s| s.specifier.is_sha())
        .map(|s| (s.id.clone(), CommitSha::from(s.specifier.as_str())))
        .collect();

    for (id, sha) in &sha_specs {
        match sha_index.get_or_describe(resolver.registry(), id, sha) {
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
    use crate::domain::action::tag_selection::ShaIndex;
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
        let mut sha_index = ShaIndex::new();
        upgrade_sha_versions_to_tags(&mut manifest, &resolver, &mut sha_index);

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
        let mut sha_index = ShaIndex::new();
        upgrade_sha_versions_to_tags(&mut manifest, &resolver, &mut sha_index);

        // SHA must stay unchanged when no token available
        assert_eq!(
            manifest.get(&ActionId::from("actions/checkout")),
            Some(&Specifier::from_v1(sha)),
            "SHA must stay unchanged without a token"
        );
    }
}
