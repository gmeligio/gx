## 1. Domain Types and Trait

- [x] 1.1 Add `ShaDescription` struct to `src/domain/resolution.rs` with fields `tags: Vec<Version>`, `repository: String`, `date: String`
- [x] 1.2 Add `describe_sha(&self, id: &ActionId, sha: &CommitSha) -> Result<ShaDescription, ResolutionError>` to `VersionRegistry` trait
- [x] 1.3 Export `ShaDescription` from `src/domain/mod.rs`

## 2. Resolver Update

- [x] 2.1 Rewrite `ActionResolver::resolve_from_sha` to call `describe_sha` instead of `lookup_sha` + `tags_for_sha`

## 3. GithubRegistry Implementation

- [x] 3.1 Implement `describe_sha` for `GithubRegistry` in `src/infrastructure/github.rs`: call `fetch_commit_date` directly + `get_tags_for_sha` with non-fatal tag errors

## 4. Test Mock Implementations

- [x] 4.1 Add `describe_sha` to `MockRegistry` in `src/domain/resolution.rs` tests
- [x] 4.2 Add `describe_sha` to `NoopRegistry` in `tests/tidy_test.rs` (returns `TokenRequired`)
- [x] 4.3 Add `describe_sha` to `MockRegistry` in `tests/tidy_test.rs` (uses `sha_tags` map)
- [x] 4.4 Add `describe_sha` to `E2eRegistry` in `tests/e2e_test.rs` (empty tags + fake metadata)
- [x] 4.5 Add `describe_sha` to `ShaAwareRegistry` in `tests/e2e_test.rs` (uses `sha_tags` map)
- [x] 4.6 Add `describe_sha` to `MockUpgradeRegistry` in `tests/upgrade_test.rs`
- [x] 4.7 Add `describe_sha` to `MockPlanRegistry` in `src/commands/upgrade.rs` tests

## 5. Unit Tests

- [x] 5.1 Update `test_resolve_from_sha_with_tags` in `src/domain/resolution.rs` to exercise the `describe_sha` path
- [x] 5.2 Update `test_resolve_from_sha_no_tags` in `src/domain/resolution.rs` for the new path
- [x] 5.3 Add `test_resolve_from_sha_describe_error_propagates` — verify `describe_sha` errors bubble up

## 6. Integration and E2E Tests

- [x] 6.1 Verify all existing tidy_test.rs SHA-first tests pass unchanged
- [x] 6.2 Verify all existing e2e_test.rs SHA-first tests pass unchanged
- [x] 6.3 Add `test_init_sha_first_describe_sha_no_tags` in e2e_test.rs — init with SHA-pinned action where `describe_sha` returns no tags, SHA used as version

## 7. Verification

- [x] 7.1 Run `cargo test` — all tests pass
- [x] 7.2 Run `cargo clippy` — no warnings
