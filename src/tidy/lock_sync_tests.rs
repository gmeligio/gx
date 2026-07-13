// Unit tests for lock synchronization — SHA-first resolution, recoverable
// errors, and the manifest-range-authoritative reconciliation (#95).

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
        entry.version_label(),
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

/// A pinned SHA tagged `v6.0.2` under manifest range `^5` is re-resolved within
/// range, never recorded as `v6.0.2`.
#[test]
fn out_of_range_pinned_sha_is_reresolved_within_range() {
    let workflow_sha = "6d1e696000000000000000000000000000000000";
    let mut manifest = make_manifest_with("actions/checkout", "v5"); // → ^5
    let mut lock = Lock::default();
    let key = ActionSpec::new(ActionId::from("actions/checkout"), Specifier::from_v1("v5"));
    let mut workflow_shas = HashMap::new();
    workflow_shas.insert(key.clone(), CommitSha::from(workflow_sha));

    // The pinned SHA only has v6 tags (out of range), so tidy re-resolves ^5,
    // whose lookup tag is v5.
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
    let version = entry.version_label();
    assert_ne!(
        version, "v6.0.2",
        "out-of-range tag v6.0.2 must not be recorded under ^5"
    );
    assert_eq!(
        version, "v5",
        "resolved version must be re-resolved within the ^5 range"
    );
}

/// Sub-major violation: `~1.15.2` does not admit `v1.16.0`.
#[test]
fn out_of_range_pinned_sha_sub_major_is_reresolved() {
    let workflow_sha = "6d1e696000000000000000000000000000000000";
    let mut manifest = make_manifest_with("some/action", "v1.15.2"); // → ~1.15.2
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
    let version = entry.version_label();
    assert_ne!(
        version, "v1.16.0",
        "out-of-range tag v1.16.0 must not be recorded under ~1.15.2"
    );
    assert_eq!(
        version, "v1.15.2",
        "resolved version must be re-resolved within the ~1.15.2 range"
    );
}

/// An out-of-range pin emits a `PinOutOfRange` event naming the rejected tag.
#[test]
fn out_of_range_pin_emits_event() {
    let workflow_sha = "6d1e696000000000000000000000000000000000";
    let mut manifest = make_manifest_with("actions/checkout", "v5");
    let mut lock = Lock::default();
    let key = ActionSpec::new(ActionId::from("actions/checkout"), Specifier::from_v1("v5"));
    let mut workflow_shas = HashMap::new();
    workflow_shas.insert(key, CommitSha::from(workflow_sha));

    let registry = FakeRegistry::new().with_sha_tags(
        "actions/checkout",
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        vec!["v6", "v6.0", "v6.0.2"],
    );
    let resolver = ActionResolver::new(&registry);
    let mut sha_index = ShaIndex::new();
    let events = update_lock(
        &mut lock,
        &mut manifest,
        &resolver,
        &workflow_shas,
        &mut sha_index,
    )
    .unwrap();

    let has_event = events.iter().any(|e| {
        matches!(
            e,
            SyncEvent::PinOutOfRange { rejected, .. } if rejected.as_str() == "v6.0.2"
        )
    });
    assert!(
        has_event,
        "expected a PinOutOfRange event for v6.0.2, got: {events:?}"
    );
}

/// An in-range pin resolves via SHA-first and emits no `PinOutOfRange` event.
#[test]
fn in_range_pin_emits_no_event() {
    let workflow_sha = "6d1e696000000000000000000000000000000000";
    let mut manifest = make_manifest_with("actions/checkout", "v5");
    let mut lock = Lock::default();
    let key = ActionSpec::new(ActionId::from("actions/checkout"), Specifier::from_v1("v5"));
    let mut workflow_shas = HashMap::new();
    workflow_shas.insert(key.clone(), CommitSha::from(workflow_sha));

    // In-range tags: SHA-first keeps v5.4.0, no re-resolution.
    let registry = FakeRegistry::new().with_sha_tags(
        "actions/checkout",
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        vec!["v5", "v5.4", "v5.4.0"],
    );
    let resolver = ActionResolver::new(&registry);
    let mut sha_index = ShaIndex::new();
    let events = update_lock(
        &mut lock,
        &mut manifest,
        &resolver,
        &workflow_shas,
        &mut sha_index,
    )
    .unwrap();

    assert!(
        !events
            .iter()
            .any(|e| matches!(e, SyncEvent::PinOutOfRange { .. })),
        "in-range pin must not emit PinOutOfRange, got: {events:?}"
    );
    assert_eq!(
        lock.get(&key).expect("entry").version_label(),
        "v5.4.0",
        "in-range SHA-first version must be kept"
    );
}

/// Init derives `^6` from a `# v6` pin, so the SHA's `v6.0.2` tag already fits
/// the range: it is kept, with no re-resolution and no event.
#[test]
fn init_derived_specifier_keeps_sha_version() {
    let workflow_sha = "6d1e696000000000000000000000000000000000";
    let mut manifest = make_manifest_with("actions/checkout", "v6"); // → ^6
    let mut lock = Lock::default();
    let key = ActionSpec::new(ActionId::from("actions/checkout"), Specifier::from_v1("v6"));
    let mut workflow_shas = HashMap::new();
    workflow_shas.insert(key.clone(), CommitSha::from(workflow_sha));

    let registry = FakeRegistry::new().with_sha_tags(
        "actions/checkout",
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        vec!["v6", "v6.0", "v6.0.2"],
    );
    let resolver = ActionResolver::new(&registry);
    let mut sha_index = ShaIndex::new();
    let events = update_lock(
        &mut lock,
        &mut manifest,
        &resolver,
        &workflow_shas,
        &mut sha_index,
    )
    .unwrap();

    assert!(
        !events
            .iter()
            .any(|e| matches!(e, SyncEvent::PinOutOfRange { .. })),
        "derived specifier must not trip the range check, got: {events:?}"
    );
    assert_eq!(
        lock.get(&key).expect("entry").version_label(),
        "v6.0.2",
        "SHA-first version (v6.0.2) satisfies derived ^6 and is kept"
    );
}
