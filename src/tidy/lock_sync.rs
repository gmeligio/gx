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
        match populate_lock_entry(lock, resolver, spec, workflow_shas, sha_index) {
            Ok(Some(event)) => events.push(event),
            Ok(None) => {}
            Err(e) => {
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

/// Resolve a single spec into the lock if its entry is missing or incomplete.
///
/// Returns `Ok(Some(event))` when an out-of-range pinned SHA was re-resolved within
/// range, `Ok(None)` on ordinary success or when no resolution was needed, and
/// `Err(ResolutionError)` if resolution fails.
fn populate_lock_entry<R: VersionRegistry>(
    lock: &mut Lock,
    resolver: &ActionResolver<'_, R>,
    spec: &ActionSpec,
    workflow_shas: &HashMap<ActionSpec, CommitSha>,
    sha_index: &mut ShaIndex,
) -> Result<Option<SyncEvent>, ResolutionError> {
    // An existing entry is left untouched, even if incomplete; only unseen
    // specs are resolved here.
    if lock.has(spec) {
        return Ok(None);
    }

    // SHA-first: derive the version from the pinned SHA's own tag. Absent a
    // workflow SHA or when the SHA can't be described, resolve version-first.
    let sha_first = workflow_shas
        .get(spec)
        .and_then(|sha| resolver.resolve_from_sha(&spec.id, sha, sha_index).ok());
    let Some(action) = sha_first else {
        let action = resolver.resolve(spec)?;
        lock.set(spec, action.version, action.commit);
        return Ok(None);
    };

    // The manifest range is authoritative over the version label. A pinned SHA
    // whose tag falls outside the declared range is a stale preference:
    // re-resolve within the range (the pnpm/uv/Cargo model). A tagless SHA
    // resolves to the bare commit as its version, which no range can match and
    // which carries no label to constrain — skip it. Non-semver specifiers are
    // exempt via `matches_version`.
    if action.version.is_sha() || spec.specifier.matches_version(&action.version) {
        lock.set(spec, action.version, action.commit);
        return Ok(None);
    }

    let reresolved = resolver.resolve(spec)?;
    let event = SyncEvent::PinOutOfRange {
        spec: spec.clone(),
        rejected: action.version,
        resolved: reresolved.version.clone(),
    };
    lock.set(spec, reresolved.version, reresolved.commit);
    Ok(Some(event))
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "tests use unwrap, indexing, and other patterns freely"
)]
#[path = "lock_sync_tests.rs"]
mod tests;
