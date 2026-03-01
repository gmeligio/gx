## 1. Simplify ResolvedAction

- [x] 1.1 Remove `resolved_version` and `specifier` fields from `ResolvedAction` in `domain/action.rs`
- [x] 1.2 Remove `populate_resolved_fields` function from `domain/resolution.rs`
- [x] 1.3 Fix all compilation errors from the struct change across the crate

## 2. Add LockEntry completeness

- [x] 2.1 Add `is_complete(&self, manifest_version: &Version) -> bool` to `LockEntry` in `domain/lock.rs`
- [x] 2.2 Handle edge cases: `Some("")` treated as missing, non-semver versions (no specifier expected)
- [x] 2.3 Add unit tests for `is_complete` covering all scenarios from the lock-reconciliation spec

## 3. Update Lock::set to accept individual fields

- [x] 3.1 Add a method to `Lock` or `LockEntry` for setting version and specifier independently (so incomplete entries can be updated without full re-resolution)
- [x] 3.2 Update existing `Lock::set` to no longer expect specifier/resolved_version from `ResolvedAction`

## 4. Restructure update_lock into two phases

- [x] 4.1 Extract Phase 1 (manifest correctness): sync manifest with workflows + SHA correction using REFINE (`tags_for_sha` + `select_best_tag`)
- [x] 4.2 Extract Phase 2 (lock completeness): for each manifest spec, check `is_complete()`, then run only needed operations (RESOLVE / REFINE / DERIVE)
- [x] 4.3 Implement the DERIVE-only path: when only specifier is missing, compute `Version::specifier()` locally without network calls
- [x] 4.4 Implement the REFINE path: when version is missing, call `tags_for_sha` to populate it
- [x] 4.5 Remove the old `update_lock` function

## 5. Unify version refinement

- [x] 5.1 Rename or refactor `correct_version` to make it clear it's the REFINE operation (shared by Phase 1 manifest correction and Phase 2 lock version population)

## 6. Tests

- [x] 6.1 Add integration test: lock with missing specifier gets DERIVE'd without network call on next tidy
- [x] 6.2 Add integration test: lock with missing version gets REFINE'd on next tidy
- [x] 6.3 Add integration test: complete lock entry is skipped (no operations)
- [x] 6.4 Add integration test: brand new entry gets full RESOLVE + REFINE + DERIVE
- [x] 6.5 Update existing tidy tests to work with the new two-phase flow
- [x] 6.6 Run full test suite and fix any regressions
