## Why

When `gx init` bootstraps from workflow files that pin actions to SHAs (e.g., `jdx/mise-action@6d1e696... # v3`), the lock resolution ignores the workflow SHA and instead resolves the manifest version tag (`v3`) via GitHub API. Since `v3` is a floating tag that maintainers move forward, this produces a lock entry with the wrong SHA (e.g., v3.6.2's commit instead of v3.6.1's). The lock's source of truth should always be the SHA — version and metadata should be derived from it, not the other way around.

## What Changes

- Lock resolution becomes SHA-first: when a workflow provides a SHA, use it directly instead of resolving the version tag via GitHub API.
- `select_best_tag` is split into two functions with clear intent: `select_broadest_tag` (fewest components, for manifest context) and `select_most_specific_tag` (most components, for lock context).
- The lock's `version` field is always populated with the most specific tag for the SHA (e.g., `v3.6.1` instead of `v3`), matching the lock-format spec.
- The `refine_version` post-step in `populate_lock_entry` is removed — version is determined during resolution, not as a separate fixup.

## Capabilities

### New Capabilities

- `sha-first-resolution`: SHA-first lock resolution path that derives version and metadata from a known SHA rather than resolving a version tag to find a SHA.

### Modified Capabilities

- `manifest-authority`: Lock resolution uses workflow SHAs when available instead of resolving exclusively via registry.
- `version-resolution`: Tag selection is split into broadest (manifest) and most-specific (lock) variants.

## Impact

- `src/domain/resolution.rs`: Split `select_best_tag`, add `resolve_from_sha` to `ActionResolver`, update `refine_version` and `correct_version` callers.
- `src/commands/tidy.rs`: Build SHA map from located actions, pass to `update_lock`/`populate_lock_entry`, remove `refine_version` post-step.
- `src/domain/mod.rs`: Update public exports.
- Existing tests for `select_best_tag`, `correct_version`, and tidy plan need updating.
