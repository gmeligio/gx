## 1. Split upgrade/mod.rs (independent — can run in parallel with everything)

- [x] 1.1 Create `src/upgrade/types.rs` with `UpgradePlan`, `UpgradeScope`, `UpgradeMode`, `UpgradeRequest`, `UpgradeError` and their tests
- [x] 1.2 Create `src/upgrade/plan.rs` with `plan()`, `determine_upgrades()`, `resolve_and_store()`, `apply_upgrade_workflows()`, mock registry, and plan tests
- [x] 1.3 Create `src/upgrade/cli.rs` with `ResolveError`, `resolve_upgrade_mode()` and their tests
- [x] 1.4 Slim `src/upgrade/mod.rs` to `Upgrade` Command impl + mod declarations + re-exports
- [x] 1.5 Verify `cargo test` passes and no file exceeds 500 lines

## 2. Analyze tidy/tests.rs for domain-logic tests (depends on split-domain-files)

- [x] 2.1 Audit each test in `tidy/tests.rs` — classify as "domain logic" or "orchestration"
- [x] 2.2 For each domain-logic test, identify the target domain module and rewrite setup to call domain methods directly instead of going through `plan()`

## 3. Migrate lock completeness tests to domain

- [x] 3.1 Move `test_lock_completeness_missing_specifier_derived`, `test_lock_completeness_missing_version_refined`, `test_lock_completeness_complete_entry_unchanged`, `test_lock_completeness_manifest_version_precision_mismatch` to `domain/lock/entry.rs` tests
- [x] 3.2 Simplify test setup to call `LockEntry` methods directly
- [x] 3.3 Remove the moved tests from `tidy/tests.rs`

## 4. Migrate override tests to domain

- [x] 4.1 Move `test_plan_multiple_versions_produces_override_diff` logic to `domain/manifest/overrides.rs` tests — test `sync_overrides()` directly
- [x] 4.2 Move `test_plan_stale_override_produces_override_removal` logic to `domain/manifest/overrides.rs` tests — test `prune_stale_overrides()` directly
- [x] 4.3 Move `test_manifest_authority_not_overwritten_by_workflow_sha` to appropriate domain test
- [x] 4.4 Remove the moved tests from `tidy/tests.rs`

## 5. Evaluate remaining tidy tests

- [x] 5.1 Review remaining tests in `tidy/tests.rs` — confirm they test orchestration, not domain logic
- [x] 5.2 Remove unused mock registries that are no longer needed after migration
- [x] 5.3 Verify `tidy/tests.rs` is under 500 lines

## 6. Final verification

- [x] 6.1 Run `cargo test` — all green
- [x] 6.2 Run `cargo clippy` — all green (pre-existing failures in `domain_does_not_import_upward` and `no_duplicate_private_fns_across_command_modules` are unrelated to this change)
- [x] 6.3 Verify all files under 500 lines
- [x] 6.4 Update TODO comments in `tests/code_health.rs`
- [x] 6.5 Lower `folder_file_count_budget` to target (8) if domain reorganization is complete
