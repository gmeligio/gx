## 1. Single scan pass and derived aggregate

- [x] 1.1 Add `WorkflowActionSet::from_located(&[LocatedAction])` constructor that builds `versions` and `counts` from located actions (no `shas` field)
- [x] 1.2 Remove `shas` field, `sha_for()` method, and related tests from `WorkflowActionSet`
- [x] 1.3 Merge `WorkflowScanner` and `WorkflowScannerLocated` traits into a single `WorkflowScanner` trait with `scan_all_located()` and `find_workflow_paths()`
- [x] 1.4 Remove `FileWorkflowScanner::scan_all()` method and update `scan_all_located` workflow scanner tests
- [x] 1.5 Update `tidy::run()` to use single scan pass: call `scan_all_located()` then derive `WorkflowActionSet::from_located()`
- [x] 1.6 Update `app::lint()` to use the unified scan approach

## 2. Manifest authority and SHA correction scoping

- [x] 2.1 Remove the "Update existing actions" block (lines 88-113 in tidy.rs) that calls `should_be_replaced_by`
- [x] 2.2 Remove `Version::should_be_replaced_by()` method and its 4 tests from `action.rs`
- [x] 2.3 Add SHA-to-tag upgrade step: after removing unused and adding new, iterate existing manifest entries where `version.is_sha()` and call `registry.tags_for_sha()` to upgrade to the best tag
- [x] 2.4 Scope SHA correction to the "Add missing" phase: when adding a new action, find a `LocatedAction` with the dominant version that has a SHA, then call `correct_version()` to validate the tag
- [x] 2.5 Add tests for manifest authority: manifest v4 with workflow v3 SHA stays v4 after tidy
- [x] 2.6 Add tests for SHA-to-tag upgrade: manifest SHA upgraded via registry, graceful degradation without token

## 3. Clean lock resolution

- [x] 3.1 Remove `sha_override` parameter from `populate_lock_entry` and all callers
- [x] 3.2 Remove the early-exit check `has_workflow_shas` that depends on `sha_for()`
- [x] 3.3 Simplify `update_lock()`: remove Phase 1 (SHA correction of existing actions), keep only lock completeness logic
- [x] 3.4 Add test: lock resolves from registry, not from workflow SHAs

## 4. Clean workflow output for SHA versions

- [x] 4.1 Update `build_file_update_map` to omit `# comment` when the resolved version is a raw SHA (`version.is_sha()`)
- [x] 4.2 Add test: SHA-only manifest version produces `@SHA` without trailing comment in workflow output

## 5. Update existing test suite

- [x] 5.1 Update `test_tidy_records_minority_version_as_override_and_does_not_overwrite_file` to work with the new single-scan approach
- [x] 5.2 Verify all existing tidy tests pass with the refactored flow
- [x] 5.3 Run full `cargo test` and `cargo clippy` to confirm no regressions
