## 1. Create shared test infrastructure

- [x] 1.1 Create `tests/common/mod.rs` that re-exports `registries` and `setup` submodules
- [x] 1.2 Create `tests/common/registries.rs` with `FakeRegistry` (hash-based SHA, `with_all_tags()`, `with_sha_tags()` builder methods), `AuthRequiredRegistry`, `EmptyDateRegistry`, and `FailingDescribeRegistry`
- [x] 1.3 Create `tests/common/setup.rs` with shared helpers: `create_test_repo()`, `write_workflow()`, `manifest_path()`, `lock_path()`, `create_empty_manifest()`

## 2. Restructure tidy tests

- [x] 2.1 Rename `tests/tidy_test.rs` to `tests/integ_tidy.rs`
- [x] 2.2 Replace `MockRegistry` and `NoopRegistry` with imports from `common::registries` (`FakeRegistry`, `AuthRequiredRegistry`)
- [x] 2.3 Replace local `create_test_repo` and helper functions with imports from `common::setup`
- [x] 2.4 Verify all 18 tidy tests pass

## 3. Restructure upgrade tests

- [x] 3.1 Rename `tests/upgrade_test.rs` to `tests/integ_upgrade.rs`
- [x] 3.2 Replace `MockUpgradeRegistry` with `FakeRegistry` from `common::registries`
- [x] 3.3 Update assertions that use concat-based SHA values to use `FakeRegistry::fake_sha()`
- [x] 3.4 Replace local `create_test_repo`, `create_manifest`, `create_lock`, `create_workflow` with imports from `common::setup`
- [x] 3.5 Verify all 16 upgrade tests pass

## 4. Restructure lint tests

- [x] 4.1 Rename `tests/lint_test.rs` to `tests/integ_lint.rs`
- [x] 4.2 Verify all 10 lint tests pass (no registry changes needed)

## 5. Restructure repo tests

- [x] 5.1 Rename `tests/repo_test.rs` to `tests/integ_repo.rs`
- [x] 5.2 Verify both repo tests pass

## 6. Split e2e_test.rs into integ_pipeline.rs and e2e_pipeline.rs

- [x] 6.1 Create `tests/integ_pipeline.rs` with edge-case tests that require specific mock behavior: `test_init_sha_first_describe_sha_no_tags`, `test_init_sha_first_describe_sha_empty_date`, `test_init_sha_first_describe_sha_fails_falls_back_to_resolve`
- [x] 6.2 Update `integ_pipeline.rs` to use registries and helpers from `common`
- [x] 6.3 Create `tests/e2e_pipeline.rs` with remaining tests converted to use `GithubRegistry::new(token)` — tests: init creates parseable files, tidy after init is noop, tidy adds new action, tidy removes stale action, tidy override changes, upgrade preserves unaffected entries, lint detects unsynced manifest, SHA-first resolution tests, full pipeline tests
- [x] 6.4 Remove `tests/e2e_test.rs`
- [x] 6.5 Verify all integration pipeline tests pass
- [x] 6.6 Verify all e2e pipeline tests pass (with `GITHUB_TOKEN`)

## 7. Update code_health.rs

- [x] 7.1 Extend `ignore_attribute_budget` to scan both `src/` and `tests/` directories
- [x] 7.2 Set `max_ignored = 0`
- [x] 7.3 Verify the test passes (no `#[ignore]` attributes remain)

## 8. Update mise tasks and CI

- [x] 8.1 Update `.config/mise.toml`: rename `test` to run `cargo test --lib`, update `integ` to use bash glob for `integ_*.rs` files plus `code_health`, update `e2e` task to run `cargo test --test e2e_pipeline` with `GITHUB_TOKEN`
- [x] 8.2 Update `.github/workflows/build.yml`: rename jobs to `unit-tests`, `integration-tests`, `e2e-tests`, each running the corresponding mise task

## 9. Final verification

- [x] 9.1 Run `mise run test` — all unit tests pass
- [x] 9.2 Run `mise run integ` — all integration tests pass
- [x] 9.3 Run `mise run clippy` — no clippy warnings
- [x] 9.4 Verify no `#[ignore]` attributes remain in codebase
