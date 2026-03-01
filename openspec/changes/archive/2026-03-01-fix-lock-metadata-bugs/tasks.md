# Tasks: Fix lock entry metadata bugs

## 1. Fix CommitDetailResponse nesting (Bug #2)

- [x] 1.1 Add `CommitObject` wrapper struct with `committer: Option<CommitterInfo>` field
- [x] 1.2 Change `CommitDetailResponse` to have `commit: CommitObject` instead of `committer: Option<CommitterInfo>`
- [x] 1.3 Update `fetch_commit_date` to navigate `response.commit.committer.and_then(|c| c.date)`

**Files**: `crates/gx-lib/src/infrastructure/github.rs`

## 2. Add Release detection in resolve_ref (Bug #3)

- [x] 2.1 After tag resolution succeeds in `resolve_ref`, call `fetch_release_date(base_repo, ref_name)` — if it returns `Ok(Some(_))`, return `RefType::Release` instead of `RefType::Tag`
- [x] 2.2 Add unit test: `test_resolve_ref_returns_release_for_tag_with_release` (requires GITHUB_TOKEN, mark `#[ignore]`)

**Files**: `crates/gx-lib/src/infrastructure/github.rs`

## 3. Replace validate_and_correct with correct_version (Bug #1)

- [x] 3.1 Add `correct_version(id, sha) -> (Version, bool)` method to `ActionResolver` — calls `tags_for_sha`, returns `(best_version, was_corrected)`, falls back to original version on error
- [x] 3.2 Add `with_sha(sha: CommitSha) -> Self` method to `ResolvedAction`
- [x] 3.3 Remove `validate_and_correct` from `ActionResolver`
- [x] 3.4 Update unit tests in `resolution.rs`: replace `test_validate_version_matches` and `test_validate_version_corrected` with `correct_version` tests

**Files**: `crates/gx-lib/src/domain/resolution.rs`, `crates/gx-lib/src/domain/action.rs`

## 4. Update tidy to use two-step flow

- [x] 4.1 Rewrite the SHA-pinned branch in `update_lock`: call `correct_version`, then `resolve`, then `with_sha`
- [x] 4.2 Update `NoopRegistry` mock in tidy.rs tests to handle new call patterns
- [x] 4.3 Update integration tests in `tidy_test.rs` — mock registries must implement `lookup_sha` returning proper metadata

**Files**: `crates/gx-lib/src/commands/tidy.rs`, `crates/gx-lib/tests/tidy_test.rs`

## 5. Verification

- [x] 5.1 Run `cargo test` — all tests pass
- [x] 5.2 Run `cargo clippy` — no warnings
- [x] 5.3 Manual test: `GITHUB_TOKEN=$(gh auth token) cargo run -- tidy` on the gx repo — verify lock entries have correct ref_type and non-empty date
