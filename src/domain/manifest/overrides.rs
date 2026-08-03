use crate::domain::action::identity::ActionId;
use crate::domain::action::spec::Spec;
use crate::domain::action::specifier::Specifier;
use crate::domain::file::actions::{ActionSet as WorkflowActionSet, Located as LocatedAction};
use crate::domain::file::site::{Id, JobId, Slot, StepIndex, WorkflowPath};
use std::collections::HashSet;

/// A version override for a specific file location.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionOverride {
    /// Relative path from repo root, e.g. ".github/workflows/deploy.yml".
    pub workflow: WorkflowPath,
    /// Job id, if scoped to a job. Always `None` for a composite action's step.
    pub job: Option<JobId>,
    /// 0-based step index, if scoped to a step. Requires a job in a workflow;
    /// stands alone in a composite action, which has none.
    pub step: Option<StepIndex>,
    /// The specifier to use at this location.
    pub version: Specifier,
}

/// Resolve the effective specifier for an action at a given site.
///
/// Resolution order, most specific first: job-step, job, file-step, file, then the global
/// default (returned as `None` — the caller falls back to it). File-step addresses a
/// composite action's step, which belongs to no job.
///
/// The tiers are disjoint by construction: [`Slot`] gives a site exactly one shape, so a
/// composite step can never be tested against a job-bearing tier.
#[must_use]
pub fn resolve_version<'ovr>(
    overrides: &'ovr [ActionOverride],
    site: &Id,
) -> Option<&'ovr Specifier> {
    let in_file = |exc: &&ActionOverride| exc.workflow == site.file;

    match &site.slot {
        Slot::WorkflowStep { job, step } => {
            if let Some(exc) = overrides
                .iter()
                .filter(in_file)
                .find(|exc| exc.job.as_ref() == Some(job) && exc.step == Some(*step))
            {
                return Some(&exc.version);
            }
            if let Some(exc) = overrides
                .iter()
                .filter(in_file)
                .find(|exc| exc.job.as_ref() == Some(job) && exc.step.is_none())
            {
                return Some(&exc.version);
            }
        }
        Slot::WorkflowJob { job } => {
            if let Some(exc) = overrides
                .iter()
                .filter(in_file)
                .find(|exc| exc.job.as_ref() == Some(job) && exc.step.is_none())
            {
                return Some(&exc.version);
            }
        }
        Slot::CompositeStep { step } => {
            if let Some(exc) = overrides
                .iter()
                .filter(in_file)
                .find(|exc| exc.job.is_none() && exc.step == Some(*step))
            {
                return Some(&exc.version);
            }
        }
    }

    overrides
        .iter()
        .filter(in_file)
        .find(|exc| exc.job.is_none() && exc.step.is_none())
        .map(|exc| &exc.version)
}

/// The override that addresses exactly this site, if one exists.
///
/// Unlike [`resolve_version`], this does not walk the precedence tiers — it asks whether
/// an override names this precise site, which is what `sync` needs to avoid writing a
/// duplicate and what `prune_stale` needs to know an override still has a target.
fn addresses(exc: &ActionOverride, site: &Id) -> bool {
    if exc.workflow != site.file {
        return false;
    }
    match &site.slot {
        Slot::WorkflowStep { job, step } => {
            exc.job.as_ref() == Some(job) && exc.step == Some(*step)
        }
        Slot::WorkflowJob { job } => exc.job.as_ref() == Some(job) && exc.step.is_none(),
        Slot::CompositeStep { step } => exc.job.is_none() && exc.step == Some(*step),
    }
}

/// Build the override that names exactly this site.
fn override_for(site: &Id, version: Specifier) -> ActionOverride {
    let (job, step) = scope_of(site);
    ActionOverride {
        workflow: site.file.clone(),
        job,
        step,
        version,
    }
}

/// A site's scope in the `(job, step)` shape `ActionOverride` and the manifest use.
///
/// `ActionOverride` stays an `Option` pair because it is a *selector*: a job-scoped
/// override names every step in that job. [`Id`] is a single address. The two are
/// deliberately different shapes; this converts one to the other.
fn scope_of(site: &Id) -> (Option<JobId>, Option<StepIndex>) {
    match &site.slot {
        Slot::WorkflowStep { job, step } => (Some(job.clone()), Some(*step)),
        Slot::WorkflowJob { job } => (Some(job.clone()), None),
        Slot::CompositeStep { step } => (None, Some(*step)),
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
                // An override lives while one scanned location matches every field it
                // names. Job and step must match the *same* location, so a composite
                // override — step, no job — is checked against job-less locations.
                .filter(|exc| {
                    if !live_workflows.contains(exc.workflow.as_str()) {
                        return false;
                    }
                    if exc.job.is_none() && exc.step.is_none() {
                        return true;
                    }
                    located.iter().any(|a| {
                        let (job, step) = scope_of(&a.site);
                        a.site.file == exc.workflow
                            && job == exc.job
                            && (exc.step.is_none() || step == exc.step)
                    })
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
