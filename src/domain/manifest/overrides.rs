use crate::domain::action::identity::ActionId;
use crate::domain::action::spec::Spec;
use crate::domain::action::specifier::Specifier;
use crate::domain::file::actions::{ActionSet as WorkflowActionSet, Located as LocatedAction};
use crate::domain::file::site::{Id, Scope, Slot, WorkflowPath};
use std::collections::HashSet;

/// A version override for a set of sites.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionOverride {
    /// Relative path from repo root, e.g. ".github/workflows/deploy.yml".
    pub workflow: WorkflowPath,
    /// Which sites within that file this override applies to.
    pub scope: Scope,
    /// The specifier to use at those sites.
    pub version: Specifier,
}

/// Resolve the effective specifier for an action at a given site.
///
/// The most specific selecting override wins — step over job over file — and the global
/// default is the fallback, returned as `None` for the caller to apply.
///
/// [`Scope::precedence`] orders the tiers, so this is one pass rather than a scan per
/// tier. Ties keep the earliest entry, matching the previous first-match-wins behaviour.
#[must_use]
pub fn resolve_version<'ovr>(
    overrides: &'ovr [ActionOverride],
    site: &Id,
) -> Option<&'ovr Specifier> {
    overrides
        .iter()
        .filter(|exc| exc.workflow == site.file && exc.scope.selects(&site.slot))
        .max_by_key(|exc| exc.scope.precedence())
        .map(|exc| &exc.version)
}

/// Whether an override names exactly this site, rather than merely selecting it.
///
/// `sync` uses this to avoid writing a duplicate and `prune_stale` to tell whether an
/// override still has a target. A job-scoped override *selects* a step but does not
/// *name* it, so the two questions have different answers.
fn addresses(exc: &ActionOverride, site: &Id) -> bool {
    exc.workflow == site.file && exc.scope == scope_of(&site.slot)
}

/// Build the override that names exactly this site.
fn override_for(site: &Id, version: Specifier) -> ActionOverride {
    ActionOverride {
        workflow: site.file.clone(),
        scope: scope_of(&site.slot),
        version,
    }
}

/// The narrowest scope naming exactly this slot.
fn scope_of(slot: &Slot) -> Scope {
    match slot {
        Slot::WorkflowStep { job, step } => Scope::JobStep {
            job: job.clone(),
            step: *step,
        },
        Slot::WorkflowJob { job } => Scope::Job { job: job.clone() },
        Slot::CompositeStep { step } => Scope::CompositeStep { step: *step },
    }
}

/// Compute all lock keys needed for overrides: one per (action, version) pair.
pub fn override_lock_keys<'ovr>(
    id: &'ovr ActionId,
    overrides: &'ovr [ActionOverride],
) -> impl Iterator<Item = Spec> + 'ovr {
    overrides
        .iter()
        .map(move |exc| Spec::new(id.clone(), exc.version.clone()))
}

/// Ensure overrides exist for every located step whose version differs from the manifest
/// global, **only when** multiple distinct versions of that action appear across workflows.
///
/// When only one version appears in workflows, no override is created.
#[expect(clippy::implicit_hasher, reason = "callers always use std HashMap")]
pub fn sync(
    actions_overrides: &mut std::collections::HashMap<ActionId, Vec<ActionOverride>>,
    actions_global: &std::collections::HashMap<ActionId, Spec>,
    located: &[LocatedAction],
    action_set: &WorkflowActionSet,
) {
    for action in located {
        let version_count = action_set.versions_for(&action.action.id).count();
        if version_count <= 1 {
            continue;
        }

        let global_specifier = match actions_global.get(&action.action.id) {
            Some(spec) => spec.specifier.clone(),
            None => continue,
        };

        let action_specifier = Specifier::from_v1(action.action.reference.label());

        if action_specifier == global_specifier {
            continue;
        }

        let empty: &[ActionOverride] = &[];
        let existing_overrides = actions_overrides
            .get(&action.action.id)
            .map_or(empty, Vec::as_slice);

        let already_covered = existing_overrides
            .iter()
            .any(|o| addresses(o, &action.site));

        if !already_covered {
            actions_overrides
                .entry(action.action.id.clone())
                .or_default()
                .push(override_for(&action.site, action_specifier));
        }
    }
}

/// Remove override entries whose referenced workflow/job/step no longer exists in the
/// scanned set.
#[expect(clippy::implicit_hasher, reason = "callers always use std HashMap")]
pub fn prune_stale(
    actions_overrides: &mut std::collections::HashMap<ActionId, Vec<ActionOverride>>,
    located: &[LocatedAction],
) {
    let live_workflows: HashSet<&str> = located.iter().map(|a| a.site.file.as_str()).collect();

    let updates: Vec<(ActionId, Vec<ActionOverride>)> = actions_overrides
        .iter()
        .map(|(id, overrides)| {
            let pruned: Vec<ActionOverride> = overrides
                .iter()
                // An override lives while it still selects a scanned site. A file-scoped
                // one lives as long as the file does — it names no position that could go
                // away.
                //
                // Note this asks only whether *some* site is selected, not whether that
                // site still holds the action the override is keyed on: an override can
                // survive here and yet apply to a different action than the user wrote it
                // for. See #163.
                .filter(|exc| {
                    if !live_workflows.contains(exc.workflow.as_str()) {
                        return false;
                    }
                    if exc.scope == Scope::File {
                        return true;
                    }
                    located
                        .iter()
                        .any(|a| a.site.file == exc.workflow && exc.scope.selects(&a.site.slot))
                })
                .cloned()
                .collect();
            (id.clone(), pruned)
        })
        .collect();

    for (id, pruned) in updates {
        if pruned.is_empty() {
            actions_overrides.remove(&id);
        } else {
            actions_overrides.insert(id, pruned);
        }
    }
}

#[cfg(test)]
#[path = "overrides_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "overrides_composite_tests.rs"]
mod composite_tests;
