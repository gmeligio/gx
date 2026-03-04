## Why

`gx init` hard-fails when `GITHUB_TOKEN` is not set because every GitHub API method has a blanket token guard (`self.token.as_ref().ok_or(TokenRequired)?`). However, all endpoints used are public read endpoints on the GitHub Git Data API that work without authentication at 60 req/hr. There is no reason for the tool to require a token for public GitHub Actions repositories. The error architecture also lacks distinction between recoverable errors (rate limit, private repo) and strict errors (action not found, network failure), causing all failures to be treated equally as hard errors.

## What Changes

- **Remove blanket token guards** from all `GithubRegistry` methods. Requests are made with or without the `Authorization` header depending on whether a token is available.
- **BREAKING**: Replace `GithubError::TokenRequired` with HTTP-status-based error variants (`RateLimited`, `Unauthorized`, `NotFound`).
- **BREAKING**: Replace `ResolutionError::TokenRequired` with `RateLimited` and `AuthRequired` variants. Add `is_recoverable()` classification method.
- **Update lock resolution** to only hard-fail on strict errors (not found, network). Recoverable errors (rate limit, auth required) become warnings — the lock is written as partial, and the user is told to retry.
- **Emit a one-time warning** when no `GITHUB_TOKEN` is set, informing the user of the lower rate limit.

## Capabilities

### New Capabilities
- `token-optional-api`: GitHub API requests work without a token for public repositories, with conditional auth header injection and a one-time rate-limit warning.
- `recoverable-errors`: Resolution errors are classified as recoverable (rate limit, auth required) or strict (not found, network). Lock resolution warns on recoverable errors and only hard-fails on strict ones.

### Modified Capabilities
- `sha-description`: The "Token required error propagates" scenario changes — `TokenRequired` is replaced by `AuthRequired` or `RateLimited` depending on the actual API failure. The error still propagates through `resolve_from_sha`, but the variant name changes.
- `sha-first-resolution`: The "describe_sha error propagates" scenario changes — `TokenRequired` is no longer a possible error variant. The fallback behavior in `populate_lock_entry` now classifies errors as recoverable vs strict.

## Impact

- **`src/infrastructure/github.rs`**: Major refactor — remove all token guards, add `authenticated_get` helper, reclassify HTTP status codes into specific error variants, update `VersionRegistry` impl mappings.
- **`src/domain/resolution.rs`**: Change `ResolutionError` variants, add `is_recoverable()`, simplify `correct_version` error handling.
- **`src/commands/tidy.rs`**: Refactor `update_lock` and `populate_lock_entry` to separate recoverable from strict errors.
- **`src/commands/app.rs`**: Add no-token warning at init time.
- **Test files**: Update `NoopRegistry`, `MockRegistry`, and all tests referencing `TokenRequired`.
- **Existing specs**: `sha-description` and `sha-first-resolution` spec scenarios referencing `TokenRequired` need updating.
