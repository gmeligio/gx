## 1. Fix tag selection

- [x] 1.1 Rename `select_best_tag` to `select_most_specific_tag` in `src/domain/resolution.rs` and update the sort to prefer more components (descending len) then highest version. Update the public export in `src/domain/mod.rs`.
- [x] 1.2 Update all callers (`correct_version`, `upgrade_sha_versions_to_tags`, `refine_version`) to use the renamed function.
- [x] 1.3 Fix existing tests for the renamed function: `test_select_best_tag_prefers_major_over_patch` and `test_select_best_tag_prefers_major_over_minor` now expect most specific (patch/minor wins over major). Add a test for `[v3, v3.6, v3.6.1]` → `v3.6.1`.

## 2. Add resolve_from_sha to ActionResolver

- [x] 2.1 Add `resolve_from_sha(&self, id: &ActionId, sha: &CommitSha) -> Result<ResolvedAction, ResolutionError>` to `ActionResolver` in `src/domain/resolution.rs`. It calls `lookup_sha` with SHA as version for metadata, `tags_for_sha` for version, and derives `ref_type` from tag presence.
- [x] 2.2 Add unit tests for `resolve_from_sha`: SHA with tags (returns most specific version, ref_type=Tag), SHA with no tags (returns SHA as version, ref_type=Commit).

## 3. Wire workflow SHAs into lock resolution

- [x] 3.1 In `plan()` in `src/commands/tidy.rs`, build a `HashMap<LockKey, CommitSha>` from `located` actions (using the manifest version for the key). Pass it to `update_lock`.
- [x] 3.2 Update `update_lock` and `populate_lock_entry` signatures to accept `&HashMap<LockKey, CommitSha>`.
- [x] 3.3 In `populate_lock_entry`, use `resolve_from_sha` when a workflow SHA exists for the lock key, fall back to `resolve` otherwise. Remove the `refine_version` post-step.

## 4. Simplify resolve return type

- [x] 4.1 Change `ActionResolver::resolve` to return `Result<ResolvedAction, ResolutionError>` instead of `ResolutionResult`. Remove the `ResolutionResult` enum and `Corrected` variant. Update callers in `populate_lock_entry`.

## 5. Integration tests

- [x] 5.1 Add a tidy plan test: workflow has SHA-pinned action with floating tag comment (e.g., `@sha # v3`), verify lock entry uses the workflow SHA and most specific version.
- [x] 5.2 Add a tidy plan test: workflow has bare version ref (no SHA), verify lock entry falls back to registry resolution.
