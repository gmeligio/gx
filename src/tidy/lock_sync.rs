use std::collections::HashMap;

use super::Error as TidyError;
use crate::domain::action::identity::CommitSha;
use crate::domain::action::spec::Spec as ActionSpec;
use crate::domain::action::tag_selection::ShaIndex;
use crate::domain::event::Event as SyncEvent;
use crate::domain::lock::Lock;
use crate::domain::manifest::Manifest;
use crate::domain::resolution::{ActionResolver, Error as ResolutionError, VersionRegistry};

/// Resolve all specs in the manifest into the lock.
///
/// Returns events including skip/warning events for recoverable errors.
///
/// # Errors
///
/// Returns [`TidyError::ResolutionFailed`] if any actions could not be resolved with a strict error.
pub(super) fn update_lock<R: VersionRegistry>(
    lock: &mut Lock,
    manifest: &mut Manifest,
    resolver: &ActionResolver<'_, R>,
    workflow_shas: &HashMap<ActionSpec, CommitSha>,
    sha_index: &mut ShaIndex,
) -> Result<Vec<SyncEvent>, TidyError> {
    let mut events: Vec<SyncEvent> = Vec::new();
    let mut unresolved = Vec::new();
    let mut recoverable_count: usize = 0;

    // Build all specs in one pass: global + override versions
    let all_specs: Vec<ActionSpec> = manifest
        .specs()
        .cloned()
        .chain(manifest.all_overrides().iter().flat_map(|(id, overrides)| {
            overrides
                .iter()
                .map(move |exc| ActionSpec::new(id.clone(), exc.version.clone()))
        }))
        .collect();

    let needs_resolving = all_specs.iter().any(|spec| !lock.has(spec));

    if !needs_resolving {
        return Ok(events);
    }

    for spec in &all_specs {
        if let Err(e) = populate_lock_entry(lock, resolver, spec, workflow_shas, sha_index) {
            if e.is_recoverable() {
                events.push(SyncEvent::ResolutionSkipped {
                    spec: spec.clone(),
                    reason: e.to_string(),
                });
                recoverable_count = recoverable_count.saturating_add(1);
            } else {
                unresolved.push(format!("{spec}: {e}"));
            }
        }
    }

    if recoverable_count > 0 {
        events.push(SyncEvent::RecoverableWarning {
            count: recoverable_count,
        });
    }

    if !unresolved.is_empty() {
        return Err(TidyError::ResolutionFailed {
            count: unresolved.len(),
            specs: unresolved.join("\n  "),
        });
    }

    Ok(events)
}

/// Resolve a single spec into the lock if missing, then populate version/specifier fields.
///
/// Returns `Ok(())` on success or when no population was needed.
/// Returns `Err(ResolutionError)` if resolution fails.
fn populate_lock_entry<R: VersionRegistry>(
    lock: &mut Lock,
    resolver: &ActionResolver<'_, R>,
    spec: &ActionSpec,
    workflow_shas: &HashMap<ActionSpec, CommitSha>,
    sha_index: &mut ShaIndex,
) -> Result<(), ResolutionError> {
    let needs_population = !lock.is_complete(spec);

    if !needs_population {
        return Ok(());
    }

    if !lock.has(spec) {
        let result = if let Some(sha) = workflow_shas.get(spec) {
            resolver
                .resolve_from_sha(&spec.id, sha, sha_index)
                .or_else(|_| resolver.resolve(spec))
        } else {
            resolver.resolve(spec)
        };

        match result {
            Ok(action) => {
                let resolved_version = action.specifier.to_lookup_tag();
                lock.set(
                    spec,
                    crate::domain::action::identity::Version::from(resolved_version.as_str()),
                    action.commit,
                );
            }
            Err(e) => {
                return Err(e);
            }
        }
    }

    Ok(())
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "tests use unwrap, indexing, and other patterns freely"
)]
mod tests {
    use super::*;
    use crate::domain::action::identity::{ActionId, CommitDate, CommitSha, Version};
    use crate::domain::action::spec::Spec as ActionSpec;
    use crate::domain::action::specifier::Specifier;
    use crate::domain::action::tag_selection::ShaIndex;
    use crate::domain::action::uses_ref::RefType;
    use crate::domain::lock::Lock;
    use crate::domain::manifest::Manifest;
    use crate::domain::resolution::testutil::FakeRegistry;
    use crate::domain::resolution::{
        ActionResolver, Error as ResolutionError, ResolvedRef, ShaDescription, VersionRegistry,
    };

    // ---------------------------------------------------------------------------
    // Registry helpers
    // ---------------------------------------------------------------------------

    /// Registry where `actions/checkout` fails with `AuthRequired` but all other actions resolve.
    #[derive(Clone)]
    struct MixedRegistry;
    impl VersionRegistry for MixedRegistry {
        fn lookup_sha(
            &self,
            id: &ActionId,
            _version: &Version,
        ) -> Result<ResolvedRef, ResolutionError> {
            if id.as_str() == "actions/checkout" {
                Err(ResolutionError::AuthRequired)
            } else {
                Ok(ResolvedRef::new(
                    CommitSha::from("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
                    id.base_repo(),
                    Some(RefType::Tag),
                    CommitDate::from("2026-01-01T00:00:00Z"),
                ))
            }
        }
        fn tags_for_sha(
            &self,
            _id: &ActionId,
            _sha: &CommitSha,
        ) -> Result<Vec<Version>, ResolutionError> {
            Err(ResolutionError::AuthRequired)
        }
        fn all_tags(&self, _id: &ActionId) -> Result<Vec<Version>, ResolutionError> {
            Err(ResolutionError::AuthRequired)
        }
        fn describe_sha(
            &self,
            _id: &ActionId,
            _sha: &CommitSha,
        ) -> Result<ShaDescription, ResolutionError> {
            Err(ResolutionError::AuthRequired)
        }
    }

