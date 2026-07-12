use std::collections::HashMap;

use super::Error as TidyError;
use crate::domain::action::identity::CommitSha;
use crate::domain::action::spec::Spec as ActionSpec;
use crate::domain::action::tag_selection::ShaIndex;
use crate::domain::action::uses_ref::RefType;
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
                // The manifest range is authoritative over the version label. A
                // pinned SHA whose *tag* falls outside the declared range is a
                // stale preference: discard the SHA-first version and re-resolve
                // within the range (the pnpm/uv/Cargo model). Only tag-backed
                // resolutions are checked — a SHA with no tags resolves to the
                // bare commit (`RefType::Commit`, version == SHA), which carries
                // no version label for the range to constrain. Non-semver
                // specifiers are exempt via `matches_version`.
                .and_then(|action| {
                    let is_tag = action.commit.ref_type != Some(RefType::Commit);
                    if is_tag && !spec.specifier.matches_version(&action.version) {
                        resolver.resolve(spec)
                    } else {
                        Ok(action)
                    }
                })
                .or_else(|_| resolver.resolve(spec))
        } else {
            resolver.resolve(spec)
        };

        match result {
            Ok(action) => {
                lock.set(spec, action.version, action.commit);
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
    use crate::domain::action::resolved::Commit;
    use crate::domain::action::spec::Spec as ActionSpec;
    use crate::domain::action::specifier::Specifier;
    use crate::domain::action::tag_selection::ShaIndex;
    use crate::domain::action::uses_ref::RefType;
    use crate::domain::lock::Lock;
    use crate::domain::manifest::Manifest;
    use crate::domain::resolution::testutil::FakeRegistry;
    use crate::domain::resolution::{
        ActionResolver, Error as ResolutionError, ShaDescription, VersionRegistry,
    };

    // ---------------------------------------------------------------------------
    // Registry helpers
    // ---------------------------------------------------------------------------

    /// Registry where `actions/checkout` fails with `AuthRequired` but all other actions resolve.
    #[derive(Clone)]
    struct MixedRegistry;
    impl VersionRegistry for MixedRegistry {
        fn lookup_sha(&self, id: &ActionId, _version: &Version) -> Result<Commit, ResolutionError> {
            if id.as_str() == "actions/checkout" {
                Err(ResolutionError::AuthRequired)
            } else {
                Ok(Commit {
                    sha: CommitSha::from("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
                    repository: id.base_repo(),
                    ref_type: Some(RefType::Tag),
                    date: CommitDate::from("2026-01-01T00:00:00Z"),
                })
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

        let entry = lock.get(&key).expect("lock entry must exist");
        assert_eq!(
            entry.commit.sha.as_str(),
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

        let entry = lock.get(&key).expect("lock entry must exist");
        assert_eq!(
            entry.commit.sha.as_str(),
            workflow_sha,
            "SHA must be from workflow"
        );
        assert_eq!(
            entry.version.as_str(),
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

        let entry = lock.get(&key).expect("lock entry must exist");
        assert_eq!(
            entry.commit.sha.as_str(),
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

    // ---------------------------------------------------------------------------
    // Manifest range is authoritative over an out-of-range pinned SHA (#95)
    // ---------------------------------------------------------------------------

    /// A workflow SHA whose most-specific tag (`v6.0.2`) violates the manifest
    /// range (`^5`) must NOT be recorded as-is. The pin is a stale preference:
    /// tidy re-resolves the version within the declared range.
    #[test]
    fn out_of_range_pinned_sha_is_reresolved_within_range() {
        let workflow_sha = "6d1e696000000000000000000000000000000000";
        // `from_v1("v5")` → specifier `^5`.
        let mut manifest = make_manifest_with("actions/checkout", "v5");
        let mut lock = Lock::default();
        let key = ActionSpec::new(ActionId::from("actions/checkout"), Specifier::from_v1("v5"));
        let mut workflow_shas = HashMap::new();
        workflow_shas.insert(key.clone(), CommitSha::from(workflow_sha));

        // The SHA's tags are all v6 (out of range for ^5). The version-first
        // fallback resolves the `^5` → `v5` lookup tag instead.
        let registry = FakeRegistry::new().with_sha_tags(
            "actions/checkout",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            vec!["v6", "v6.0", "v6.0.2"],
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

        let entry = lock.get(&key).expect("lock entry must exist");
        let version = entry.version.as_str();
        assert_ne!(
            version, "v6.0.2",
            "out-of-range tag v6.0.2 must not be recorded under ^5"
        );
        // The version-first fallback resolves the `^5` → `v5` lookup tag.
        assert_eq!(
            version, "v5",
            "resolved version must be re-resolved within the ^5 range"
        );
    }

    /// Sub-major violation: `~1.15.2` does not admit `v1.16.0`.
    #[test]
    fn out_of_range_pinned_sha_sub_major_is_reresolved() {
        let workflow_sha = "6d1e696000000000000000000000000000000000";
        // `from_v1("v1.15.2")` → specifier `~1.15.2`.
        let mut manifest = make_manifest_with("some/action", "v1.15.2");
        let mut lock = Lock::default();
        let key = ActionSpec::new(ActionId::from("some/action"), Specifier::from_v1("v1.15.2"));
        let mut workflow_shas = HashMap::new();
        workflow_shas.insert(key.clone(), CommitSha::from(workflow_sha));

        let registry = FakeRegistry::new().with_sha_tags(
            "some/action",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            vec!["v1.16", "v1.16.0"],
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

        let entry = lock.get(&key).expect("lock entry must exist");
        let version = entry.version.as_str();
        assert_ne!(
            version, "v1.16.0",
            "out-of-range tag v1.16.0 must not be recorded under ~1.15.2"
        );
        // The version-first fallback resolves the `~1.15.2` → `v1.15.2` lookup tag.
        assert_eq!(
            version, "v1.15.2",
            "resolved version must be re-resolved within the ~1.15.2 range"
        );
    }
}
