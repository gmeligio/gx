## Context

Currently, `GithubRegistry` guards every private HTTP method with `self.token.as_ref().ok_or(GithubError::TokenRequired)?`, making a token mandatory for any API interaction. All endpoints used (`/repos/{o}/{r}/git/ref/...`, `/repos/{o}/{r}/commits/...`, `/repos/{o}/{r}/releases/tags/...`, etc.) are public read endpoints on the GitHub API that work without authentication for public repositories — the only difference is the rate limit (60 req/hr unauthenticated vs 5,000/hr authenticated).

The error architecture collapses all resolution failures into a single `TidyError::ResolutionFailed` with no distinction between recoverable and strict errors.

## Goals / Non-Goals

**Goals:**
- Make all GitHub API calls work without a token for public repositories
- Classify API errors by recoverability so callers can degrade gracefully
- Allow `gx init` and `gx tidy` to produce partial results when some resolutions fail recoverably
- Inform users about rate limit implications when no token is set

**Non-Goals:**
- Private repository support improvements (works if token is set, warns if not — same as today)
- Rate limit retry/backoff logic (future enhancement)
- Caching API responses to reduce request count

## Decisions

### 1. Conditional auth header via `authenticated_get` helper

Add a method to `GithubRegistry`:

```rust
fn authenticated_get(&self, url: &str) -> reqwest::blocking::RequestBuilder {
    let req = self.client.get(url);
    match &self.token {
        Some(token) => req.header("Authorization", format!("Bearer {token}")),
        None => req,
    }
}
```

All private HTTP methods (`fetch_ref`, `fetch_commit_sha`, `get_tags_for_sha`, `get_version_tags`, `fetch_commit_date`, `fetch_release_date`, `fetch_tag_date`) replace their token guard + manual header with `self.authenticated_get(url)`.

For `dereference_tag`, which currently takes `token: &str` as a parameter: change it to use `self.authenticated_get` directly, removing the `token` parameter from its signature.

**Why not a default header on the client?** The client is built once in `new()`. Using per-request conditional headers keeps the token concern local to the helper and avoids rebuilding the client.

### 2. HTTP status classification in `GithubError`

Replace the catch-all `ApiStatus { status, url }` and `TokenRequired` with specific variants:

```rust
pub enum GithubError {
    ClientInit(reqwest::Error),
    Request { operation, url, source },
    RateLimited { url: String },
    Unauthorized { url: String },
    NotFound { url: String },
    ApiError { status: u16, url: String },
    ParseResponse { url, source },
}
```

Add a `check_status` helper that classifies HTTP responses:
- 429 → `RateLimited`
- 403 with `x-ratelimit-remaining: 0` → `RateLimited`
- 401 or 403 (otherwise) → `Unauthorized`
- 404 → `NotFound`
- Other non-2xx → `ApiError`

Each existing method's `if !response.status().is_success()` block is replaced by a call to `check_status`.

**Why `Unauthorized` not `AuthRequired`?** At the HTTP layer, 401/403 means unauthorized. The semantic "auth required" interpretation happens at the `ResolutionError` level.

### 3. Two-tier error classification in `ResolutionError`

```rust
pub enum ResolutionError {
    ResolveFailed { spec, reason },
    NoTagsForSha { action, sha },
    RateLimited,
    AuthRequired,
}
```

With a classification method:

```rust
impl ResolutionError {
    pub fn is_recoverable(&self) -> bool {
        matches!(self, Self::RateLimited | Self::AuthRequired)
    }
}
```

The `VersionRegistry` impl maps:
- `GithubError::RateLimited` → `ResolutionError::RateLimited`
- `GithubError::Unauthorized` → `ResolutionError::AuthRequired`
- All others → `ResolutionError::ResolveFailed` (or `NoTagsForSha`)

### 4. `update_lock` separates recoverable from strict errors

`populate_lock_entry` changes from mutating `unresolved: &mut Vec<String>` to returning `Result<(), ResolutionError>`. The caller (`update_lock`) classifies:

```
for each spec:
    match populate_lock_entry(...) {
        Ok(()) => continue,
        Err(e) if e.is_recoverable() => warn, increment counter
        Err(e) => push to strict_errors
    }

if recoverable_count > 0:
    warn summary with "run gx tidy again to retry"

if strict_errors non-empty:
    return TidyError::ResolutionFailed (strict only)
```

### 5. One-time no-token warning at command level

In `commands/app.rs`, before calling `tidy::plan`, check `config.settings.github_token.is_none()` and emit:

```
[WARN] No GITHUB_TOKEN set — using unauthenticated GitHub API (60 requests/hour limit).
```

This is a single warning at the command entry point, not repeated per-request.

## Risks / Trade-offs

- **60 req/hr limit**: A repo with ~10 actions triggers ~30 API calls during init. This fits within the limit for a single run, but rapid successive runs or large repos (~20+ actions) could hit it. → Mitigation: the warning tells users to set a token; rate-limited actions degrade to warnings rather than hard failure.
- **GitHub may change unauthenticated access**: Unlikely for public repos, but possible. → Mitigation: the error classification handles 401/403 gracefully regardless.
- **`NotFound` vs "not a tag"**: A 404 from `fetch_ref` for a tag probe isn't a real error — it means "try branch next". The existing code already handles this by falling through in `resolve_ref`. The new `NotFound` variant flows through the same logic since `resolve_ref` matches on `Err(_)` generically.
