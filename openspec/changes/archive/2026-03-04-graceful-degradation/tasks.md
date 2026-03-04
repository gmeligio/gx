## 1. GithubError Refactor

- [x] 1.1 Replace `GithubError::TokenRequired` and `GithubError::ApiStatus` with `RateLimited`, `Unauthorized`, `NotFound`, and `ApiError` variants in `src/infrastructure/github.rs`
- [x] 1.2 Add `check_status` helper method to `GithubRegistry` that classifies HTTP responses by status code (429, 403+rate-limit-exhausted → RateLimited, 401/403 → Unauthorized, 404 → NotFound, other → ApiError)
- [x] 1.3 Add `authenticated_get` helper method to `GithubRegistry` that conditionally attaches the `Authorization` header based on `self.token`

## 2. Remove Token Guards

- [x] 2.1 Refactor `fetch_ref` to use `authenticated_get` and `check_status` instead of token guard and manual status check
- [x] 2.2 Refactor `fetch_commit_sha` to use `authenticated_get` and `check_status`
- [x] 2.3 Refactor `get_tags_for_sha` to use `authenticated_get` and `check_status`
- [x] 2.4 Refactor `dereference_tag` to remove `token: &str` parameter and use `self.authenticated_get` instead
- [x] 2.5 Refactor `get_version_tags` to use `authenticated_get` and `check_status`
- [x] 2.6 Refactor `fetch_commit_date` to use `authenticated_get` and `check_status`
- [x] 2.7 Refactor `fetch_release_date` to use `authenticated_get` and `check_status`
- [x] 2.8 Refactor `fetch_tag_date` to use `authenticated_get` and `check_status`

## 3. ResolutionError Refactor

- [x] 3.1 Replace `ResolutionError::TokenRequired` with `RateLimited` and `AuthRequired` variants in `src/domain/resolution.rs`
- [x] 3.2 Add `is_recoverable()` method to `ResolutionError`
- [x] 3.3 Update `VersionRegistry` impl error mappings in `src/infrastructure/github.rs` to map `RateLimited` → `RateLimited`, `Unauthorized` → `AuthRequired`, others → `ResolveFailed`/`NoTagsForSha`
- [x] 3.4 Simplify `correct_version` in `src/domain/resolution.rs` — remove `TokenRequired`-specific match arm, use generic warning for all errors

## 4. Lock Resolution Graceful Degradation

- [x] 4.1 Change `populate_lock_entry` in `src/commands/tidy.rs` to return `Result<(), ResolutionError>` instead of mutating `unresolved: &mut Vec<String>`
- [x] 4.2 Update `update_lock` to classify errors: warn+skip recoverable, collect strict into `unresolved`
- [x] 4.3 Add summary warning when recoverable errors occurred ("N action(s) skipped... run `gx tidy` again to retry")

## 5. Command-Level Warning

- [x] 5.1 Add one-time no-token warning in `src/commands/app.rs` init and tidy entry points when `config.settings.github_token.is_none()`

## 6. Update Tests

- [x] 6.1 Update `NoopRegistry` in `src/commands/tidy.rs` and `tests/tidy_test.rs` to return `AuthRequired` instead of `TokenRequired`
- [x] 6.2 Update `MockRegistry` in `src/domain/resolution.rs` tests for new error variants
- [x] 6.3 Update unit tests in `src/infrastructure/github.rs` that reference `TokenRequired`
- [x] 6.4 Update e2e tests in `tests/e2e_test.rs` if any reference `TokenRequired`
- [x] 6.5 Add test for `is_recoverable()` classification
- [x] 6.6 Add test for `update_lock` with mixed recoverable/strict errors
- [x] 6.7 Run `cargo test` and `cargo clippy` to verify all changes

## 7. Update Existing Specs

- [x] 7.1 Update `openspec/specs/sha-description/spec.md` — replace "Token required error propagates" scenario with auth/rate-limit variants
- [x] 7.2 Update `openspec/specs/sha-first-resolution/spec.md` — replace `TokenRequired` references with new error variants
