## Why

`gx tidy` does not treat the manifest as the source of truth for existing action versions. When the manifest has version `v4` but workflows still have a stale SHA pointing to `v3`, the SHA correction logic in `update_lock()` "corrects" the manifest back to `v3` — overriding the user's intent. The root cause is that `sha_for()` captures an arbitrary workflow SHA per action (not per version) and uses it to override the manifest. The tidy flow is also harder to reason about than necessary: it has two redundant scan passes, interleaved concerns, and a `should_be_replaced_by` method that trusts workflow comments blindly instead of using the registry.

## What Changes

- **Fix manifest authority**: existing manifest versions are never overwritten by workflow state. The manifest is the source of truth for actions it tracks; workflows are the source of truth only for discovering new actions and removing unused ones.
- **Remove `sha_for()` from `WorkflowActionSet`**: eliminate the action-level SHA map that leaks stale workflow SHAs into lock resolution.
- **Replace `should_be_replaced_by` with registry-based SHA-to-tag upgrade**: when the manifest has a raw SHA as a version, look up tags via the registry instead of trusting workflow comments. Gracefully degrades (SHA stays if no token).
- **Remove `sha_override` from lock resolution**: `populate_lock_entry` resolves SHAs exclusively from the registry or existing lock entries, never from workflow state.
- **Merge two scan passes into one**: derive `WorkflowActionSet` from `Vec<LocatedAction>` instead of scanning workflows twice.
- **Scope SHA correction to new actions only**: when a new action is added to the manifest, use the located action's SHA (version-aware) to verify the tag via the registry. Do not apply SHA correction to existing manifest entries.
- **No comment for SHA-only versions in workflow output**: when the manifest version is a raw SHA (no tag resolved), write `@SHA` without a `# SHA` comment to avoid ugly duplication.

## Capabilities

### New Capabilities
- `manifest-authority`: the manifest-workflow reconciliation rules — which source of truth wins, when SHA correction applies, and how SHA-to-tag upgrades work

### Modified Capabilities
_(none — `lock-reconciliation` and `version-resolution` requirements are unchanged; this changes where inputs come from, not how the lock or upgrade system operates)_

## Impact

- `src/commands/tidy.rs`: rewrite of the `run()` flow, removal of `sha_override` plumbing in `update_lock`/`populate_lock_entry`, simplified `build_file_update_map` for SHA-only versions
- `src/domain/workflow_actions.rs`: remove `shas` field and `sha_for()` method from `WorkflowActionSet`, add `from_located()` constructor
- `src/domain/action.rs`: remove `Version::should_be_replaced_by()` and its tests
- `src/infrastructure/workflow.rs`: remove `scan_all()` from `FileWorkflowScanner` (keep `scan_all_located()` only), update trait impls
- `src/domain/workflow.rs`: remove `scan_all()` from `WorkflowScanner` trait or unify with `WorkflowScannerLocated`
- Test suite: update/remove tests that depend on `sha_for()`, `should_be_replaced_by`, and the old SHA correction path; add tests for the new manifest-authority semantics
