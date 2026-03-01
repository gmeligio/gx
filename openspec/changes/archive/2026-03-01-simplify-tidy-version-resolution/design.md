## Context

The `tidy::run()` function orchestrates manifest, lock, and workflow synchronization. It currently has interleaved concerns: two redundant workflow scan passes, an action-level SHA map (`sha_for`) that leaks stale SHAs into lock resolution, a `Version::should_be_replaced_by` method that trusts workflow comments over the registry, and a SHA correction phase that treats workflow state as authoritative over the manifest.

The manifest is the user-controlled configuration file. When it exists, its versions are the user's intent. The workflows hold the deployed state (SHA pins with version comments) that tidy updates to match the manifest.

## Goals / Non-Goals

**Goals:**
- Manifest versions are never overwritten by workflow state for existing actions
- SHA correction applies only when adding new actions to the manifest
- Lock resolution uses the registry exclusively (no workflow SHA leaking)
- Single scan pass for workflows
- SHA-only manifest versions produce clean workflow output (no `# SHA` comment)
- Simpler code that's easier to test in isolation

**Non-Goals:**
- Changing how the `upgrade` command works
- Changing the lock entry format or reconciliation logic
- Changing the override mechanism (minority version recording)
- Changing how `WorkflowUpdater` writes files (only what string it receives)

## Decisions

### 1. Single scan pass, derived aggregate

**Decision**: Remove `WorkflowScanner::scan_all()`. Scan once via `scan_all_located()` and derive `WorkflowActionSet` from the located actions.

**Rationale**: Both scan methods iterate all workflows and call `extract_actions` + `interpret`. The `WorkflowActionSet` is a statistical aggregate (versions, counts) that can be computed from `Vec<LocatedAction>`. Two passes do the same I/O work for no benefit.

**Change**: Add `WorkflowActionSet::from_located(&[LocatedAction])` constructor. Remove `WorkflowScanner` trait and `FileWorkflowScanner::scan_all()`. The `scan_all_located` method moves to `WorkflowScanner` as the sole scan method (along with `find_workflow_paths`). Update callers (`tidy::run`, `app::lint`).

### 2. Remove `shas` from `WorkflowActionSet`

**Decision**: Drop the `shas: HashMap<ActionId, CommitSha>` field and `sha_for()` method entirely.

**Rationale**: `sha_for` maps one SHA per action (first-seen wins), regardless of version. This is wrong in multi-version scenarios and leaks stale SHAs into three places in tidy: the SHA correction phase, the early-exit check, and the `sha_override` parameter in `populate_lock_entry`. All three uses are buggy — they assume the workflow SHA corresponds to the manifest version.

**Alternative considered**: Make `sha_for` version-aware (`HashMap<(ActionId, Version), CommitSha>`). Rejected because the only legitimate use is during new-action addition, where the SHA is readily available from the `LocatedAction` that provided the dominant version. A global map is unnecessary.

### 3. Scope SHA correction to new actions only

**Decision**: SHA correction (`registry.tags_for_sha`) runs only when adding a new action to the manifest (the "Add missing" phase). It does not apply to existing manifest entries.

**Rationale**: For new actions, the version comes from the workflow and might have a wrong comment tag (e.g., `@abc123 # v4` but abc123 is actually v3.6.0). SHA correction validates this. For existing actions, the manifest version is the user's intent — "correcting" it from a stale workflow SHA inverts the source of truth.

**Implementation**: During the "Add missing" phase, after selecting the dominant version, find a `LocatedAction` with that version that also has a SHA. If found, call `correct_version(id, sha, dominant_version)` to validate. This is version-aware (the SHA corresponds to the version being corrected).

### 4. Replace `should_be_replaced_by` with registry-based SHA-to-tag upgrade

**Decision**: Remove `Version::should_be_replaced_by()`. Add a new "upgrade SHA versions" step in manifest sync that uses `registry.tags_for_sha()`.

**Rationale**: `should_be_replaced_by` compares the manifest (SHA) against the workflow comment (tag) — trusting the comment. The registry is the authoritative source for what tags a SHA points to. The registry-based approach is accurate, independent of workflow state, and gracefully degrades (no token → SHA stays).

**Implementation**: After removing unused and adding new actions, iterate existing manifest entries. For any entry where `version.is_sha()`, look up `registry.tags_for_sha(id, CommitSha::from(version))`. If tags are found, upgrade the manifest to the best tag. If no token or no tags, keep the SHA unchanged.

### 5. Remove `sha_override` from `populate_lock_entry`

**Decision**: `populate_lock_entry` no longer accepts an `sha_override` parameter. Lock entries are populated exclusively from registry resolution or existing cached entries.

**Rationale**: The `sha_override` was the workflow's SHA substituted into the resolved lock entry, potentially replacing the correct SHA for the manifest's version with a stale one from the workflow. Without it, lock resolution is self-contained: if the lock has a complete entry, skip; if not, resolve from registry.

### 6. Clean workflow output for SHA-only versions

**Decision**: When the resolved version is a raw SHA (no tag), write `@SHA` in the workflow without a `# SHA` comment.

**Rationale**: Writing `@abc123 # abc123` is redundant and ugly. The comment is meant for human-readable tag names. When there is no tag, the comment adds no value. The `build_file_update_map` function changes from always formatting `"{sha} # {version}"` to omitting the comment when `version.is_sha()`.

### 7. Simplified tidy flow

The new `run()` function follows five sequential phases with clear responsibilities:

```
1. SCAN
   located = scan_all_located()
   action_set = WorkflowActionSet::from_located(&located)

2. SYNC MANIFEST
   a. Remove unused (in manifest, not in workflows)
   b. Add new (in workflows, not in manifest)
      - dominant version + SHA correction for new actions only
   c. Upgrade SHA versions to tags via registry (existing manifest entries)

3. SYNC OVERRIDES
   - Record overrides for minority versions
   - Prune stale overrides

4. RESOLVE LOCK
   - For each (action, version) in manifest (global + overrides):
     if lock has complete entry → skip
     else → resolve from registry
   - Prune unused lock entries

5. UPDATE WORKFLOWS
   - Per location: resolve version via manifest hierarchy → look up SHA from lock
   - Format: "SHA # version" or just "SHA" if version is a raw SHA
```

## Risks / Trade-offs

- **[More registry calls]** → Removing `sha_override` means the registry is always used for new lock entries instead of shortcutting with the workflow SHA. In practice this is negligible: entries are only resolved when missing from the lock, and the registry is already called for version correction. The lock cache prevents repeated resolution across tidy runs.

- **[SHA versions stay longer without a token]** → Without `should_be_replaced_by`, a manifest SHA can only be upgraded to a tag via the registry. Users without a `GITHUB_TOKEN` keep the SHA until they set one. This is the correct behavior — better to keep a SHA than to trust a potentially wrong workflow comment.

- **[Trait change is breaking for test mocks]** → Removing `WorkflowScanner` and consolidating into `WorkflowScannerLocated` (renamed to `WorkflowScanner`) changes the trait boundary. Test mocks implementing the old trait need updating. Straightforward mechanical change.