    fn make_manifest_with(action: &str, version: &str) -> Manifest {
        let mut m = Manifest::default();
        m.set(ActionId::from(action), Specifier::from_v1(version));
        m
    }

    // ---------------------------------------------------------------------------
    // SHA-first resolution
    // ---------------------------------------------------------------------------

    /// SHA-first: workflow SHA is used directly; registry only provides metadata.
    #[test]
    fn lock_resolves_from_workflow_sha_first() {
        let workflow_sha = "cccccccccccccccccccccccccccccccccccccccc";
        let mut manifest = make_manifest_with("actions/checkout", "v4");
        let mut lock = Lock::default();
        let key = ActionSpec::new(ActionId::from("actions/checkout"), Specifier::from_v1("v4"));
        let mut workflow_shas = HashMap::new();
        workflow_shas.insert(key.clone(), CommitSha::from(workflow_sha));

        let registry = FakeRegistry::new().fail_tags();
        let resolver = ActionResolver::new(&registry);
        let mut sha_index = ShaIndex::new();
        update_lock(
            &mut lock,
            &mut manifest,
            &resolver,
            &workflow_shas,
            &mut sha_index,
        )
        .unwrap();

        let (_, commit) = lock.get(&key).expect("lock entry must exist");
        assert_eq!(
            commit.sha.as_str(),
            workflow_sha,
            "SHA must come from workflow (SHA-first)"
        );
    }

    /// SHA-first: most specific tag from registry is stored as lock version.
    #[test]
    fn sha_first_lock_uses_workflow_sha_and_most_specific_version() {
        let workflow_sha = "6d1e696000000000000000000000000000000000";
        let mut manifest = make_manifest_with("jdx/mise-action", "v3");
        let mut lock = Lock::default();
        let key = ActionSpec::new(ActionId::from("jdx/mise-action"), Specifier::from_v1("v3"));
        let mut workflow_shas = HashMap::new();
        workflow_shas.insert(key.clone(), CommitSha::from(workflow_sha));

        let registry = FakeRegistry::new().with_sha_tags(
            "jdx/mise-action",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            vec!["v3", "v3.6", "v3.6.1"],
        );
        let resolver = ActionResolver::new(&registry);
        let mut sha_index = ShaIndex::new();
        update_lock(
            &mut lock,
            &mut manifest,
            &resolver,
            &workflow_shas,
            &mut sha_index,
        )
        .unwrap();

        let (resolution, commit) = lock.get(&key).expect("lock entry must exist");
        assert_eq!(
            commit.sha.as_str(),
            workflow_sha,
            "SHA must be from workflow"
        );
        assert_eq!(
            resolution.version.as_str(),
            "v3.6.1",
            "version must be most specific tag"
        );
    }

    /// Registry fallback: when no workflow SHA is present, registry provides the SHA.
    #[test]
    fn version_ref_falls_back_to_registry_resolution() {
        let registry_sha = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
        let mut manifest = make_manifest_with("actions/checkout", "v4");
        let mut lock = Lock::default();
        let key = ActionSpec::new(ActionId::from("actions/checkout"), Specifier::from_v1("v4"));
        let workflow_shas = HashMap::new(); // no SHA in workflow

        let registry = FakeRegistry::new().with_fixed_sha(registry_sha).fail_tags();
        let resolver = ActionResolver::new(&registry);
        let mut sha_index = ShaIndex::new();
        update_lock(
            &mut lock,
            &mut manifest,
            &resolver,
            &workflow_shas,
            &mut sha_index,
        )
        .unwrap();

        let (_, commit) = lock.get(&key).expect("lock entry must exist");
        assert_eq!(
            commit.sha.as_str(),
            registry_sha,
            "SHA must come from registry when no workflow SHA"
        );
    }

    // ---------------------------------------------------------------------------
    // Recoverable errors
    // ---------------------------------------------------------------------------

    /// Recoverable `AuthRequired` errors are skipped; other actions still resolve.
    #[test]
    fn update_lock_recoverable_errors_are_skipped() {
        let mut manifest = Manifest::default();
        manifest.set(ActionId::from("actions/checkout"), Specifier::from_v1("v4"));
        manifest.set(
            ActionId::from("actions/setup-node"),
            Specifier::from_v1("v4"),
        );
        let mut lock = Lock::default();
        let workflow_shas = HashMap::new();

        let resolver = ActionResolver::new(&MixedRegistry);
        let mut sha_index = ShaIndex::new();
        // Should not error — checkout is recoverable (AuthRequired), setup-node succeeds
        update_lock(
            &mut lock,
            &mut manifest,
            &resolver,
            &workflow_shas,
            &mut sha_index,
        )
        .unwrap();

        let setup_node_key = ActionSpec::new(
            ActionId::from("actions/setup-node"),
            Specifier::from_v1("v4"),
        );
        assert!(
            lock.get(&setup_node_key).is_some(),
            "setup-node must be resolved"
        );

        let checkout_key =
            ActionSpec::new(ActionId::from("actions/checkout"), Specifier::from_v1("v4"));
        assert!(
            lock.get(&checkout_key).is_none(),
            "checkout must be skipped (AuthRequired)"
        );
    }
}
